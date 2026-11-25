//! 플레이어입니다. 생명 상태(살아 있음, 쓰러짐, 사망), 이동, 상호작용(부활, 수리, 채집, 건설)을 다룹니다.
//! 공격은 `weapon`이 합니다. 가진 무기는 `loadout`이 정합니다.

use std::collections::{BTreeMap, BTreeSet};

use crate::build::{self, BuildRefusal};
use crate::collision::{Body, move_circle};
use crate::ids::{ChestId, GroundWeaponId, NodeId, PlayerId, Team, TowerId};
use crate::kind::WeaponKind;
use crate::loadout::Loadout;
use crate::loot::{self, Prompt};
use crate::math::Vec2;
use crate::resources;
use crate::stamina::Stamina;
use crate::stats::UnitStats;
use crate::structures::TowerState;
use crate::tuning::{RepairTuning, Tuning};
use crate::world::{Ctx, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub team: Team,
    /// 이 플레이어가 쓰는 등장 지점의 순서입니다. 새 원정은 이 순서의 등장 지점으로 돌려놓고, 한 지점에 둘이 몰리면 원정마다 다시 정합니다.
    pub(crate) spawn_slot: usize,
    pub pos: Vec2,
    /// 마지막으로 조준한 방향입니다. 공격이 이 방향을 향합니다.
    pub facing: Vec2,
    pub life: Life,
    /// 가진 무기입니다. 생명 상태가 아니라 플레이어에게 있으므로, 쓰러졌다가 부활하거나 낮이 시작되어도 그대로입니다.
    /// 새 원정이 시작되면 `PlayerTuning::start_loadout`으로 돌아갑니다.
    pub loadout: Loadout,
    /// 마지막으로 처리한 입력의 순서 번호입니다.
    pub last_input_seq: u32,
    /// 마지막으로 한 공격입니다. 이벤트가 아니라 상태이므로, 스냅샷을 한 번 놓친 클라이언트도 공격 효과를 그릴 수 있습니다.
    pub last_attack: Option<LastAttack>,
}

/// 플레이어가 마지막으로 한 공격입니다. 공격한 무기를 함께 기억하므로, 공격한 직후에 무기를 바꿔도 화면은 공격한 무기의 효과를 그립니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastAttack {
    pub tick: u64,
    pub weapon: WeaponKind,
}

