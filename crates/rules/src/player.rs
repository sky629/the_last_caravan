//! 플레이어입니다. 생명 상태(살아 있음, 쓰러짐, 사망), 이동, 근접 공격, 상호작용(부활, 수리, 건설)을 다룹니다.

use std::collections::{BTreeMap, BTreeSet};

use crate::build::{self, BuildRefusal};
use crate::collision::{Body, move_circle};
use crate::ids::{PlayerId, Team, TowerId};
use crate::math::Vec2;
use crate::stats::UnitStats;
use crate::structures::TowerState;
use crate::tuning::Tuning;
use crate::world::{Ctx, Hit, Target, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub team: Team,
    /// 이 플레이어가 쓰는 등장 지점의 순서입니다. 새 원정은 이 순서의 등장 지점으로 돌려놓고, 한 지점에 둘이 몰리면 원정마다 다시 정합니다.
    pub(crate) spawn_slot: usize,
    pub pos: Vec2,
    /// 마지막으로 조준한 방향입니다. 근접 공격의 부채꼴이 이 방향을 향합니다.
    pub facing: Vec2,
    pub life: Life,
    /// 마지막으로 처리한 입력의 순서 번호입니다.
    pub last_input_seq: u32,
    /// 마지막으로 근접 공격을 휘두른 Tick입니다. 이벤트가 아니라 상태이므로, 스냅샷을 한 번 놓친 클라이언트도 공격 효과를 그릴 수 있습니다.
    pub last_attack_tick: Option<u64>,
}

/// 플레이어의 생명 상태입니다. 상태마다 그 상태에서 의미가 있는 값만 가집니다.
/// 그래서 다음 상태는 코드로 표현할 수 없습니다.
/// - 쓰러졌거나 사망한 플레이어가 HP, 쿨다운, 상호작용을 가지는 상태 (공격, 부활, 수리를 할 수 없습니다).
/// - 사망한 플레이어가 출혈 시간을 가지는 상태.
///
/// ```text
///   Alive --(HP가 0이 됨)--> Downed --(출혈 시간이 0이 됨)--> Dead
///     ^                        |
///     +----(부활이 끝남)-------+
///
///   새 원정이 시작되면 모든 플레이어가 최대 HP의 Alive가 됩니다.
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum Life {
    Alive {
        /// 항상 1 이상입니다. HP가 0이 되면 `Alive`를 `Downed`로 바꿉니다.
        hp: u32,
        /// 다음 근접 공격까지 남은 Tick입니다.
        cooldown_ticks: u32,
        /// 상호작용 키(F)나 건설 키(B)가 지금 하고 있는 일입니다. 값이 있으면 이동과 공격을 할 수 없습니다.
        interaction: Option<Interaction>,
        /// 건설 키를 누르고 있지만 세울 수 없을 때의 이유입니다. 이벤트가 아니라 상태이고, 키를 누르고 있는 동안 Tick마다 다시 정해집니다.
        build_refusal: Option<BuildRefusal>,
    },
    Downed {
        /// 남은 출혈 시간입니다. 아무도 부활시키지 않는 동안에만 줄어듭니다.
        bleed_left_ticks: u32,
    },
    /// 이 원정이 끝날 때까지 부활할 수 없습니다.
    Dead,
}

/// 상호작용 키(F)나 건설 키(B)를 누르고 있는 동안의 행동입니다. 그 행동을 하는 플레이어가 가지고 있습니다.
///
/// 부활의 진행도는 부활시키는 플레이어의 `Interaction`에만 있고, 건설의 진행도는 짓는 중인 감시탑(`TowerState::Building`)에만 있습니다.
/// 「플레이어 X가 부활되고 있는가」는 `Reviving { target: X }`를 가진 플레이어를 찾아서 알아냅니다.
/// 쓰러진 플레이어에게는 아무것도 저장하지 않으므로, 두 곳의 값이 서로 어긋날 수 없습니다.
#[derive(Clone, Debug, PartialEq)]
pub enum Interaction {
    Reviving {
        target: PlayerId,
        /// 키를 끊지 않고 누른 Tick 수입니다. 누르기 시작한 Tick을 포함합니다.
        /// 취소하면 이 값을 통째로 버리므로 진행도가 처음으로 돌아갑니다.
        elapsed_ticks: u32,
    },
    Repairing {
        tower: TowerId,
        /// 마지막 수리 이후의 Tick 수입니다. 수리를 멈춰도 이미 회복한 HP는 유지됩니다.
        since_pulse_ticks: u32,
    },
    /// `tower`는 이 플레이어가 짓고 있는 감시탑입니다.
    Building { tower: TowerId },
}

