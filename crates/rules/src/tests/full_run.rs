//! 모의 원정: 기본 수치의 원정 하나를 야영지에서 준비하는 것부터 보스를 잡거나 실패할 때까지 한 번에 이어서 돕니다.
//!
//! 모의 전투(`raid`)와 달리 밤마다 마차의 HP와 포탑을 되돌리지 않습니다. 낮에 모은 자원으로 지은 감시탑의 수, 밤에 상한 마차와 포탑이 다음 날로 이어지므로,
//! 인원이 적은 방이 낮 동안 감시탑을 몇 개 세우는지와 그 감시탑으로 둘째 밤과 셋째 밤과 보스를 넘기는지를 한꺼번에 봅니다.
//! 입력은 `Sim`의 공개 입력(`PlayerInput`)뿐이고 상태를 직접 고치지 않습니다. 타일 지도는 길을 찾으려고 읽기만 합니다(`Sim::tiles`).
//!
//! 플레이어(`Hand`)가 하는 일은 다음과 같습니다. 모두 검 하나로 시작하고 상자는 열지 않으며, 낮의 몬스터는 기본 수치 그대로 있습니다.
//! 길을 걸을 때는 달리기 게이지가 남아 있으면 달리고, 길은 타일 지도를 보고 찾습니다.
//! - 낮: 곁에 온 몬스터와 가까운(4타일 안) 몬스터를 처치합니다. 다치면 마차 곁으로 물러나 회복합니다. 마차가 많이 상했으면 수리하고,
//!   보관함에 감시탑 하나의 비용이 있으면 마차에 가까운 자리부터 감시탑을 세우고, 아니면 채집 지점에서 자원을 들고 와서 마차에 넣습니다.
//!   낮이 끝나기 전에 마차로 돌아오고, 할 일이 없으면 상한 포탑과 마차를 수리합니다. 포탑을 수리하는 것은 새로 짓는 것보다 시간이 훨씬 걸리므로 맨 뒤입니다.
//! - 밤: 마차를 지키며(`guard_the_wagon`) 쓰러진 동료를 부활시키고, 곁에 몬스터가 셋 이상 몰리면 그 무리에게서 달려서 벗어나고(`evade`), 몬스터가 없는 동안에는 수리합니다.
//! - 보스의 밤: 모의 전투와 같은 일을 합니다(`Bot`). 방에 둘 이상이 있으면 한 명이 마차를 지키고 나머지는 보스를 때리고, 한 명뿐이면 보스를 때립니다.
//!
//! 이 입력은 사람이 하는 플레이의 대표값이 아닙니다. 상자를 열지 않고, 무기와 포탑 종류를 바꾸지 않고, 포탑을 놓는 자리를 마차에 가까운 순서로만 고릅니다.
//! 위험을 피하는 일은 HP가 낮으면 물러나는 것과 몬스터가 몰리면 달려서 벗어나는 것 둘뿐입니다.
//! 수치가 어느 크기인지를 비교하는 자이고, 같은 입력으로 수치만 바꾸어 견줄 때만 뜻이 있습니다.
//!
//! 이 파일이 단언하는 것은 1명의 방이 기본 수치로 끝까지 이기는 것(`a_lone_player_...`)과 진 구간이 넘기지 못한 것으로 적히는 것(`a_journey_that_loses_...`) 둘뿐이고, 나머지는 출력입니다. 모든 인원과 수치를 돌리는 출력 시험은 몇 분이 걸려서 `#[ignore]`이므로 다음 명령으로 돌립니다.
//!
//! ```text
//! cargo test --release -p caravan_rules full_run -- --ignored --nocapture --test-threads=1
//! ```

use std::collections::{BTreeSet, VecDeque};

use super::raid::{
    Bot, Role, guard_the_wagon, is_minion, revive_input, swing_at_minion_beside, walking,
};
use super::support::nights;
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::{NodeId, PlayerId};
use crate::input::{AimDir, PlayerInput, PlayerInputs};
use crate::kind::MonsterKind;
use crate::loot::Seed;
use crate::math::{Rect, Vec2};
use crate::nav::FlowField;
use crate::player::{Interaction, Player};
use crate::resources::GatherNode;
use crate::sim::Sim;
use crate::tuning::{Finale, Tuning};
use crate::world::Tile;

/// 원정이 이 시간 안에 끝나지 않으면 거기서 멈추고 끝나지 않은 것으로 셉니다. 기본 수치의 원정은 20분을 넘기지 않습니다.
const LIMIT_SECS: f32 = 1800.0;

/// 낮의 플레이어가 HP가 최대 HP의 이 비율(%) 아래로 내려가면 마차 곁으로 물러납니다.
const RETREAT_BELOW: u32 = 45;

/// 물러난 플레이어가 HP가 최대 HP의 이 비율(%) 이상으로 돌아오면 다시 나섭니다.
const RETURN_AT: u32 = 90;

