//! 마차와 시설(포탑)입니다. 둘 다 HP를 가진 단단한 사각형입니다.
//!
//! 시설은 번호를 키로 하는 모음(`World::towers`)에 저장합니다. 자리는 타일 하나(`tile`)이고 사각형은 거기서 얻습니다.
//! 시설은 짓는 중(`Building`)과 서 있는 상태(`Standing`) 둘 중 하나입니다. 짓는 중인 시설도 시설이므로 타일을 차지하고,
//! 길을 막고, 상한(`village.tower_limit`)에 포함되고, 취소하면 지워집니다. 그래서 동시에 진행되는 두 건설이 서로를 「이미 있는 시설」로 봅니다.
//! 파괴된 포탑은 잔해를 남기지 않고 모음에서 지워집니다.

use crate::build::{self, BuildRefusal};
use crate::ids::{MonsterId, PlayerId, Team, TowerId};
use crate::kind::StructureKind;
use crate::math::{Rect, Vec2};
use crate::monster::Monster;
use crate::player::{Interaction, Life};
use crate::tuning::{TowerAction, Tuning};
use crate::world::{Affliction, Ctx, Hit, Source, Target, Tile, World};

/// 마차입니다. HP가 0이 되면 원정이 실패합니다.
/// 같은 Tick에 원정이 끝나므로 「파괴된 상태」는 따로 없습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Wagon {
    pub team: Team,
    pub rect: Rect,
    pub hp: u32,
}

impl Wagon {
    pub(crate) fn new(t: &Tuning) -> Wagon {
        Wagon {
            team: Team::CARAVAN,
            rect: t.map.wagon,
            hp: t.wagon.max_hp,
        }
    }

    /// HP를 `max_hp`까지 회복합니다.
    pub(crate) fn repair(&mut self, amount: u32, max_hp: u32) {
        self.hp = self.hp.saturating_add(amount).min(max_hp);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tower {
    pub id: TowerId,
    pub kind: StructureKind,
    pub team: Team,
    pub tile: Tile,
    pub state: TowerState,
}

/// 포탑의 상태입니다. 짓는 중인 포탑에는 HP, 쿨다운, 사격 대상이 없고, 서 있는 포탑에는 짓는 사람과 진행도가 없습니다.
/// 그래서 「짓는 중인 포탑이 사격한다」와 「서 있는 포탑이 아직 지어진다」는 코드로 표현할 수 없습니다.
///
/// ```text
///   건설 시작 --> Building --(건설 시간이 지남)--> Standing --(HP가 0이 됨)--> 모음에서 지워짐
///                    |
///                    +--(짓는 사람이 키를 놓음, 쓰러짐, 나감, 완료 때 조건이 깨짐)--> 모음에서 지워지고 자원이 돌아감
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum TowerState {
    Building {
        /// 짓고 있는 플레이어입니다. 이 플레이어의 `Interaction::Building { tower }`가 이 포탑을 가리키는 동안만 진행됩니다.
        builder: PlayerId,
        /// 건설을 시작할 때 보관함에서 낸 자원입니다. 취소하면 이만큼 돌려줍니다.
        paid: u32,
        /// 건설 키를 끊지 않고 누른 Tick 수입니다. 시작한 Tick을 포함합니다. 진행도는 이 값을 건설 시간으로 나눈 것입니다.
        elapsed_ticks: u32,
    },
    Standing {
        /// `World::apply_hits`가 포탑을 지우기 전의 한 순간을 빼면 항상 1 이상입니다.
        hp: u32,
        /// 다음 사격까지 남은 Tick입니다.
        cooldown_ticks: u32,
        last_shot: Option<Shot>,
    },
}

/// 포탑의 마지막 사격입니다. 이벤트가 아니라 상태이므로, 모든 클라이언트가 스냅샷만으로 같은 사격 효과를 그릴 수 있습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shot {
    pub target: MonsterId,
    pub tick: u64,
}

impl Tower {
    pub fn rect(&self) -> Rect {
        self.tile.rect()
    }