impl Player {
    /// 최대 HP의 살아 있는 플레이어를 `pos`에 만듭니다.
    pub(crate) fn new(
        id: PlayerId,
        team: Team,
        spawn_slot: usize,
        pos: Vec2,
        t: &Tuning,
    ) -> Player {
        Player {
            id,
            team,
            spawn_slot,
            pos,
            facing: Vec2::new(1.0, 0.0),
            life: Life::Alive {
                hp: t.player.max_hp,
                cooldown_ticks: 0,
                interaction: None,
                build_refusal: None,
            },
            last_input_seq: 0,
            last_attack_tick: None,
        }
    }

    /// 새 원정이 시작될 때 최대 HP의 살아 있는 상태로 등장 순서가 `spawn_slot`인 `pos`에 돌려놓습니다.
    /// 입력의 순서 번호는 원정이 바뀌어도 이어집니다.
    pub(crate) fn reset(&mut self, spawn_slot: usize, pos: Vec2, t: &Tuning) {
        let seq = self.last_input_seq;
        *self = Player::new(self.id, self.team, spawn_slot, pos, t);
        self.last_input_seq = seq;
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.life, Life::Alive { .. })
    }

    pub fn is_downed(&self) -> bool {
        matches!(self.life, Life::Downed { .. })
    }

    /// 쓰러졌거나 사망한 플레이어의 HP는 0입니다.
    pub fn hp(&self) -> u32 {
        match self.life {
            Life::Alive { hp, .. } => hp,
            Life::Downed { .. } | Life::Dead => 0,
        }
    }

    pub fn interaction(&self) -> Option<&Interaction> {
        match &self.life {
            Life::Alive { interaction, .. } => interaction.as_ref(),
            Life::Downed { .. } | Life::Dead => None,
        }
    }

    /// 건설 키를 누르고 있지만 세울 수 없을 때의 이유입니다.
    pub fn build_refusal(&self) -> Option<&BuildRefusal> {
        match &self.life {
            Life::Alive { build_refusal, .. } => build_refusal.as_ref(),
            Life::Downed { .. } | Life::Dead => None,
        }
    }

    /// 이동 속도, 공격 피해량, 쿨다운을 읽는 유일한 곳입니다. 상태 효과를 추가하면 여기에서 배율을 곱합니다.
    pub(crate) fn stats(&self, t: &Tuning) -> UnitStats {
        let move_speed = match &self.life {
            Life::Alive {
                interaction: None, ..
            } => t.player.speed,
            Life::Downed { .. } => t.player.crawl_speed,
            // 부활이나 수리를 하는 동안과 사망한 뒤에는 움직이지 못합니다.
            Life::Alive {
                interaction: Some(_),
                ..
            }
            | Life::Dead => 0.0,
        };
        UnitStats {
            move_speed,
            attack_damage: t.player.attack_damage,
            attack_cooldown_ticks: t.ticks(t.player.attack_cooldown_secs),
        }
    }

    /// 피해를 받습니다. 살아 있는 플레이어의 HP가 0이 되면 쓰러지고, 이때 하고 있던 상호작용이 함께 사라집니다.
    /// 쓰러졌거나 사망한 플레이어는 피해를 받지 않습니다.
    pub(crate) fn take_damage(&mut self, amount: u32, t: &Tuning) {
        if let Life::Alive { hp, .. } = &mut self.life {
            *hp = hp.saturating_sub(amount);
            if *hp == 0 {
                self.life = Life::Downed {
                    bleed_left_ticks: t.ticks(t.revive.bleed_secs),
                };
            }
        }
    }
}