/// 플레이어의 생명 상태입니다. 상태마다 그 상태에서 의미가 있는 값만 가집니다.
/// 그래서 다음 상태는 코드로 표현할 수 없습니다.
/// - 쓰러졌거나 사망한 플레이어가 HP, 쿨다운, 상호작용, 든 자원, 달리기 게이지를 가지는 상태 (공격, 부활, 수리, 채집, 달리기를 할 수 없고 자원을 들 수 없습니다).
/// - 사망한 플레이어가 출혈 시간을 가지는 상태.
///
/// ```text
///   Alive --(HP가 0이 됨)--> Downed --(출혈 시간이 0이 됨)--> Dead
///     ^                        |
///     +----(부활이 끝남)-------+
///
///   새 원정이 시작되거나 낮이 시작되면 모든 플레이어가 최대 HP와 가득 찬 달리기 게이지의 Alive가 됩니다.
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum Life {
    Alive {
        /// 항상 1 이상입니다. HP가 0이 되면 `Alive`를 `Downed`로 바꿉니다.
        hp: u32,
        /// 다음 공격까지 남은 Tick입니다.
        cooldown_ticks: u32,
        /// 상호작용 키(F)나 건설 키(B)가 지금 하고 있는 일입니다. 값이 있으면 이동과 공격을 할 수 없습니다.
        interaction: Option<Interaction>,
        /// 건설 키를 누르고 있지만 세울 수 없을 때의 이유입니다. 이벤트가 아니라 상태이고, 키를 누르고 있는 동안 Tick마다 다시 정해집니다.
        build_refusal: Option<BuildRefusal>,
        /// 들고 있는 자원입니다. 상한은 `ResourceTuning::carry_capacity`입니다. 마차에 가까이 가면 보관함으로 옮겨지고, 쓰러지면 그 자리에 떨어집니다.
        carried: u32,
        /// 달리기 게이지입니다. 쓰러지면 사라지고, 부활하면 가득 찬 상태로 다시 생깁니다.
        stamina: Stamina,
        /// 이번 Tick에 달리고 있는지입니다. `update_sprint`가 Tick마다 정하고, 이동 속도를 읽는 `Player::stats`가 읽습니다.
        sprinting: bool,
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
/// 채집 지점의 남은 양은 지점(`GatherNode`)에만 있습니다.
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
        target: RepairTarget,
        /// 마지막 수리 이후의 Tick 수입니다. 수리를 멈춰도 이미 회복한 HP는 유지됩니다.
        since_pulse_ticks: u32,
    },
    Gathering {
        node: NodeId,
        /// 마지막으로 자원을 옮긴 뒤의 Tick 수입니다. 채집을 멈춰도 이미 든 자원은 그대로입니다.
        since_pulse_ticks: u32,
    },
    /// `tower`는 이 플레이어가 짓고 있는 감시탑입니다.
    Building { tower: TowerId },
    /// 무기 상자를 엽니다. 진행도는 이 값에만 있고, 상자는 열릴 때까지 아무것도 기억하지 않습니다.
    /// F를 놓거나 상자가 사라지거나 닿는 거리 밖으로 벗어나거나 피해를 받으면 값을 통째로 버리므로 진행도가 처음으로 돌아갑니다.
    /// 닿는 거리 안에서 동료가 쓰러져도 이어지고, 다 연 뒤에 F를 누르고 있으면 그때 부활이 시작됩니다.
    Opening {
        chest: ChestId,
        /// 키를 끊지 않고 누른 Tick 수입니다. 누르기 시작한 Tick을 포함합니다.
        elapsed_ticks: u32,
    },
}

/// 수리하는 대상입니다. 감시탑은 번호로 가리키고, 마차는 방마다 하나뿐이므로 번호가 없습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairTarget {
    Tower(TowerId),
    Wagon,
}

