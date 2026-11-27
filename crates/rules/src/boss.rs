//! 마지막 밤의 보스입니다. 보스의 몸은 `World::monsters`에 있는 종류 `MonsterKind::Boss`의 몬스터이고, 보스가 무엇을 노리고 지금 무엇을 하는지는
//! `World::boss`의 `Boss`가 가집니다. 몸은 맞고 걷고 효과를 받는 일을 다른 몬스터와 같은 규칙으로 하므로, 무기와 포탑과 효과와 스냅샷이 보스를 따로 알지 않습니다.
//! 이 파일이 정하는 것은 보스의 대상, 행동의 순서, 예고된 공격입니다.
//!
//! 보스는 걷고, 때가 되면 행동 하나를 합니다. 행동은 순서(`BossTuning::script`)대로 되풀이합니다.
//!
//! ```text
//!   Walking --(걷는 시간이 끝나고 대상이 `target_range` 안에 있음)--> 순서의 다음 행동
//!     ^                                                        |
//!     |        Slam -> Slamming(예고) --(시간이 지남)--> 원 안에 피해
//!     +----------------------------------(Walking으로 돌아가고 순서가 한 칸 나아감)
//! ```
//!
//! 보스는 걷는 동안에만 걷고, 예고하는 동안에는 걷는 일을 하지 않습니다(`Boss::walk_aim`이 `None`입니다).
//! 대상은 Tick마다 다시 정합니다. 보스에게서 `target_range` 안에 있는 살아 있고 자리에 있는 플레이어 가운데 최근 `threat_secs` 동안
//! 보스에게 가장 많은 피해를 준 플레이어이고, 그런 플레이어가 없으면 마차입니다. 포탑이 준 피해는 세지 않습니다.
//!
//! 한 Tick에서 `act`는 포탑의 사격 뒤에, 몬스터의 이동 앞에 있습니다. 보스의 몸과 걷는 방향이 이 Tick에 정해진 행동을 따라야 하고,
//! 플레이어의 이동이 끝난 자리에서 예고가 터져야 하며(예고가 터지는 Tick에 영역 밖으로 걸어 나간 플레이어는 맞지 않습니다),
//! 이번 Tick의 피해는 아직 적용되지 않아서 대상을 정하는 데 쓰이지 않기 때문입니다.

use std::collections::VecDeque;

use crate::ids::{MonsterId, PlayerId};
use crate::math::Vec2;
use crate::spawn::SpawnPlan;
use crate::telegraph::{self, Telegraph, Zone};
use crate::tuning::{BossTuning, Finale, Move, Tuning};
use crate::world::{Ctx, Hit, World};

/// 보스가 노리는 대상입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    Player(PlayerId),
    Wagon,
}

/// 보스가 지금 하고 있는 일입니다. 일마다 그 일에서만 의미 있는 값을 가집니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// 대상을 향해 걷습니다. `ticks_left`가 0이 되고 대상이 `target_range` 안에 있으면 다음 행동을 시작합니다.
    /// 대상이 멀면 0에 머물면서 계속 걷습니다.
    Walking { ticks_left: u32 },
    /// 내려찍기를 예고하는 중입니다. 보스는 서 있고, 영역은 예고가 시작될 때 대상이 서 있던 자리의 원입니다.
    Slamming(Telegraph),
}

/// 최근에 플레이어가 보스에게 준 피해입니다. 플레이어마다 따로 모으지 않고 피해 하나하나를 시간순으로 적으므로, 오래된 피해가 빠지는 때가 Tick 단위로 정확합니다.
#[derive(Clone, Debug, PartialEq)]
struct Threat {
    /// 보스가 지금까지 진행한 Tick의 수입니다. 피해를 적을 때의 시각입니다.
    now: u32,
    window_ticks: u32,
    /// 시각, 피해를 준 플레이어, 피해입니다. 오래된 것이 앞에 있습니다.
    entries: VecDeque<(u32, PlayerId, u32)>,
}

impl Threat {
    /// 한 Tick 지나가고, 되돌아보는 시간보다 오래된 피해를 버립니다.
    fn advance(&mut self) {
        self.now += 1;
        while self
            .entries
            .front()
            .is_some_and(|&(at, _, _)| self.now - at > self.window_ticks)
        {
            self.entries.pop_front();
        }
    }