/// 상호작용 키(F)와 건설 키(B)를 처리합니다. 키를 누르고 있는 살아 있는 플레이어마다 부활, 수리, 건설을 계속하거나, 새로 시작하거나, 취소합니다.
///
/// 진행 중인 부활은 키를 누르고 있고, 대상이 여전히 쓰러진 동료이고, 거리가 범위 안인 동안만 이어집니다.
/// 공격받는 것은 조건이 아닙니다. 조건이 하나라도 깨지면 `Interaction` 값을 버리므로 진행도가 처음으로 돌아갑니다.
/// 쓰러진 플레이어 한 명은 한 명만 부활시킬 수 있습니다. 이미 진행 중인 부활이 먼저 대상을 차지하고,
/// 새로 시작하려는 플레이어는 번호 순서로 남은 대상을 차지합니다.
///
/// 수리는 중단해도 잃는 것이 없으므로(이미 회복한 HP는 그대로입니다) Tick마다 대상을 다시 고릅니다.
/// 그래서 수리하던 플레이어 곁의 동료가 쓰러지면 곧바로 부활로 바뀝니다.
/// 수리는 정해진 간격마다 감시탑의 HP를 회복시키고, 여러 명이 수리하면 회복량이 더해집니다.
///
/// F로 할 일이 있으면 B보다 먼저입니다. 이미 짓고 있는 건설은 B를 누르고 있는 한 이어지고, F를 눌러도 바뀌지 않습니다.
/// F로 할 일이 없고 B를 누르고 있으면 건설 요청으로 모아서 `build::start_builds`가 번호 순서로 하나씩 판정합니다.
/// 건설의 진행과 완료와 취소는 `structures::advance_construction`이 정합니다.
pub(crate) fn update_interactions(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    let holding = |id: PlayerId| ctx.inputs.get(&id).is_some_and(|input| input.interact);
    let holding_build = |id: PlayerId| ctx.inputs.get(&id).is_some_and(|input| input.build);

    // 읽기: 이번 Tick에 플레이어마다 가지게 될 상호작용을 정합니다.
    let mut next: BTreeMap<PlayerId, Option<Interaction>> = BTreeMap::new();
    let mut claimed: BTreeSet<PlayerId> = BTreeSet::new();
    let mut repairs: Vec<(TowerId, u32)> = Vec::new();
    let mut build_requests: Vec<PlayerId> = Vec::new();
    for player in world.players.values() {
        if let Some(Interaction::Reviving {
            target,
            elapsed_ticks,
        }) = player.interaction()
            && holding(player.id)
            && revive_still_valid(world, player, *target, t)
        {
            next.insert(
                player.id,
                Some(Interaction::Reviving {
                    target: *target,
                    elapsed_ticks: elapsed_ticks + 1,
                }),
            );
            claimed.insert(*target);
        }
    }
    for player in world.players.values() {
        if !player.is_alive() || next.contains_key(&player.id) {
            continue;
        }
        if let Some(Interaction::Building { tower }) = player.interaction()
            && holding_build(player.id)
            && build::is_building(world, *tower, player.id)
        {
            next.insert(player.id, Some(Interaction::Building { tower: *tower }));
            continue;
        }
        let mut chosen = if holding(player.id) {
            choose_interaction(world, player, &claimed, t)
        } else {
            None
        };
        if let Some(Interaction::Reviving { target, .. }) = &chosen {
            claimed.insert(*target);
        }
        if let Some(Interaction::Repairing { tower, .. }) = chosen {
            let (interaction, repaired) = count_repair_tick(world, player.interaction(), tower, t);
            if let Some(amount) = repaired {
                repairs.push((tower, amount));
            }
            chosen = Some(interaction);
        }
        if chosen.is_none() && holding_build(player.id) {
            build_requests.push(player.id);
        }
        next.insert(player.id, chosen);
    }

    // 적용
    for (id, interaction) in next {
        if let Some(Player {
            life:
                Life::Alive {
                    interaction: slot,
                    build_refusal,
                    ..
                },
            ..
        }) = world.players.get_mut(&id)
        {
            *slot = interaction;
            *build_refusal = None;
        }
    }
    for (id, amount) in repairs {
        if let Some(tower) = world.towers.get_mut(&id) {
            let max_hp = t.structures.get(tower.kind).max_hp;
            tower.repair(amount, max_hp);
        }
    }
    build::start_builds(world, ctx, &build_requests);
}