impl Life {
    /// 하고 있는 일이 없고, 쿨다운이 없고, 든 자원이 없고, 달리기 게이지가 가득 찬 `Alive`입니다.
    pub(crate) fn alive(hp: u32, t: &Tuning) -> Life {
        Life::Alive {
            hp,
            cooldown_ticks: 0,
            interaction: None,
            build_refusal: None,
            carried: 0,
            stamina: Stamina::full(t),
            sprinting: false,
        }
    }
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
            life: Life::alive(t.player.max_hp, t),
            loadout: t.player.start_loadout,
            last_input_seq: 0,
            last_attack: None,
        }
    }

    /// 새 원정이 시작될 때 최대 HP의 살아 있는 상태로 등장 순서가 `spawn_slot`인 `pos`에 돌려놓습니다.
    /// 입력의 순서 번호는 원정이 바뀌어도 이어집니다.
    pub(crate) fn reset(&mut self, spawn_slot: usize, pos: Vec2, t: &Tuning) {
        let seq = self.last_input_seq;
        *self = Player::new(self.id, self.team, spawn_slot, pos, t);
        self.last_input_seq = seq;
    }

    /// 낮이 시작될 때 최대 HP와 가득 찬 달리기 게이지의 살아 있는 상태로 되돌립니다. 위치, 하고 있던 일, 든 자원은 그대로이고,
    /// 쓰러졌거나 사망한 플레이어도 그 자리에서 되살아납니다(쓰러질 때 든 자원은 이미 떨어졌습니다).
    pub(crate) fn restore_for_new_day(&mut self, t: &Tuning) {
        if let Life::Alive { hp, stamina, .. } = &mut self.life {
            *hp = t.player.max_hp;
            *stamina = Stamina::full(t);
        } else {
            self.life = Life::alive(t.player.max_hp, t);
        }
    }

    /// 지금 드는 무기입니다.
    pub fn weapon(&self) -> WeaponKind {
        self.loadout.held()
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

    /// 들고 있는 자원입니다. 쓰러졌거나 사망한 플레이어는 자원을 들 수 없으므로 0입니다.
    pub fn carried(&self) -> u32 {
        match self.life {
            Life::Alive { carried, .. } => carried,
            Life::Downed { .. } | Life::Dead => 0,
        }
    }

    /// 달리기 게이지가 가득 찬 값에 대해 몇 퍼센트인지입니다(0에서 100). 쓰러졌거나 사망한 플레이어는 게이지가 없으므로 0입니다.
    pub fn stamina_percent(&self, t: &Tuning) -> u8 {
        match &self.life {
            Life::Alive { stamina, .. } => stamina.percent(t),
            Life::Downed { .. } | Life::Dead => 0,
        }
    }

    /// 이동 속도, 공격 피해량, 쿨다운을 읽는 유일한 곳입니다. 상태 효과를 추가하면 여기에서 배율을 곱합니다.
    /// 달리는 중(`sprinting`)이면 걷는 속도에 달리기 배율을 곱합니다. 공격 피해량과 쿨다운은 들고 있는 무기의 행에서 읽습니다.
    pub(crate) fn stats(&self, t: &Tuning) -> UnitStats {
        let move_speed = match &self.life {
            Life::Alive {
                interaction: None,
                sprinting: true,
                ..
            } => t.player.speed * t.sprint.speed_multiplier,
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
        let weapon = t.weapons.get(self.weapon());
        UnitStats {
            move_speed,
            attack_damage: weapon.damage,
            attack_cooldown_ticks: t.ticks(weapon.cooldown_secs),
        }
    }

    /// 공격할 수 있는 상태입니다. 살아 있고, 부활, 수리, 채집, 건설, 상자 열기를 하고 있지 않습니다.
    pub(crate) fn is_free_to_act(&self) -> bool {
        matches!(
            self.life,
            Life::Alive {
                interaction: None,
                ..
            }
        )
    }

    /// 피해를 받습니다. 살아 있는 플레이어의 HP가 0이 되면 쓰러지고, 이때 하고 있던 상호작용이 함께 사라집니다.
    /// 상자를 열고 있었다면 HP가 남아도 열기가 취소됩니다. 부활, 수리, 채집, 건설은 HP가 남으면 이어집니다.
    /// 쓰러졌거나 사망한 플레이어는 피해를 받지 않습니다.
    ///
    /// 쓰러지면서 잃은 든 자원의 양을 돌려주므로, 부르는 쪽이 그 자리에 떨어뜨립니다. 쓰러지지 않았으면 0입니다.
    pub(crate) fn take_damage(&mut self, amount: u32, t: &Tuning) -> u32 {
        let Life::Alive {
            hp,
            carried,
            interaction,
            ..
        } = &mut self.life
        else {
            return 0;
        };
        if matches!(interaction, Some(Interaction::Opening { .. })) {
            *interaction = None;
        }
        *hp = hp.saturating_sub(amount);
        if *hp > 0 {
            return 0;
        }
        let dropped = *carried;
        self.life = Life::Downed {
            bleed_left_ticks: t.ticks(t.revive.bleed_secs),
        };
        dropped
    }
}

/// 상호작용 키(F)와 건설 키(B)를 처리합니다. 키를 누르고 있는 살아 있는 플레이어마다 부활, 수리, 채집, 건설을 계속하거나, 새로 시작하거나, 취소합니다.
///
/// 진행 중인 부활은 키를 누르고 있고, 대상이 여전히 쓰러진 동료이고, 거리가 범위 안인 동안만 이어집니다.
/// 공격받는 것은 조건이 아닙니다. 조건이 하나라도 깨지면 `Interaction` 값을 버리므로 진행도가 처음으로 돌아갑니다.
/// 쓰러진 플레이어 한 명은 한 명만 부활시킬 수 있습니다. 이미 진행 중인 부활이 먼저 대상을 차지하고,
/// 새로 시작하려는 플레이어는 번호 순서로 남은 대상을 차지합니다.
///
/// 수리와 채집은 중단해도 잃는 것이 없으므로(이미 회복한 HP와 이미 든 자원은 그대로입니다) Tick마다 대상을 다시 고릅니다.
/// 그래서 수리하던 플레이어 곁의 동료가 쓰러지면 곧바로 부활로 바뀝니다.
/// 수리와 채집은 정해진 간격마다 한 번씩 HP를 회복시키거나 자원을 옮기고, 여러 명이 같은 대상에 하면 양이 더해집니다.
/// 대상은 보관함이 수리의 비용을 낼 수 있는지와 관계없이 고릅니다. 낼 수 없는 동안에도 플레이어는 그 수리를 계속하고 채집으로 바뀌지 않습니다.
///
/// F로 할 일이 있으면 B보다 먼저입니다. 이미 짓고 있는 건설은 B를 누르고 있는 한 이어지고, F를 눌러도 바뀌지 않습니다.
/// F로 할 일이 없고 B를 누르고 있으면 건설 요청으로 모아서 `build::start_builds`가 번호 순서로 하나씩 판정합니다.
/// 건설의 진행과 완료와 취소는 `structures::advance_construction`이 정합니다.
pub(crate) fn update_interactions(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    let holding = |id: PlayerId| ctx.inputs.get(&id).is_some_and(|input| input.interact);
    let pressed = |id: PlayerId| {
        ctx.inputs
            .get(&id)
            .is_some_and(|input| input.interact_pressed)
    };
    let holding_build = |id: PlayerId| ctx.inputs.get(&id).is_some_and(|input| input.build);

    // 읽기: 이번 Tick에 플레이어마다 가지게 될 상호작용을 정합니다.
    let mut next: BTreeMap<PlayerId, Option<Interaction>> = BTreeMap::new();
    let mut claimed: BTreeSet<PlayerId> = BTreeSet::new();
    let mut repairs: Vec<RepairTarget> = Vec::new();
    let mut gathers: Vec<(PlayerId, NodeId)> = Vec::new();
    let mut build_requests: Vec<PlayerId> = Vec::new();
    let mut pickups: Vec<(PlayerId, GroundWeaponId)> = Vec::new();
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
        if let Some(Interaction::Opening {
            chest,
            elapsed_ticks,
        }) = player.interaction()
            && holding(player.id)
            && loot::opening_still_valid(world, player, *chest, t)
        {
            next.insert(
                player.id,
                Some(Interaction::Opening {
                    chest: *chest,
                    elapsed_ticks: elapsed_ticks + 1,
                }),
            );
            continue;
        }
        // F를 한 번 두드리기만 해도 더 높은 순위의 대상이 있으면 줍기를 막습니다. 하지만 일을 시작하는 것은 누르고 있을 때뿐입니다.
        let target = if holding(player.id) || pressed(player.id) {
            choose_interaction(world, player, &claimed, t)
        } else {
            None
        };
        if target.is_none()
            && pressed(player.id)
            && let Some(weapon) = loot::nearest_ground_weapon(world, player, t)
        {
            pickups.push((player.id, weapon));
        }
        let mut chosen = if holding(player.id) { target } else { None };
        match chosen {
            Some(Interaction::Reviving { target, .. }) => {
                claimed.insert(target);
            }
            Some(Interaction::Repairing { target, .. }) => {
                if let Some(spec) = repair_spec(world, target, t) {
                    let previous = match player.interaction() {
                        Some(Interaction::Repairing {
                            target: before,
                            since_pulse_ticks,
                        }) if *before == target => Some(*since_pulse_ticks),
                        _ => None,
                    };
                    let (since_pulse_ticks, pulsed) =
                        count_pulse(previous, t.ticks(spec.heal.pulse_secs));
                    if pulsed {
                        repairs.push(target);
                    }
                    chosen = Some(Interaction::Repairing {
                        target,
                        since_pulse_ticks,
                    });
                }
            }
            Some(Interaction::Gathering { node, .. }) => {
                let previous = match player.interaction() {
                    Some(Interaction::Gathering {
                        node: before,
                        since_pulse_ticks,
                    }) if *before == node => Some(*since_pulse_ticks),
                    _ => None,
                };
                let (since_pulse_ticks, pulsed) =
                    count_pulse(previous, t.ticks(t.resources.gather_pulse_secs));
                if pulsed {
                    gathers.push((player.id, node));
                }
                chosen = Some(Interaction::Gathering {
                    node,
                    since_pulse_ticks,
                });
            }
            Some(Interaction::Building { .. }) | Some(Interaction::Opening { .. }) | None => {}
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
    apply_repairs(world, &repairs, t);
    resources::apply_gathers(world, &gathers, t);
    loot::apply_pickups(world, &pickups, t);
    build::start_builds(world, ctx, &build_requests);
}

/// 새로 시작할 상호작용을 고릅니다. 범위 안에 있는 것 가운데 다음 순서로 고르고, 같은 종류 안에서는 가장 가까운 것입니다.
/// 쓰러진 동료의 부활, 손상된 감시탑의 수리, 손상된 마차의 수리, 채집 지점의 채집, 무기 상자 열기.
/// 이 뒤에 바닥의 무기 줍기가 있지만, 줍기는 시간이 걸리는 일이 아니라서 `Interaction`이 아니고 `update_interactions`가 따로 처리합니다.
fn choose_interaction(
    world: &World,
    me: &Player,
    claimed: &BTreeSet<PlayerId>,
    t: &Tuning,
) -> Option<Interaction> {
    nearest_downed_ally(world, me, claimed, t)
        .or_else(|| nearest_damaged_tower(world, me, t))
        .or_else(|| damaged_wagon_in_reach(world, me, t))
        .or_else(|| resources::nearest_gatherable_node(world, me, t))
        .or_else(|| loot::nearest_openable_chest(world, me, t))
}

/// 살아 있고 하는 일이 없는 `me`가 지금 F를 누르면 하게 되는 일이 화면에 안내할 만한 것이면 그 안내입니다.
/// 서버가 F를 처리할 때와 같은 우선순위(`choose_interaction`, 그 뒤에 줍기)로 정하므로 안내가 서버의 판정과 어긋나지 않습니다.
/// 부활, 수리, 채집이 우선하면 안내가 없고, 상자를 열 수 있으면 바닥의 무기보다 상자의 안내가 앞섭니다. 다른 플레이어가 이미 부활시키는 동료는 서버처럼 고르지 않습니다.
pub(crate) fn prompt(world: &World, me: &Player, t: &Tuning) -> Option<Prompt> {
    if !me.is_free_to_act() {
        return None;
    }
    let claimed: BTreeSet<PlayerId> = world
        .players
        .values()
        .filter_map(|other| match other.interaction() {
            Some(Interaction::Reviving { target, .. }) => Some(*target),
            _ => None,
        })
        .collect();
    match choose_interaction(world, me, &claimed, t) {
        Some(Interaction::Opening { .. }) => Some(Prompt::OpenChest),
        Some(_) => None,
        None => loot::nearest_ground_weapon(world, me, t)
            .and_then(|id| world.ground_weapons.iter().find(|weapon| weapon.id == id))
            .map(|weapon| Prompt::TakeWeapon(weapon.kind)),
    }
}

/// 범위 안의 쓰러진 동료 가운데 가장 가까운 동료의 부활입니다. 이미 다른 플레이어가 부활시키는 동료(`claimed`)는 고르지 않습니다.
fn nearest_downed_ally(
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
    nearest.map(|(_, target)| Interaction::Reviving {
        target,
        elapsed_ticks: 1,
    })
}

/// 범위 안에서 손상된 가장 가까운 감시탑의 수리입니다. 파괴된 감시탑과 HP가 가득 찬 감시탑은 고르지 않습니다.
fn nearest_damaged_tower(world: &World, me: &Player, t: &Tuning) -> Option<Interaction> {
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
        if gap <= spec.repair.heal.range && nearest_tower.is_none_or(|(best, _)| gap < best) {
            nearest_tower = Some((gap, tower.id));
        }
    }
    nearest_tower.map(|(_, tower)| Interaction::Repairing {
        target: RepairTarget::Tower(tower),
        since_pulse_ticks: 0,
    })
}

/// 범위 안에 있는 손상된 마차의 수리입니다. 마차의 HP가 0이면 원정이 곧바로 실패하므로 고르지 않습니다.
fn damaged_wagon_in_reach(world: &World, me: &Player, t: &Tuning) -> Option<Interaction> {
    let wagon = &world.wagon;
    let damaged = wagon.hp > 0 && wagon.hp < t.wagon.max_hp;
    let gap = Body::Box(wagon.rect).gap_from(me.pos, t.player.radius);
    (damaged && me.team.is_allied_with(wagon.team) && gap <= t.wagon.repair.heal.range).then_some(
        Interaction::Repairing {
            target: RepairTarget::Wagon,
            since_pulse_ticks: 0,
        },
    )
}

/// 수리 대상의 수리 수치입니다. 대상인 감시탑이 사라졌으면 `None`입니다.
pub(crate) fn repair_spec<'a>(
    world: &World,
    target: RepairTarget,
    t: &'a Tuning,
) -> Option<&'a RepairTuning> {
    match target {
        RepairTarget::Tower(id) => world
            .towers
            .get(&id)
            .map(|tower| &t.structures.get(tower.kind).repair),
        RepairTarget::Wagon => Some(&t.wagon.repair),
    }
}