/// 마차의 HP가 최대 HP의 이 비율(%) 아래이면 낮의 플레이어가 다른 일보다 먼저 마차를 수리합니다.
const WAGON_WORN: u32 = 70;

/// 낮이 끝나기 이 시간(초) 전부터는 채집하지 않고 마차로 돌아옵니다. 가장 먼 채집 지점에서 마차까지 걷는 시간(약 6초)보다 깁니다.
const HOME_SECS: f32 = 8.0;

/// 달리기 게이지가 이 비율(%) 이상 남아 있으면 길을 걸을 때 달립니다.
const RUN_FROM: u8 = 5;

/// 낮의 플레이어가 이 거리(타일) 안의 몬스터에게는 다가가서 처치합니다. 낮의 몬스터는 처치하지 않으면 밤에 마차로 몰려오기 때문입니다.
const HUNT_REACH: f32 = 4.0;

/// 이 거리(타일) 안의 몬스터를 곁에 몰린 몬스터로 셉니다.
const CROWD_RADIUS: f32 = 2.2;

/// 곁에 몰린 몬스터가 이만큼 이상이면 그 무리에게서 달려서 벗어납니다. HP가 절반보다 적으면 이보다 하나 적을 때도 벗어납니다.
const CROWD_SIZE: usize = 3;

/// 벗어나는 방향을 고를 때 마차에서 이보다 멀어지는 자리는 고르지 않습니다(마차의 가장자리에서 잰 거리, 타일).
const EVADE_RANGE: f32 = 12.0;

/// 곁에 있는 사각형에 닿는 거리에서 이만큼(타일) 안쪽까지 다가가서 섭니다. 경계에 서면 한 걸음 차이로 닿지 않기 때문입니다.
const SLACK: f32 = 0.2;

/// 모의 원정의 수치입니다. 이름은 출력에 씁니다.
#[derive(Clone)]
struct Numbers {
    name: String,
    tuning: Tuning,
}

impl Numbers {
    /// 같은 수치에서 마을에 세울 수 있는 포탑의 수를 `limit`으로 줄인 수치입니다. 낮에 포탑을 세울 자원이나 시간이 모자란 방을 흉내 냅니다.
    fn with_tower_limit(&self, limit: u32) -> Numbers {
        let mut tuning = self.tuning.clone();
        tuning.village.tower_limit = limit;
        Numbers {
            name: format!("{}, at most {limit} towers", self.name),
            tuning,
        }
    }

    /// 같은 수치에서 파괴병이 포탑이 준 피해의 몇 %를 받는지를 `percent`로 바꾼 수치입니다.
    fn with_sapper_tower_damage_percent(&self, percent: u32) -> Numbers {
        let mut tuning = self.tuning.clone();
        tuning.monsters.sapper.resist.tower_damage_percent = percent;
        Numbers {
            name: format!("{}, sappers take {percent}% of a tower hit", self.name),
            tuning,
        }
    }
}

/// 기본 수치 그대로입니다.
fn as_shipped() -> Numbers {
    Numbers {
        name: "as shipped".to_string(),
        tuning: Tuning::default(),
    }
}

/// 돌격병이 포탑의 피해를 모두 받는 수치입니다. 그 밖에는 기본 수치와 같습니다.
fn runners_take_every_tower_hit() -> Numbers {
    let mut tuning = Tuning::default();
    tuning.monsters.runner.resist.tower_damage_percent = 100;
    Numbers {
        name: "runners take 100% of a tower hit".to_string(),
        tuning,
    }
}

/// 돌격병과 파괴병이 없는 무리입니다. 밤마다 무리의 크기는 기본 수치와 같은 합이고 모두 기본 몬스터이며, 보스의 졸개도 기본 몬스터입니다.
fn grunts_only() -> Numbers {
    let mut tuning = Tuning::default();
    tuning.nights.plans = nights(&[&[8, 12], &[20, 30, 40], &[60, 90, 150]]);
    if let Finale::Boss(boss) = &mut tuning.nights.finale {
        boss.summon.kind = MonsterKind::Grunt;
    }
    Numbers {
        name: "grunts only (no runners, no sappers, grunt minions)".to_string(),
        tuning,
    }
}

/// 원정에서 따로 보는 구간입니다. 밤마다 하나이고, 마지막 밤의 마지막 무리를 처치한 뒤부터 보스를 잡을 때까지가 하나입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Night(u32),
    Boss,
}

/// 지금 단계가 속한 구간입니다. 낮과 새벽은 구간이 아닙니다.
fn part_of(phase: Option<Phase>) -> Option<Part> {
    match phase? {
        Phase::Night {
            step: NightStep::Warning { .. } | NightStep::Boss,
            ..
        } => Some(Part::Boss),
        Phase::Night { day, .. } => Some(Part::Night(day)),
        Phase::Day { .. } | Phase::Dawn { .. } => None,
    }
}