    /// 최대 HP의 서 있는 상태입니다. 건설을 마친 포탑이 이 값으로 시작합니다.
    pub(crate) fn standing_state(kind: StructureKind, t: &Tuning) -> TowerState {
        TowerState::Standing {
            hp: t.structures.get(kind).max_hp,
            cooldown_ticks: 0,
            last_shot: None,
        }
    }

    /// 짓는 중인 포탑의 HP는 0입니다.
    pub fn hp(&self) -> u32 {
        match self.state {
            TowerState::Standing { hp, .. } => hp,
            TowerState::Building { .. } => 0,
        }
    }

    pub fn is_standing(&self) -> bool {
        matches!(self.state, TowerState::Standing { .. })
    }

    /// 건설의 진행도(0.0 이상 1.0 이하)입니다. 서 있는 포탑에는 진행도가 없습니다.
    pub fn build_progress(&self, t: &Tuning) -> Option<f32> {
        match self.state {
            TowerState::Building { elapsed_ticks, .. } => {
                let required = t.ticks(t.structures.get(self.kind).build_secs).max(1);
                Some((elapsed_ticks as f32 / required as f32).min(1.0))
            }
            TowerState::Standing { .. } => None,
        }
    }

    /// 피해를 받습니다. 짓는 중인 포탑은 피해를 받지 않습니다.
    /// HP가 0이 된 포탑은 `World::apply_hits`가 같은 Tick에 지웁니다.
    pub(crate) fn take_damage(&mut self, amount: u32) {
        if let TowerState::Standing { hp, .. } = &mut self.state {
            *hp = hp.saturating_sub(amount);
        }
    }