/// 새로 시작할 상호작용을 고릅니다. 범위 안의 쓰러진 동료가 있으면 가장 가까운 동료를 부활시키고,
/// 없으면 범위 안에서 손상된 가장 가까운 감시탑을 수리합니다.
/// 이미 다른 플레이어가 부활시키는 동료(`claimed`)와, 파괴된 감시탑과, HP가 가득 찬 감시탑은 고르지 않습니다.
fn choose_interaction(
    world: &World,
    me: &Player,
    claimed: &BTreeSet<PlayerId>,
    t: &Tuning,
) -> Option<Interaction> {
    let mut nearest: Option<(f32, PlayerId)> = None;
    for other in world.players.values() {
        if other.id == me.id || !other.is_downed() || claimed.contains(&other.id) {
            continue;
        }
        if !me.team.is_allied_with(other.team) {
            continue;
        }
        let distance = me.pos.distance(other.pos);
        if distance <= t.revive.range && nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, other.id));
        }
    }
    if let Some((_, target)) = nearest {
        return Some(Interaction::Reviving {
            target,
            elapsed_ticks: 1,
        });
    }

    let mut nearest_tower: Option<(f32, TowerId)> = None;
    for tower in world.towers.values() {
        let TowerState::Standing { hp, .. } = tower.state else {
            continue;
        };
        let spec = t.structures.get(tower.kind);
        if hp >= spec.max_hp || !me.team.is_allied_with(tower.team) {
            continue;
        }
        let gap = Body::Box(tower.rect()).gap_from(me.pos, t.player.radius);
        if gap <= spec.repair_range && nearest_tower.is_none_or(|(best, _)| gap < best) {
            nearest_tower = Some((gap, tower.id));
        }
    }
    nearest_tower.map(|(_, tower)| Interaction::Repairing {
        tower,
        since_pulse_ticks: 0,
    })
}

/// 수리하는 한 Tick을 셉니다. 같은 감시탑을 계속 수리하면 지난 Tick에서 이어서 세고, 새로 시작하면 1부터 셉니다.
/// 정해진 간격이 차면 카운터를 0으로 돌리고 이번에 회복시킬 HP를 함께 돌려줍니다.
fn count_repair_tick(
    world: &World,
    previous: Option<&Interaction>,
    tower: TowerId,
    t: &Tuning,
) -> (Interaction, Option<u32>) {
    let counted = match previous {
        Some(Interaction::Repairing {
            tower: before,
            since_pulse_ticks,
        }) if *before == tower => since_pulse_ticks + 1,
        _ => 1,
    };
    let spec = world
        .towers
        .get(&tower)
        .map(|found| t.structures.get(found.kind));
    match spec {
        Some(spec) if counted >= t.ticks(spec.repair_pulse_secs) => (
            Interaction::Repairing {
                tower,
                since_pulse_ticks: 0,
            },
            Some(spec.repair_hp_per_pulse),
        ),
        _ => (
            Interaction::Repairing {
                tower,
                since_pulse_ticks: counted,
            },
            None,
        ),
    }
}

fn revive_still_valid(world: &World, reviver: &Player, target: PlayerId, t: &Tuning) -> bool {
    world.players.get(&target).is_some_and(|other| {
        other.is_downed()
            && reviver.team.is_allied_with(other.team)
            && reviver.pos.distance(other.pos) <= t.revive.range
    })
}