/// 구간 하나의 시작과 끝의 모습입니다.
#[derive(Clone, Debug)]
struct Stretch {
    part: Part,
    towers_at_start: usize,
    stash_at_start: u32,
    /// 원정이 시작된 뒤 마차에 넣은 자원의 합입니다.
    deposited_at_start: u32,
    wagon_at_start: u32,
    wagon_at_end: u32,
    towers_at_end: usize,
    /// 이 구간에서 쓰러진 횟수의 합입니다. 모든 플레이어를 더합니다.
    downs: u32,
    /// 구간을 끝까지 넘겼는지입니다. 실패했거나 제한 시간이 되었으면 거짓입니다.
    cleared: bool,
}

/// 원정 하나가 끝났을 때의 모습입니다.
#[derive(Debug)]
struct Journey {
    party: u32,
    seed: u64,
    /// 제한 시간 안에 끝나지 않았으면 `None`입니다.
    outcome: Option<Outcome>,
    secs: f32,
    stretches: Vec<Stretch>,
}

impl Journey {
    fn won(&self) -> bool {
        self.outcome == Some(Outcome::Victory)
    }
}

/// 한 사람이 다른 사람들과 겹치지 않으려고 알아야 하는, 다른 사람들이 정한 채집 지점과 짓는 자리입니다.
struct Claims {
    nodes: BTreeSet<NodeId>,
    sites: BTreeSet<Tile>,
}

/// 모의 원정의 플레이어 한 명입니다.
struct Hand {
    id: PlayerId,
    /// 보스의 밤에 맡는 일입니다.
    role: Role,
    /// 보스의 밤에 입력을 만듭니다. 다른 단계에서는 부르지 않으므로 예고를 읽는 시간의 기록이 보스가 나온 뒤부터 쌓입니다.
    in_the_boss_fight: Bot,
    /// 지금 채집하러 가는 지점입니다.
    node: Option<NodeId>,
    /// 지금 감시탑을 지으려는 자리입니다.
    site: Option<Tile>,
    /// 짓지 못한다고 거절당한 자리입니다. 다시 고르지 않습니다.
    refused: BTreeSet<Tile>,
    /// 다쳐서 마차 곁에서 회복하는 중인지입니다.
    recovering: bool,
    /// 원정이 시작된 뒤 움직이기 시작하기까지의 Tick입니다. 사람이 같은 순간에 시작하지 않는 것을 흉내 내고, 씨앗마다 입력이 조금씩 달라지게 합니다.
    lag_ticks: u64,
}

impl Hand {
    fn new(id: PlayerId, role: Role, lag_ticks: u64) -> Hand {
        Hand {
            id,
            role,
            lag_ticks,
            in_the_boss_fight: Bot {
                id,
                role,
                seen: VecDeque::new(),
            },
            node: None,
            site: None,
            refused: BTreeSet::new(),
            recovering: false,
        }
    }

    /// 이번 Tick의 입력입니다. `anyone`은 모두가 쓰러진 동료를 부활시키러 갈 수 있는 목록(`reviver_of`)이고, `boss_roster`는 보스의 밤의 일입니다.
    fn input(
        &mut self,
        sim: &Sim,
        anyone: &[(PlayerId, Role)],
        boss_roster: &[(PlayerId, Role)],
        claims: &Claims,
    ) -> PlayerInput {
        let Some(me) = sim.player(self.id).filter(|me| me.is_alive()) else {
            return PlayerInput::default();
        };
        match sim.phase() {
            None => PlayerInput::default(),
            Some(Phase::Night {
                step: NightStep::Boss,
                ..
            }) => self.in_the_boss_fight.input(sim, boss_roster),
            Some(Phase::Night {
                step: NightStep::Warning { .. },
                ..
            }) => mend_or_wait(sim, me),
            Some(Phase::Night { .. }) => guard(sim, me, anyone),
            Some(Phase::Day { ticks_left, .. }) => self.work(
                sim,
                me,
                anyone,
                claims,
                Some(ticks_left as f32 * sim.tuning().dt()),
            ),
            Some(Phase::Dawn { .. }) => self.work(sim, me, anyone, claims, None),
        }
    }