    /// HP를 `max_hp`까지 회복합니다. 짓는 중인 포탑은 회복하지 않습니다.
    pub(crate) fn repair(&mut self, amount: u32, max_hp: u32) {
        if let TowerState::Standing { hp, .. } = &mut self.state {
            *hp = hp.saturating_add(amount).min(max_hp);
        }
    }
}

/// 서 있는 포탑 가운데 몬스터에게 작용하는 포탑의 이번 Tick의 동작을 정합니다. 피해는 `hits`에, 피해 없이 거는 효과는 `afflictions`에 모으고,
/// `World::apply_hits`와 `effect::apply_afflictions`가 한꺼번에 적용합니다. 그래서 같은 Tick에 몬스터가 보는 상태는 모두 이전 Tick까지의 상태입니다.
///
/// 포탑은 쿨다운(감속 포탑은 펄스의 간격)을 Tick마다 줄이고, 0이 되면 대상을 찾아서 찾았을 때만 동작하고 쿨다운을 다시 채웁니다.
/// 대상이 없으면 쿨다운은 0에 머물므로 대상이 나타난 Tick에 곧바로 동작합니다. 쿨다운이 남아 있는 동안 나타난 대상은 쿨다운이 끝난 Tick에 고릅니다.
/// - 사격: 범위 안의 가장 가까운 적대적 몬스터 한 마리입니다. 거리가 같으면 번호가 작은 몬스터입니다.
/// - 한 마리에 효과: 같은 순서로, 그 효과가 걸릴(`MonsterEffects::would_take`) 몬스터 가운데 이번 Tick에 다른 포탑이 고르지 않은 몬스터입니다.
///   효과는 Tick의 끝에 걸리므로, 고른 몬스터를 기억하지 않으면 두 포탑이 한 번만 걸릴 스턴을 같은 몬스터에게 두 번 씁니다.
///   고른 몬스터가 같은 Tick에 죽거나 다른 곳에서 같은 효과를 먼저 받으면, 포탑의 효과는 버려지고 쿨다운은 쓰입니다.
/// - 범위에 효과: 범위 안의 모든 적대적 몬스터입니다. 이미 더 센 효과가 걸린 몬스터에게는 `MonsterEffects::apply`가 걸지 않습니다.
///
/// 플레이어에게 작용하는 버프 포탑은 쿨다운이 없고 Tick의 끝에서 `hold_auras`가 동작시키므로, 여기서는 건너뜁니다.
pub(crate) fn towers_act(
    world: &mut World,
    ctx: &Ctx,
    hits: &mut Vec<Hit>,
    afflictions: &mut Vec<Affliction>,
) {
    let t = ctx.t;
    let mut candidates = Vec::new();
    // 이번 Tick에 효과를 걸기로 고른 몬스터입니다.
    let mut taken: Vec<MonsterId> = Vec::new();
    for tower in world.towers.values_mut() {
        let TowerState::Standing {
            cooldown_ticks,
            last_shot,
            ..
        } = &mut tower.state
        else {
            continue;
        };
        let spec = t.structures.get(tower.kind);
        let period_secs = match spec.action {
            TowerAction::Shoot(shoot) => shoot.cooldown_secs,
            TowerAction::Afflict(afflict) => afflict.cooldown_secs,
            TowerAction::Pulse(pulse) => pulse.pulse_secs,
            // 버프 포탑입니다.
            TowerAction::Aura(_) => continue,
        };
        *cooldown_ticks = cooldown_ticks.saturating_sub(1);
        if *cooldown_ticks > 0 {
            continue;
        }

        let center = tower.tile.center();
        ctx.grid.query(center, spec.range, &mut candidates);
        let acted = match spec.action {
            TowerAction::Shoot(shoot) => {
                let target = nearest(&world.monsters, &candidates, tower.team, center, |_| true);
                if let Some(target) = target {
                    hits.push(Hit {
                        target: Target::Monster(target),
                        amount: shoot.damage,
                        by: Source::Tower,
                        debuff: shoot.on_hit,
                    });
                    *last_shot = Some(Shot {
                        target,
                        tick: ctx.tick,
                    });
                }
                target.is_some()
            }
            TowerAction::Afflict(afflict) => {
                let target = nearest(
                    &world.monsters,
                    &candidates,
                    tower.team,
                    center,
                    |monster| {
                        !taken.contains(&monster.id)
                            && monster.effects.would_take(afflict.debuff, t)
                    },
                );
                if let Some(target) = target {
                    taken.push(target);
                    afflictions.push(Affliction {
                        target,
                        debuff: afflict.debuff,
                        by: Source::Tower,
                    });
                    *last_shot = Some(Shot {
                        target,
                        tick: ctx.tick,
                    });
                }
                target.is_some()
            }
            TowerAction::Pulse(pulse) => {
                let before = afflictions.len();
                for &index in &candidates {
                    let monster = &world.monsters[index];
                    if tower.team.is_hostile_to(monster.team) {
                        afflictions.push(Affliction {
                            target: monster.id,
                            debuff: pulse.debuff,
                            by: Source::Tower,
                        });
                    }
                }
                afflictions.len() > before
            }
            // 위에서 건너뛰었으므로 오지 않는 갈래입니다.
            TowerAction::Aura(_) => false,
        };
        if acted {
            *cooldown_ticks = t.ticks(period_secs);
        }
    }
}

/// `candidates`(`World::monsters`의 위치) 가운데 `team`에 적대적이고 `accept`가 참인 몬스터 중 `center`에서 가장 가까운 몬스터의 번호입니다.
/// 거리가 같으면 번호가 작은 몬스터입니다.
fn nearest(
    monsters: &[Monster],
    candidates: &[usize],
    team: Team,
    center: Vec2,
    accept: impl Fn(&Monster) -> bool,
) -> Option<MonsterId> {
    let mut best: Option<(f32, usize)> = None;
    for &index in candidates {
        let monster = &monsters[index];
        if !team.is_hostile_to(monster.team) || !accept(monster) {
            continue;
        }
        let distance = center.distance(monster.pos);
        let closer = match best {
            None => true,
            Some((best_distance, best_index)) => {
                distance < best_distance || (distance == best_distance && index < best_index)
            }
        };
        if closer {
            best = Some((distance, index));
        }
    }
    best.map(|(_, index)| monsters[index].id)
}

/// 서 있는 버프 포탑마다 범위 안에 있는 살아 있는 플레이어에게 버프를 줍니다. 짓는 중인 포탑은 주지 않고, 부서진 포탑은 모음에서 지워지므로 주지 않습니다.
/// Tick마다 주므로 범위 안에 있는 동안 버프가 끊기지 않고, 범위를 나간 플레이어는 마지막으로 받은 버프의 길이만큼 더 가지고 있다가 잃습니다.
/// 같은 버프는 칸이 하나이므로 같은 종류의 포탑 둘의 범위가 겹쳐도 세기는 하나이고, 남은 시간이 더 긴 버프는 줄지 않습니다(`PlayerBuffs::grant`).
///
/// 버프의 시간이 줄어든(`effect::advance_effects`) 뒤에 부르므로, `secs`초짜리 버프는 범위를 나간 뒤 정확히 `secs`초만큼의 Tick 동안 플레이어의 행동을 바꿉니다.
/// 플레이어가 쓰러지는 일(`World::apply_hits`)과 부활하는 일(`player::advance_life_timers`)도 끝난 뒤에 부르므로, Tick이 끝났을 때 범위 안에 살아 있는 플레이어는 언제나 버프를 가집니다.
pub(crate) fn hold_auras(world: &mut World, t: &Tuning) {
    for tower in world.towers.values() {
        let spec = t.structures.get(tower.kind);
        let TowerAction::Aura(aura) = spec.action else {
            continue;
        };
        if !tower.is_standing() {
            continue;
        }
        let center = tower.tile.center();
        let range_squared = spec.range * spec.range;
        for player in world.players.values_mut() {
            if tower.team.is_allied_with(player.team)
                && (player.pos - center).length_squared() <= range_squared
            {
                player.grant(aura.buff, t);
            }
        }
    }
}

/// 짓는 중인 포탑의 한 Tick입니다. 건설의 진행, 완료, 취소를 정하는 유일한 곳입니다.
///
/// 건설이 취소되는 모든 경우(키를 놓음, 쓰러짐, 접속을 끊음, 다른 일을 시작함)를 「짓는 사람의 `Interaction`이 이 포탑을 가리키지 않는다」
/// 하나로 판정합니다. 취소하는 원인마다 따로 자원을 돌려주는 곳이 없으므로, 어떤 원인이든 자원이 한 번만 돌아갑니다.
/// 건설 시간을 채우면 `build::check_site`로 한 번 더 판정하고, 통과하면 서 있는 포탑이 됩니다. 어긋났으면 취소하고 짓는 사람에게 이유를 남깁니다.
///
/// `player::update_interactions`가 이번 Tick의 `Interaction`을 정한 뒤에 부릅니다.
pub(crate) fn advance_construction(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    let sites: Vec<(TowerId, PlayerId, Tile, u32)> = world
        .towers
        .values()
        .filter_map(|tower| match tower.state {
            TowerState::Building {
                builder,
                elapsed_ticks,
                ..
            } => Some((tower.id, builder, tower.tile, elapsed_ticks)),
            TowerState::Standing { .. } => None,
        })
        .collect();

    for (id, builder, tile, elapsed_ticks) in sites {
        let still_building = world.players.get(&builder).is_some_and(|player| {
            matches!(player.interaction(), Some(Interaction::Building { tower }) if *tower == id)
        });
        if !still_building {
            cancel(world, id);
            continue;
        }

        let elapsed_ticks = elapsed_ticks + 1;
        let kind = world.towers[&id].kind;
        if elapsed_ticks < t.ticks(t.structures.get(kind).build_secs) {
            if let Some(TowerState::Building {
                elapsed_ticks: slot,
                ..
            }) = world.towers.get_mut(&id).map(|tower| &mut tower.state)
            {
                *slot = elapsed_ticks;
            }
            continue;
        }

        let verdict = build::check_site(world, t, builder, tile, Some(id));
        if verdict.is_ok() {
            world.finish_tower(id, t);
        } else {
            cancel(world, id);
        }
        if let Some(Life::Alive {
            interaction,
            build_refusal,
            ..
        }) = world
            .players
            .get_mut(&builder)
            .map(|player| &mut player.life)
        {
            *interaction = None;
            *build_refusal = verdict.err().map(|why| BuildRefusal { tile, why });
        }
    }
}

/// 짓던 포탑을 지우고 낸 자원을 보관함에 돌려줍니다.
fn cancel(world: &mut World, id: TowerId) {
    if let Some(Tower {
        state: TowerState::Building { paid, .. },
        ..
    }) = world.remove_tower(id)
    {
        world.stash += paid;
    }
}