/// 이번 Tick에 수리의 간격이 찬 플레이어마다 보관함에서 비용을 내고 대상의 HP를 회복시킵니다. 플레이어 번호 순서로 하나씩 처리합니다.
/// 보관함의 자원이 비용보다 적으면 그 수리는 회복시키지 않고 아무것도 내지 않으므로, 보관함은 0 아래로 내려가지 않습니다.
/// 같은 Tick에 여럿이 수리하면 번호가 작은 플레이어가 먼저 내고, 자원이 모자라면 번호가 큰 플레이어의 수리가 회복시키지 못합니다.
/// 대상의 HP가 최대에 가까워서 회복시키는 양이 `hp_per_pulse`보다 적어도 비용은 같습니다.
/// 같은 Tick의 앞선 수리가 대상을 이미 최대 HP로 만들었으면, 뒤의 수리는 회복시킬 것이 없으므로 비용도 내지 않습니다.
fn apply_repairs(world: &mut World, pulses: &[RepairTarget], t: &Tuning) {
    for &target in pulses {
        let Some(spec) = repair_spec(world, target, t) else {
            continue;
        };
        if !is_below_max_hp(world, target, t) || world.stash < spec.cost_per_pulse {
            continue;
        }
        world.stash -= spec.cost_per_pulse;
        let amount = spec.heal.hp_per_pulse;
        match target {
            RepairTarget::Tower(id) => {
                if let Some(tower) = world.towers.get_mut(&id) {
                    let max_hp = t.structures.get(tower.kind).max_hp;
                    tower.repair(amount, max_hp);
                }
            }
            RepairTarget::Wagon => world.wagon.repair(amount, t.wagon.max_hp),
        }
    }
}