    /// 지금 시각에 `eligible`을 만족하는 플레이어 가운데 최근에 가장 많은 피해를 준 플레이어입니다. 피해를 준 적이 없으면 `None`입니다.
    /// 같은 값이면 지금 노리고 있는 플레이어(`holder`)가 그대로 노리고, 노리는 플레이어가 그 가운데 없으면 번호가 작은 플레이어입니다.
    /// 그래서 더 세게 때린 플레이어만 시선을 넘겨받습니다.
    fn top(
        &self,
        eligible: impl Fn(PlayerId) -> bool,
        holder: Option<PlayerId>,
    ) -> Option<PlayerId> {
        let mut totals: Vec<(PlayerId, u32)> = Vec::new();
        for &(_, by, amount) in &self.entries {
            match totals.iter_mut().find(|(id, _)| *id == by) {
                Some((_, total)) => *total += amount,
                None => totals.push((by, amount)),
            }
        }
        totals.retain(|(id, _)| eligible(*id));
        let best = totals.iter().map(|&(_, total)| total).max()?;
        let tied = || {
            totals
                .iter()
                .filter(move |&&(_, total)| total == best)
                .map(|&(id, _)| id)
        };
        let lowest = tied().min()?;
        Some(
            holder
                .filter(|holder| tied().any(|id| id == *holder))
                .unwrap_or(lowest),
        )
    }
}

/// 보스의 상태입니다. 보스의 몸이 사라지면 함께 사라집니다(`World::apply_hits`).
#[derive(Clone, Debug, PartialEq)]
pub struct Boss {
    /// 보스의 몸인 몬스터의 번호입니다.
    pub id: MonsterId,
    /// 이 밤의 인원에 맞춰 정해진 최대 HP입니다. 몸의 HP가 이 값 이하입니다.
    pub max_hp: u32,
    /// 지금 노리는 대상입니다. 행동을 하는 동안에도 Tick마다 다시 정해지므로 화면이 누가 노려지는지 언제나 알 수 있습니다.
    pub aim: Aim,
    pub act: Act,
    /// 순서에서 지금 하고 있거나 다음에 할 행동의 자리입니다.
    step: usize,
    threat: Threat,
}

impl Boss {
    /// 방에 플레이어가 `party`명 있는 밤에 막 나타난 보스입니다. 최대 HP가 인원에 비례합니다. 마차를 노리고, 걷는 시간부터 시작합니다.
    pub(crate) fn new(id: MonsterId, party: u32, cfg: &BossTuning, t: &Tuning) -> Boss {
        Boss {
            id,
            max_hp: t.nights.scaled(t.monsters.boss.max_hp, party),
            aim: Aim::Wagon,
            act: Act::Walking {
                ticks_left: t.ticks(cfg.rest_secs),
            },
            step: 0,
            threat: Threat {
                now: 0,
                window_ticks: t.ticks(cfg.threat_secs),
                entries: VecDeque::new(),
            },
        }
    }

    /// 플레이어 `by`가 보스에게 `amount`의 피해를 준 것을 적습니다. 0은 적지 않습니다.
    pub(crate) fn record(&mut self, by: PlayerId, amount: u32) {
        if amount > 0 {
            self.threat.entries.push_back((self.threat.now, by, amount));
        }
    }

    /// 보스가 걷는 중이면 걸어가는 대상이고, 서 있어야 하면 `None`입니다.
    pub(crate) fn walk_aim(&self) -> Option<Aim> {
        matches!(self.act, Act::Walking { .. }).then_some(self.aim)
    }

    /// 한 행동이 끝났습니다. 순서가 한 칸 나아가고 걷는 시간이 시작됩니다.
    fn finish(&mut self, cfg: &BossTuning, t: &Tuning) -> Act {
        self.step = (self.step + 1) % cfg.script.len();
        Act::Walking {
            ticks_left: t.ticks(cfg.rest_secs),
        }
    }
}

/// 보스가 나타날 자리입니다. 밤의 무리가 나타나는 자리 가운데 마지막 무리 다음 순번의 방향에서 첫째 자리입니다.
pub(crate) fn arrival_spot(spawns: &SpawnPlan, t: &Tuning) -> Vec2 {
    spawns.night_spot(t.nights.wave_total(), 0)
}