    /// 낮의 입력입니다. `secs_left`는 낮이 끝나기까지 남은 시간이고, 새벽에는 없습니다.
    fn work(
        &mut self,
        sim: &Sim,
        me: &Player,
        anyone: &[(PlayerId, Role)],
        claims: &Claims,
        secs_left: Option<f32>,
    ) -> PlayerInput {
        let t = sim.tuning();
        if let Some(swing) = swing_at_minion_beside(sim, me.pos) {
            return swing;
        }
        if let Some(revive) = revive_input(sim, anyone, me) {
            return revive;
        }
        if me.hp() * 100 < t.player.max_hp * RETREAT_BELOW {
            self.recovering = true;
        } else if me.hp() * 100 >= t.player.max_hp * RETURN_AT {
            self.recovering = false;
        }
        if self.recovering {
            return mend(sim, me, false).unwrap_or_else(|| wait_by_the_wagon(sim, me));
        }
        if let Some(input) = hunt(sim, me) {
            return input;
        }
        if sim.wagon().hp * 100 < t.wagon.max_hp * WAGON_WORN
            && let Some(input) = mend(sim, me, false)
        {
            return input;
        }
        if let Some(input) = self.build(sim, me, claims) {
            return input;
        }
        let going_home = secs_left.is_some_and(|secs| secs < HOME_SECS);
        let full = me.carried() >= t.resources.carry_capacity;
        if me.carried() > 0 && (full || going_home) {
            return bring_to_the_wagon(sim, me);
        }
        if !going_home && let Some(input) = self.gather(sim, me, claims) {
            return input;
        }
        if me.carried() > 0 {
            return bring_to_the_wagon(sim, me);
        }
        mend_or_wait(sim, me)
    }

    /// 보관함에 감시탑 하나의 비용이 있으면 마차에 가까운 빈 자리부터 감시탑을 짓는 입력입니다. 짓고 있는 포탑이 있으면 끝날 때까지 키를 누르고 있습니다.
    fn build(&mut self, sim: &Sim, me: &Player, claims: &Claims) -> Option<PlayerInput> {
        let t = sim.tuning();
        let building = matches!(me.interaction(), Some(Interaction::Building { .. }));
        if !building {
            let cost = t.structures.get(me.build_kind).build_cost;
            if sim.stash() < cost || sim.towers().count() >= t.village.tower_limit as usize {
                self.site = None;
                return None;
            }
            if let Some(refusal) = me.build_refusal() {
                self.refused.insert(refusal.tile);
                self.site = None;
            }
            let tiles = sim.tiles();
            let open = |tile: &Tile| {
                tiles.is_free(*tile) && !self.refused.contains(tile) && !claims.sites.contains(tile)
            };
            if self.site.is_none_or(|site| !open(&site)) {
                self.site = t
                    .spread_tower_sites(t.village.tower_limit)
                    .into_iter()
                    .find(open);
            }
        }
        let site = self.site?;
        let here = sim.tiles().tile_at(me.pos);
        let (dx, dy) = (
            i32::from(site.x) - i32::from(here.x),
            i32::from(site.y) - i32::from(here.y),
        );
        if dx.abs() <= 1 && dy.abs() <= 1 && (dx, dy) != (0, 0) {
            // 서 있는 타일에서 자리로 향하는 방향이 앞 타일을 정하므로, 서 있는 점이 아니라 타일의 중심으로 조준합니다.
            return Some(PlayerInput {
                aim: AimDir::new(dx as f32, dy as f32),
                build: true,
                ..PlayerInput::default()
            });
        }
        let goal = stand_by(sim, site.rect(), me.pos)?;
        let step = route_to(sim, me.pos, goal)?;
        Some(travel(sim, me, step))
    }

    /// 채집 지점에서 자원을 드는 입력입니다. 다른 사람이 고르지 않은 가까운 지점을 고르고, 지점이 비면 다른 지점으로 갑니다. 갈 지점이 없으면 `None`입니다.
    fn gather(&mut self, sim: &Sim, me: &Player, claims: &Claims) -> Option<PlayerInput> {
        let t = sim.tuning();
        let kept = self
            .node
            .and_then(|id| sim.nodes().find(|node| node.id == id))
            .filter(|node| node.amount > 0);
        let chosen = kept.or_else(|| {
            sim.nodes().filter(|node| node.amount > 0).min_by(|a, b| {
                claims
                    .nodes
                    .contains(&a.id)
                    .cmp(&claims.nodes.contains(&b.id))
                    .then(distance_to_node(a, me).total_cmp(&distance_to_node(b, me)))
            })
        });
        self.node = chosen.map(|node| node.id);
        let node = chosen?;
        let rect = node.tile.rect();
        Some(
            match approach(sim, me.pos, rect, t.resources.gather_reach) {
                Some(step) => travel(sim, me, step),
                None => PlayerInput {
                    aim: AimDir::new(rect.center().x - me.pos.x, rect.center().y - me.pos.y),
                    interact: true,
                    ..PlayerInput::default()
                },
            },
        )
    }
}

/// 걸어서 가는 입력입니다. 달리기 게이지가 충분히 남아 있으면 달립니다. 게이지가 비면 달리기 키를 떼야 차므로 조금 남았을 때 뗍니다.
fn travel(sim: &Sim, me: &Player, step: Vec2) -> PlayerInput {
    PlayerInput {
        sprint: me.stamina_percent(sim.tuning()) >= RUN_FROM,
        ..walking(step, step, false)
    }
}

fn distance_to_node(node: &GatherNode, me: &Player) -> f32 {
    node.tile.center().distance(me.pos)
}