/// 수리할 대상의 HP가 최대보다 적은지 봅니다. 대상이 사라졌으면 `false`입니다.
fn is_below_max_hp(world: &World, target: RepairTarget, t: &Tuning) -> bool {
    match target {
        RepairTarget::Tower(id) => world
            .towers
            .get(&id)
            .is_some_and(|tower| tower.hp() < t.structures.get(tower.kind).max_hp),
        RepairTarget::Wagon => world.wagon.hp < t.wagon.max_hp,
    }
}

/// 수리나 채집의 한 Tick을 셉니다. `previous`는 같은 대상을 지난 Tick까지 센 값이고, 새로 시작하면 `None`이어서 1부터 셉니다.
/// 정해진 간격(`pulse_ticks`)이 차면 카운터를 0으로 돌리고 `true`를 함께 돌려줍니다.
fn count_pulse(previous: Option<u32>, pulse_ticks: u32) -> (u32, bool) {
    let counted = previous.map_or(1, |count| count + 1);
    if counted >= pulse_ticks {
        (0, true)
    } else {
        (counted, false)
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
            revived.life = Life::alive(hp, t);
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

/// 마차 곁에 있는 살아 있는 플레이어의 HP를 회복합니다. 낮과 밤, 공격받는 중인지와 관계없이 같고,
/// 쓰러졌거나 사망한 플레이어는 회복하지 않습니다.
///
/// 회복하는 때는 방의 Tick 번호가 정하므로 플레이어마다 따로 세는 값이 없고, 모든 플레이어가 같은 Tick에 회복합니다.
/// 이번 Tick의 피해를 모두 적용한 뒤에 부르므로, 쓰러뜨리는 피해를 같은 Tick의 회복이 되돌리지 않습니다.
pub(crate) fn recover_near_wagon(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    let heal = &t.wagon.recovery;
    if !ctx.tick.is_multiple_of(u64::from(t.ticks(heal.pulse_secs))) {
        return;
    }
    let wagon_body = Body::Box(world.wagon.rect);
    let wagon_team = world.wagon.team;
    for player in world.players.values_mut() {
        let in_reach = wagon_body.gap_from(player.pos, t.player.radius) <= heal.range
            && player.team.is_allied_with(wagon_team);
        if let Life::Alive { hp, .. } = &mut player.life
            && in_reach
        {
            *hp = hp.saturating_add(heal.hp_per_pulse).min(t.player.max_hp);
        }
    }
}

/// 이번 Tick에 플레이어마다 달리는지 정하고 달리기 게이지를 고칩니다. `move_players`가 이동 속도를 읽기 전에 부릅니다.
///
/// 달리는 플레이어는 살아 있고, 하고 있는 일이 없고(부활, 수리, 채집, 건설 중에는 움직일 수 없습니다), 움직이려 하고,
/// 달리기 키를 누르고 있고, 게이지에 한 Tick을 달릴 만큼이 남아 있는 플레이어입니다. 달리면 게이지가 Tick마다 줄고, 달리기 키를 떼고 있으면 Tick마다 찹니다.
/// 키를 누르고 있지만 달리지 못하는 동안(제자리에 서 있거나 게이지가 빈 상태)에는 게이지가 줄지도 차지도 않으므로,
/// 빈 게이지에서 키를 누르고 있어도 달리기와 걷기가 번갈아 나오지 않고 걷기만 합니다.
/// 입력이 없는 플레이어는 키를 떼고 있는 것으로 봅니다.
pub(crate) fn update_sprint(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    for player in world.players.values_mut() {
        let Life::Alive {
            stamina,
            sprinting,
            interaction,
            ..
        } = &mut player.life
        else {
            continue;
        };
        let input = ctx.inputs.get(&player.id);
        let holding = input.is_some_and(|input| input.sprint);
        let moving = input.is_some_and(|input| input.move_dir.get() != Vec2::ZERO);
        *sprinting = holding && moving && interaction.is_none() && stamina.can_spend(t);
        if *sprinting {
            stamina.spend(t);
        } else if !holding {
            stamina.refill(t);
        }
    }
}

/// 입력대로 플레이어를 움직입니다. 살아 있는 플레이어는 걷거나 달리고, 쓰러진 플레이어는 기어갑니다.
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