/// Tick마다 보스의 한 걸음입니다. 대상을 다시 정하고, 하고 있는 일을 한 Tick 진행하고, 끝나는 일이 있으면 피해를 `hits`에 더합니다.
/// 보스가 없거나 마지막 밤의 끝이 보스가 아니면 아무것도 하지 않습니다.
pub(crate) fn act(world: &mut World, ctx: &Ctx, hits: &mut Vec<Hit>) {
    let Finale::Boss(cfg) = &ctx.t.nights.finale else {
        return;
    };
    let Some(mut boss) = world.boss.take() else {
        return;
    };
    if let Ok(index) = world.monsters.binary_search_by_key(&boss.id, |m| m.id) {
        step(world, index, &mut boss, cfg, ctx.t, hits);
    }
    world.boss = Some(boss);
}

/// `index`번 몬스터인 보스의 몸이 이번 Tick에 하는 일입니다.
fn step(
    world: &mut World,
    index: usize,
    boss: &mut Boss,
    cfg: &BossTuning,
    t: &Tuning,
    hits: &mut Vec<Hit>,
) {
    boss.threat.advance();
    boss.aim = choose_aim(world, boss, world.monsters[index].pos, cfg);

    boss.act = match boss.act {
        Act::Walking { ticks_left } => {
            let ticks_left = ticks_left.saturating_sub(1);
            let pos = world.monsters[index].pos;
            if ticks_left == 0 && gap_to(world, boss.aim, pos) <= cfg.target_range {
                start(world, index, boss, cfg, t)
            } else {
                Act::Walking { ticks_left }
            }
        }
        Act::Slamming(mut telegraph) => {
            if telegraph.tick() {
                telegraph::strike(world, &telegraph.zone, &cfg.slam.strike, hits);
                boss.finish(cfg, t)
            } else {
                Act::Slamming(telegraph)
            }
        }
    };
}

/// 보스가 이번 Tick에 노릴 대상입니다.
fn choose_aim(world: &World, boss: &Boss, pos: Vec2, cfg: &BossTuning) -> Aim {
    let eligible = |id: PlayerId| {
        world.players.get(&id).is_some_and(|player| {
            player.is_targetable() && player.pos.distance(pos) <= cfg.target_range
        })
    };
    let holder = match boss.aim {
        Aim::Player(id) => Some(id),
        Aim::Wagon => None,
    };
    boss.threat
        .top(eligible, holder)
        .map_or(Aim::Wagon, Aim::Player)
}

/// 보스의 중심 `pos`에서 대상까지의 거리입니다. 플레이어는 중심까지, 마차는 가장자리까지 잽니다. 사라진 플레이어는 무한히 멉니다.
fn gap_to(world: &World, aim: Aim, pos: Vec2) -> f32 {
    match aim {
        Aim::Player(id) => world
            .players
            .get(&id)
            .map_or(f32::INFINITY, |player| player.pos.distance(pos)),
        Aim::Wagon => world.wagon.rect.distance_to(pos),
    }
}

/// 대상을 겨냥하는 점입니다. 플레이어는 중심이고 마차는 사각형의 중심입니다.
fn point_of(world: &World, aim: Aim) -> Vec2 {
    match aim {
        Aim::Player(id) => world
            .players
            .get(&id)
            .map_or(world.wagon.rect.center(), |player| player.pos),
        Aim::Wagon => world.wagon.rect.center(),
    }
}

/// 순서의 다음 행동을 시작합니다. 영역은 지금 정하고 그 뒤에는 바꾸지 않으므로, 플레이어는 예고를 보고 피할 수 있습니다.
/// 보스는 겨냥한 쪽을 바라봅니다. `match`에 `_` 갈래가 없으므로 행동을 추가하면 여기서 컴파일 오류가 납니다.
fn start(world: &mut World, index: usize, boss: &Boss, cfg: &BossTuning, t: &Tuning) -> Act {
    let pos = world.monsters[index].pos;
    let point = point_of(world, boss.aim);
    let toward = (point - pos).normalize_or_zero();
    if toward != Vec2::ZERO {
        world.monsters[index].facing = toward;
    }
    match cfg.script[boss.step] {
        Move::Slam => Act::Slamming(Telegraph::new(
            Zone::Circle {
                center: point,
                radius: cfg.slam.radius,
            },
            t.ticks(cfg.slam.windup_secs),
        )),
    }
}