/// `HUNT_REACH`타일 안에 몬스터가 있으면 가장 가까운 몬스터에게 길을 따라 다가가는 입력입니다. 곁에 닿으면 `swing_at_minion_beside`가 벱니다.
fn hunt(sim: &Sim, me: &Player) -> Option<PlayerInput> {
    let prey = sim
        .monsters()
        .filter(|monster| is_minion(monster) && monster.pos.distance(me.pos) < HUNT_REACH)
        .min_by(|a, b| a.pos.distance(me.pos).total_cmp(&b.pos.distance(me.pos)))?;
    let goal = sim.tiles().tile_at(prey.pos);
    let step =
        route_to(sim, me.pos, goal).unwrap_or_else(|| (prey.pos - me.pos).normalize_or_zero());
    Some(walking(step, step, false))
}

/// 곁에 몬스터가 몰려 있으면 그 무리의 중심에서 가장 먼 쪽으로 달려서 벗어나는 입력입니다. 갈 수 있는 자리(비어 있는 타일이고 마차에서 `EVADE_RANGE` 안)가 없으면 `None`이고 그 자리에서 싸웁니다.
/// 몬스터는 걷는 속도(2.2)가 플레이어(4.0)보다 느려서 달려서 벗어나면 따라오는 몬스터가 길게 늘어서므로, 포탑이 쏘는 길로 몬스터를 끌고 다니는 셈입니다.
fn evade(sim: &Sim, me: &Player) -> Option<PlayerInput> {
    let t = sim.tuning();
    let crowd: Vec<Vec2> = sim
        .monsters()
        .filter(|monster| is_minion(monster) && monster.pos.distance(me.pos) < CROWD_RADIUS)
        .map(|monster| monster.pos)
        .collect();
    let threshold = if me.hp() * 2 < t.player.max_hp {
        CROWD_SIZE - 1
    } else {
        CROWD_SIZE
    };
    if crowd.len() < threshold {
        return None;
    }
    let center = crowd.iter().fold(Vec2::ZERO, |sum, pos| sum + *pos) * (1.0 / crowd.len() as f32);
    let away = (me.pos - center).normalize_or_zero();
    let tiles = sim.tiles();
    let wagon = sim.wagon().rect;
    let way = (0..16)
        .map(|step| {
            let angle = step as f32 * std::f32::consts::FRAC_PI_8;
            Vec2::new(angle.cos(), angle.sin())
        })
        .filter(|dir| {
            let landing = me.pos + *dir * 4.0;
            let inside = landing.x > 1.0
                && landing.y > 1.0
                && landing.x < t.map.width - 1.0
                && landing.y < t.map.height - 1.0;
            inside
                && tiles.is_free(tiles.tile_at(landing))
                && wagon.distance_to(landing) <= EVADE_RANGE
        })
        .max_by(|a, b| a.dot(away).total_cmp(&b.dot(away)))?;
    Some(PlayerInput {
        sprint: me.stamina_percent(t) >= RUN_FROM,
        ..walking(way, way, false)
    })
}

/// 밤의 입력입니다. 곁에 온 몬스터를 베고, 쓰러진 동료를 부활시키고, 몬스터가 있으면 마차를 지키고, 없으면 수리합니다.
fn guard(sim: &Sim, me: &Player, anyone: &[(PlayerId, Role)]) -> PlayerInput {
    if let Some(evade) = evade(sim, me) {
        return evade;
    }
    if let Some(swing) = swing_at_minion_beside(sim, me.pos) {
        return swing;
    }
    if let Some(revive) = revive_input(sim, anyone, me) {
        return revive;
    }
    if sim.monsters().any(is_minion) {
        return guard_the_wagon(sim, me.pos, false);
    }
    mend_or_wait(sim, me)
}

fn mend_or_wait(sim: &Sim, me: &Player) -> PlayerInput {
    mend(sim, me, true).unwrap_or_else(|| wait_by_the_wagon(sim, me))
}

/// 보관함에 자원이 있고 상한 포탑이나 마차가 있으면 그것을 수리하는 입력입니다. 마차의 HP가 절반보다 적으면 마차가 먼저이고,
/// 아니면 가장 가까운 상한 포탑이 먼저이고, 그 뒤가 마차입니다. `towers`가 거짓이면 마차만 고칩니다. 고칠 것이 없으면 `None`입니다.
/// 포탑을 수리하는 것은 새로 짓는 것보다 시간이 훨씬 걸리므로(20 HP/초와 약 100 HP/초), 낮에는 지을 것도 모을 것도 없을 때와 밤에 몬스터가 없을 때만 합니다.
fn mend(sim: &Sim, me: &Player, towers: bool) -> Option<PlayerInput> {
    if sim.stash() == 0 {
        return None;
    }
    let t = sim.tuning();
    let wagon = sim.wagon();
    let hurt_tower = sim
        .towers()
        .filter(|_| towers)
        .filter(|tower| tower.is_standing() && tower.hp() < t.structures.get(tower.kind).max_hp)
        .min_by(|a, b| {
            a.rect()
                .distance_to(me.pos)
                .total_cmp(&b.rect().distance_to(me.pos))
        });
    let wagon_weak = wagon.hp * 2 < t.wagon.max_hp;
    let wagon_hurt = wagon.hp < t.wagon.max_hp;
    let (rect, reach) = match hurt_tower {
        Some(tower) if !wagon_weak => {
            (tower.rect(), t.structures.get(tower.kind).repair.heal.range)
        }
        _ if wagon_hurt => (wagon.rect, t.wagon.repair.heal.range),
        _ => return None,
    };
    Some(match approach(sim, me.pos, rect, reach) {
        Some(step) => travel(sim, me, step),
        None => PlayerInput {
            aim: AimDir::new(rect.center().x - me.pos.x, rect.center().y - me.pos.y),
            interact: true,
            ..PlayerInput::default()
        },
    })
}