/// 부활을 마무리하고 출혈 시간을 줄입니다.
/// 부활시키는 플레이어가 이번 Tick에 쓰러졌다면 이미 `Alive`가 아니므로 부활이 끝나지 않습니다.
pub(crate) fn advance_life_timers(world: &mut World, t: &Tuning) {
    let required = t.ticks(t.revive.revive_secs);
    let mut finished: Vec<(PlayerId, PlayerId)> = Vec::new();
    let mut being_revived: BTreeSet<PlayerId> = BTreeSet::new();
    for player in world.players.values() {
        if let Some(Interaction::Reviving {
            target,
            elapsed_ticks,
        }) = player.interaction()
        {
            being_revived.insert(*target);
            if *elapsed_ticks >= required {
                finished.push((player.id, *target));
            }
        }
    }

    for (reviver, target) in finished {
        if let Some(Player {
            life: Life::Alive { interaction, .. },
            ..
        }) = world.players.get_mut(&reviver)
        {
            *interaction = None;
        }
        if let Some(revived) = world.players.get_mut(&target) {
            let hp = (t.player.max_hp * t.revive.revived_hp_percent / 100).max(1);
            revived.life = Life::Alive {
                hp,
                cooldown_ticks: 0,
                interaction: None,
                build_refusal: None,
            };
        }
    }

    // 부활이 진행되는 동안에는 출혈 시간이 멈춥니다.
    for player in world.players.values_mut() {
        if let Life::Downed { bleed_left_ticks } = &mut player.life
            && !being_revived.contains(&player.id)
        {
            *bleed_left_ticks = bleed_left_ticks.saturating_sub(1);
            if *bleed_left_ticks == 0 {
                player.life = Life::Dead;
            }
        }
    }
}

/// 입력대로 플레이어를 움직입니다. 살아 있는 플레이어는 걷고, 쓰러진 플레이어는 기어갑니다.
pub(crate) fn move_players(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    let mut scratch = Vec::new();
    for player in world.players.values_mut() {
        let Some(input) = ctx.inputs.get(&player.id) else {
            continue;
        };
        player.last_input_seq = input.seq;
        if let Some(aim) = input.aim.get() {
            player.facing = aim;
        }
        let speed = player.stats(t).move_speed;
        if speed > 0.0 {
            let delta = input.move_dir.get() * (speed * t.dt());
            player.pos = move_circle(
                player.pos,
                t.player.radius,
                delta,
                &world.tiles,
                &mut scratch,
            );
        }
    }
}

/// 근접 공격입니다. 상호작용 중이 아닌 살아 있는 플레이어가 공격 버튼을 누르고 쿨다운이 끝났으면 휘두르고,
/// 부채꼴 안에 있고 공격 거리 안에 있는 적대적인 몬스터를 모두 맞힙니다.
/// 몬스터를 맞히지 못해도 휘두르면 쿨다운이 시작됩니다.
pub(crate) fn player_attacks(world: &mut World, ctx: &Ctx, hits: &mut Vec<Hit>) {
    let t = ctx.t;
    let arc_cos = t.player.attack_arc_cos();
    let search_radius = t.player.radius + t.player.attack_reach + t.monsters.largest_radius();
    let mut candidates = Vec::new();

    for player in world.players.values_mut() {
        let stats = player.stats(t);
        let Life::Alive {
            cooldown_ticks,
            interaction,
            ..
        } = &mut player.life
        else {
            continue;
        };
        *cooldown_ticks = cooldown_ticks.saturating_sub(1);
        let pressed = ctx.inputs.get(&player.id).is_some_and(|input| input.attack);
        if !pressed || *cooldown_ticks > 0 || interaction.is_some() {
            continue;
        }

        *cooldown_ticks = stats.attack_cooldown_ticks;
        player.last_attack_tick = Some(ctx.tick);
        ctx.grid.query(player.pos, search_radius, &mut candidates);
        for &index in &candidates {
            let monster = &world.monsters[index];
            if !player.team.is_hostile_to(monster.team) {
                continue;
            }
            let body = Body::Circle {
                center: monster.pos,
                radius: t.monsters.get(monster.kind).radius,
            };
            let in_reach = body.gap_from(player.pos, t.player.radius) <= t.player.attack_reach;
            if in_reach && in_arc(player.pos, player.facing, monster.pos, arc_cos) {
                hits.push(Hit {
                    target: Target::Monster(monster.id),
                    amount: stats.attack_damage,
                });
            }
        }
    }
}

/// `target`이 `origin`에서 `facing` 방향의 부채꼴 안에 있는지 판정합니다.
fn in_arc(origin: Vec2, facing: Vec2, target: Vec2, arc_cos: f32) -> bool {
    let offset = target - origin;
    let distance = offset.length();
    // 중심이 거의 겹쳤으면 방향을 알 수 없으므로 부채꼴 안으로 봅니다.
    distance < 1e-4 || facing.dot(offset * (1.0 / distance)) >= arc_cos
}