/// 든 자원을 보관함에 넣으려고 마차의 닿는 거리 안으로 걷는 입력입니다. 안에 있으면 서 있습니다. 자원은 닿는 순간 저절로 들어갑니다.
fn bring_to_the_wagon(sim: &Sim, me: &Player) -> PlayerInput {
    let reach = sim.tuning().resources.deposit_reach;
    match approach(sim, me.pos, sim.wagon().rect, reach) {
        Some(step) => travel(sim, me, step),
        None => walking(Vec2::ZERO, Vec2::new(1.0, 0.0), false),
    }
}

/// 마차 곁의 회복이 닿는 자리로 걷고 서 있는 입력입니다.
fn wait_by_the_wagon(sim: &Sim, me: &Player) -> PlayerInput {
    let reach = sim.tuning().wagon.recovery.range * 0.6;
    match approach(sim, me.pos, sim.wagon().rect, reach) {
        Some(step) => travel(sim, me, step),
        None => walking(Vec2::ZERO, Vec2::new(1.0, 0.0), false),
    }
}

/// 플레이어의 가장자리에서 `rect`까지 잰 거리가 `reach` 안이 될 때까지 길을 따라 걷는 방향입니다. 이미 안이면 `None`입니다.
fn approach(sim: &Sim, at: Vec2, rect: Rect, reach: f32) -> Option<Vec2> {
    if rect.distance_to(at) - sim.tuning().player.radius <= reach - SLACK {
        return None;
    }
    let goal = stand_by(sim, rect, at)?;
    Some(route_to(sim, at, goal).unwrap_or_else(|| (rect.center() - at).normalize_or_zero()))
}

/// `rect`에 이웃한 비어 있는 타일 가운데 `from`에서 가장 가까운 타일입니다. 타일 하나의 사각형이면 그 타일에 이웃한 여덟 타일 가운데입니다.
fn stand_by(sim: &Sim, rect: Rect, from: Vec2) -> Option<Tile> {
    sim.tiles()
        .engage_tiles(rect, 1.0)
        .into_iter()
        .min_by(|a, b| {
            a.center()
                .distance(from)
                .total_cmp(&b.center().distance(from))
        })
}

/// 타일 `goal`까지 길을 따라 걷는 방향입니다. 이미 그 타일 안이면 `None`입니다.
fn route_to(sim: &Sim, at: Vec2, goal: Tile) -> Option<Vec2> {
    let tiles = sim.tiles();
    if tiles.tile_at(at) == goal {
        return None;
    }
    let toward = FlowField::toward(tiles, &[goal])
        .next_waypoint(tiles, at)
        .map_or(goal.center() - at, |waypoint| waypoint - at);
    Some(toward.normalize_or_zero())
}

/// 구간 하나가 열릴 때 재 둔 값입니다.
struct Opened {
    part: Part,
    towers: usize,
    stash: u32,
    deposited: u32,
    wagon: u32,
    downs: u32,
}

fn towers_standing(sim: &Sim) -> usize {
    sim.towers().filter(|tower| tower.is_standing()).count()
}

fn deposited(sim: &Sim) -> u32 {
    sim.players().map(|player| player.record.deposited).sum()
}

fn downs(sim: &Sim) -> u32 {
    sim.players().map(|player| player.record.downs).sum()
}

fn open(sim: &Sim, part: Part) -> Opened {
    Opened {
        part,
        towers: towers_standing(sim),
        stash: sim.stash(),
        deposited: deposited(sim),
        wagon: sim.wagon().hp,
        downs: downs(sim),
    }
}

fn close(sim: &Sim, opened: Opened, cleared: bool) -> Stretch {
    Stretch {
        part: opened.part,
        towers_at_start: opened.towers,
        stash_at_start: opened.stash,
        deposited_at_start: opened.deposited,
        wagon_at_start: opened.wagon,
        wagon_at_end: sim.wagon().hp,
        towers_at_end: towers_standing(sim),
        downs: downs(sim) - opened.downs,
        cleared,
    }
}

/// `party`명이 `seed`의 방에서 `numbers`의 수치로 원정을 야영지에서부터 끝까지 합니다. 모두 준비를 누르고, 3초 뒤에 첫날의 낮이 시작됩니다.
fn journey(numbers: &Numbers, party: u32, seed: u64) -> Journey {
    let mut sim = Sim::new(numbers.tuning.clone(), Seed(seed));
    let ids: Vec<PlayerId> = (0..party)
        .map(|_| sim.add_player().expect("the room has space"))
        .collect();
    let mut hands: Vec<Hand> = ids
        .iter()
        .enumerate()
        .map(|(index, &id)| {
            let role = if party >= 2 && index as u32 == party - 1 {
                Role::Guard
            } else {
                Role::Fighter
            };
            Hand::new(id, role, (seed * 7 + index as u64 * 13) % 60)
        })
        .collect();
    let boss_roster: Vec<(PlayerId, Role)> =
        hands.iter().map(|hand| (hand.id, hand.role)).collect();
    let anyone: Vec<(PlayerId, Role)> = ids.iter().map(|&id| (id, Role::Fighter)).collect();

    let limit = sim.tuning().ticks(LIMIT_SECS);
    let mut open_stretch: Option<Opened> = None;
    let mut stretches = Vec::new();
    let mut since_start = 0u64;
    for tick in 0..limit {
        let mut inputs = PlayerInputs::new();
        if sim.phase().is_none() {
            // 야영지입니다. 첫 Tick에 모두 준비 키를 한 번 누릅니다.
            for &id in &ids {
                inputs.insert(
                    id,
                    PlayerInput {
                        toggle_ready: tick == 0,
                        ..PlayerInput::default()
                    },
                );
            }
        } else {
            since_start += 1;
            for index in 0..hands.len() {
                let claims = Claims {
                    nodes: hands
                        .iter()
                        .enumerate()
                        .filter(|&(other, _)| other != index)
                        .filter_map(|(_, hand)| hand.node)
                        .collect(),
                    sites: hands
                        .iter()
                        .enumerate()
                        .filter(|&(other, _)| other != index)
                        .filter_map(|(_, hand)| hand.site)
                        .collect(),
                };
                let id = hands[index].id;
                let input = if since_start <= hands[index].lag_ticks {
                    PlayerInput::default()
                } else {
                    hands[index].input(&sim, &anyone, &boss_roster, &claims)
                };
                inputs.insert(id, input);
            }
        }
        sim.step(&inputs);

        let now = part_of(sim.phase());
        if open_stretch.as_ref().map(|opened| opened.part) != now {
            if let Some(opened) = open_stretch.take() {
                // 실패해서 단계가 끝난 구간은 넘기지 못한 것입니다. 결과 화면이 열려 있지 않거나 승리이면 넘긴 구간입니다.
                let cleared = sim
                    .debrief()
                    .is_none_or(|debrief| debrief.outcome() == Outcome::Victory);
                stretches.push(close(&sim, opened, cleared));
            }
            open_stretch = now.map(|part| open(&sim, part));
        }
        if sim.debrief().is_some() {
            break;
        }
    }
    let outcome = sim.debrief().map(|debrief| debrief.outcome());
    if let Some(opened) = open_stretch.take() {
        stretches.push(close(&sim, opened, outcome == Some(Outcome::Victory)));
    }
    Journey {
        party,
        seed,
        outcome,
        secs: sim.tick() as f32 * sim.tuning().dt(),
        stretches,
    }
}

fn part_name(part: Part) -> String {
    match part {
        Part::Night(day) => format!("night {day}"),
        Part::Boss => "boss".to_string(),
    }
}

fn describe(numbers: &Numbers, journey: &Journey) -> String {
    let stretches: Vec<String> = journey
        .stretches
        .iter()
        .map(|stretch| {
            format!(
                "{}: {} towers, stash {}, gathered {}, wagon {} -> {} towers, wagon {}, {} downs{}",
                part_name(stretch.part),
                stretch.towers_at_start,
                stretch.stash_at_start,
                stretch.deposited_at_start,
                stretch.wagon_at_start,
                stretch.towers_at_end,
                stretch.wagon_at_end,
                stretch.downs,
                if stretch.cleared { "" } else { " (lost here)" }
            )
        })
        .collect();
    format!(
        "{} players, seed {}, {}: {:?} after {:.0}s | {}",
        journey.party,
        journey.seed,
        numbers.name,
        journey.outcome,
        journey.secs,
        stretches.join(" | ")
    )
}

/// 모의 원정을 돌릴 씨앗입니다. 씨앗은 상자의 자리와 낮의 몬스터가 서는 자리를 바꾸고, 플레이어가 움직이기 시작하는 때도 바꿉니다(`Hand::lag_ticks`).
const SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 6];

/// 같은 수치와 인원의 원정 여러 개를 구간마다 한 줄로 모은 것입니다. 구간마다 닿은 원정의 수, 거기서 진 원정의 수, 시작할 때의 감시탑 수의 범위,
/// 끝났을 때 마차의 가장 낮은 HP, 쓰러진 횟수의 합을 적습니다.
fn summarize(journeys: &[Journey]) -> String {
    let parts = [Part::Night(1), Part::Night(2), Part::Night(3), Part::Boss];
    let lines: Vec<String> = parts
        .iter()
        .filter_map(|&part| {
            let reached: Vec<&Stretch> = journeys
                .iter()
                .flat_map(|journey| journey.stretches.iter().filter(|stretch| stretch.part == part))
                .collect();
            let towers = reached.iter().map(|stretch| stretch.towers_at_start);
            let (fewest, most) = (towers.clone().min()?, towers.max()?);
            Some(format!(
                "{}: reached by {}, lost by {}, towers at start {fewest}..{most}, wagon at end at least {}, {} downs",
                part_name(part),
                reached.len(),
                reached.iter().filter(|stretch| !stretch.cleared).count(),
                reached.iter().map(|stretch| stretch.wagon_at_end).min()?,
                reached.iter().map(|stretch| stretch.downs).sum::<u32>()
            ))
        })
        .collect();
    lines.join(" | ")
}

/// `numbers`의 원정을 `party`명이 모든 씨앗으로 돌리고, 원정마다 한 줄과 모은 한 줄(`SUMMARY`)을 출력합니다. 이긴 원정의 수를 돌려줍니다.
fn print_all_seeds(numbers: &Numbers, party: u32) -> usize {
    let journeys: Vec<Journey> = SEEDS
        .iter()
        .map(|&seed| journey(numbers, party, seed))
        .collect();
    for journey in &journeys {
        println!("{}", describe(numbers, journey));
    }
    let won = journeys.iter().filter(|journey| journey.won()).count();
    println!(
        "SUMMARY {party} players, {}: {won} of {} won | {}",
        numbers.name,
        journeys.len(),
        summarize(&journeys)
    );
    won
}

#[test]
#[ignore = "every seed of every party size and set of numbers takes about a minute and a half; run with --ignored"]
fn a_full_mock_expedition_of_one_to_four_players_is_printed_for_each_set_of_numbers() {
    for numbers in [as_shipped(), runners_take_every_tower_hit(), grunts_only()] {
        for party in [1, 2, 3, 4] {
            print_all_seeds(&numbers, party);
        }
    }
}

#[test]
#[ignore = "every seed of every limit of towers takes minutes; run with --ignored"]
fn a_full_mock_expedition_with_at_most_so_many_towers_is_printed_to_find_where_a_small_room_loses()
{
    // 낮에 모은 자원으로 짓는 감시탑의 수가 얼마나 되어야 원정을 끝내는지를 봅니다. 포탑의 상한을 낮추면 그 수를 넘게 짓지 못하므로, 자원이나 시간이 모자란 방을 흉내 냅니다.
    for numbers in [as_shipped(), runners_take_every_tower_hit(), grunts_only()] {
        for party in [1, 2, 3, 4] {
            for limit in [2, 4, 6, 8, 11] {
                print_all_seeds(&numbers.with_tower_limit(limit), party);
            }
        }
    }
}

#[test]
#[ignore = "every seed of every party size at every tower damage takes minutes; run with --ignored"]
fn a_full_mock_expedition_is_printed_for_each_tower_damage_a_sapper_takes() {
    for percent in [25, 30, 35, 40, 45, 50] {
        let numbers = as_shipped().with_sapper_tower_damage_percent(percent);
        for party in [1, 2, 3, 4] {
            print_all_seeds(&numbers, party);
        }
    }
}

#[test]
fn a_lone_player_finishes_the_whole_expedition_at_the_default_numbers_on_the_first_four_seeds() {
    for seed in &SEEDS[..4] {
        let result = journey(&as_shipped(), 1, *seed);
        assert!(result.won(), "{}", describe(&as_shipped(), &result));
    }
}

#[test]
fn a_journey_that_loses_marks_only_the_part_it_lost_in_as_not_cleared() {
    // 포탑을 둘만 세울 수 있는 1명의 방은 셋째 밤에서 집니다. 진 구간만 넘기지 못한 것으로 적혀야 합니다.
    let numbers = as_shipped().with_tower_limit(2);
    let result = journey(&numbers, 1, 2);
    assert_eq!(
        result.outcome,
        Some(Outcome::Defeat),
        "{}",
        describe(&numbers, &result)
    );
    let cleared: Vec<(Part, bool)> = result
        .stretches
        .iter()
        .map(|stretch| (stretch.part, stretch.cleared))
        .collect();
    assert_eq!(
        cleared,
        vec![
            (Part::Night(1), true),
            (Part::Night(2), true),
            (Part::Night(3), false)
        ],
        "{}",
        describe(&numbers, &result)
    );
}
