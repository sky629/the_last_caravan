//! 낮에 찾는 무기입니다. 낮이 시작될 때 후보 자리 가운데 몇 곳에 무기 상자가 놓이고 상자마다 몬스터가 지킵니다.
//! 바닥에 놓인 무기를 플레이어가 F를 한 번 눌러서 줍습니다.
//!
//! ```text
//!   낮이 시작됨 --> 상자(자리와 내용이 씨앗에서 정해짐) + 그 둘레의 몬스터 3마리
//!   상자 --(F를 1.5초 끊지 않고 누름)--> 사라지고, 그 타일의 중심에 상자 안의 무기가 바닥의 무기로 놓임
//!   상자 --(밤이 시작됨)--> 사라짐(몬스터는 마차를 향함)
//!   바닥의 무기 --(F를 누른 순간)--> 플레이어의 빈 칸, 또는 든 무기와 맞바꿈(이전 무기가 그 자리에 놓임, 시작 무기와 같은 종류는 사라짐)
//!   바닥의 무기 --(원정이 끝남)--> 사라짐
//! ```
//!
//! 상자를 여는 일은 부활, 수리, 채집과 같이 F를 누르고 있는 동안의 행동(`Interaction::Opening`)이고, 진행도는 여는 플레이어에게만 있습니다.
//! 상자는 열릴 때까지 아무것도 기억하지 않습니다. 여는 동안 피해를 받거나 F를 놓으면 진행도가 처음으로 돌아가고,
//! 지키는 몬스터가 살아 있어도 맞지 않는 동안에는 열 수 있습니다. 두 명이 같은 상자를 열면 먼저 마친 쪽이 무기를 꺼내고 다른 쪽의 열기는 끝납니다.
//!
//! 상자는 채집 지점처럼 타일 하나를 차지하고 길을 막습니다. 타일 지도를 고치는 곳은 `World::add_chest`와 `World::remove_chest`뿐이고,
//! 감시탑과 같이 `layout_revision`을 올리므로 몬스터의 이동 경로가 다시 만들어집니다.
//! 바닥의 무기는 땅에 떨어진 자원(`Pile`)처럼 길을 막지 않고, 원정이 끝날 때까지 그 자리에 남습니다.
//! 자원은 걸어서 지나가면 자동으로 줍지만 무기는 누른 순간에만 줍습니다. 자동으로 줍으면 서 있기만 해도 두 무기가 계속 서로 바뀌기 때문입니다.
//!
//! 무작위는 `dice`가 만드는 `Rng`로만 쓰고, `Rng`는 이 모듈 밖에서 만들 수 없습니다. 무작위를 쓰는 함수는 `roll_day` 하나입니다.

mod dice;

use std::collections::BTreeSet;

use dice::Rng;

pub use dice::Seed;

use crate::collision::{Body, move_circle};
use crate::ids::{ChestId, GroundWeaponId, PlayerId};
use crate::kind::WeaponKind;
use crate::math::Vec2;
use crate::monster::Order;
use crate::player::{Interaction, Life, Player};
use crate::spawn::SpawnPlan;
use crate::tuning::Tuning;
use crate::world::{Tile, World};

/// 무기 상자입니다. 타일 하나를 차지하고 길을 막습니다. 내용은 상자가 놓일 때 정해지고, 스냅샷에는 실리지 않습니다.
/// `contents`가 이 크레이트 안에서만 보이므로, 프로토콜 크레이트가 내용을 읽으면 컴파일되지 않습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Chest {
    pub id: ChestId,
    pub tile: Tile,
    /// 열면 바닥에 놓이는 무기입니다.
    pub(crate) contents: WeaponKind,
}

/// 바닥에 놓인 무기입니다. 길을 막지 않고, 원정이 끝날 때까지 그 자리에 남습니다.
/// 다른 플레이어가 줍는 일은 무기를 바꿔 놓는 일(`kind`가 바뀜)이므로, 번호와 자리는 그대로이고 무기의 수는 늘지도 줄지도 않습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct GroundWeapon {
    pub id: GroundWeaponId,
    pub pos: Vec2,
    pub kind: WeaponKind,
}

/// F를 누르면 무엇을 하게 되는지 화면에 알리는 안내입니다. 부활, 수리, 채집은 따로 안내하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    /// F를 누르고 있으면 가까운 상자를 엽니다.
    OpenChest,
    /// F를 한 번 누르면 가까운 바닥의 무기를 줍습니다.
    TakeWeapon(WeaponKind),
}

/// 낮 하루에 뽑은 상자 하나입니다. `site`는 `MapSpec::chest_sites`의 순서입니다.
struct ChestRoll {
    site: usize,
    contents: WeaponKind,
}

/// 용도마다 씨앗을 따로 이어 가는 번호입니다. 번호를 바꾸면 같은 씨앗의 결과가 달라집니다.
const SITES: u64 = 1;
const CONTENTS: u64 = 2;

/// 원정 씨앗 `expedition`의 `day`번째 날에 놓일 상자를 뽑습니다. 무작위를 쓰는 유일한 함수입니다.
///
/// 자리는 후보 자리에서 서로 다른 `chests_per_day`곳을 뽑고, 내용은 자리마다 따로 무기 표의 `chest_weight`에 비례해서 뽑습니다.
/// 내용의 씨앗이 자리의 순서에서 이어지므로, 어느 자리의 상자 내용은 다른 상자가 어느 자리에 놓이는지와 관계없이 정해집니다.
fn roll_day(expedition: Seed, day: u32, t: &Tuning) -> Vec<ChestRoll> {
    let day_seed = expedition.derive(u64::from(day));
    let sites = Rng::new(day_seed.derive(SITES))
        .pick_distinct(t.loot.chests_per_day as usize, t.map.chest_sites.len());
    let weights: Vec<u32> = WeaponKind::ALL
        .iter()
        .map(|kind| t.weapons.get(*kind).chest_weight)
        .collect();
    sites
        .into_iter()
        .filter_map(|site| {
            let mut rng = Rng::new(day_seed.derive(CONTENTS).derive(site as u64));
            let kind = WeaponKind::ALL[rng.pick_weighted(&weights)?];
            Some(ChestRoll {
                site,
                contents: kind,
            })
        })
        .collect()
}

/// 낮이 시작될 때 상자를 놓고 상자마다 몬스터를 세웁니다. `begin_day`가 부릅니다.
/// 상자가 놓인 타일에 서 있던 플레이어는 타일 밖으로 밀려납니다.
pub(crate) fn start_day(
    world: &mut World,
    spawns: &SpawnPlan,
    expedition: Seed,
    day: u32,
    t: &Tuning,
) {
    for roll in roll_day(expedition, day, t) {
        world.add_chest(t.map.chest_sites[roll.site], roll.contents);
        for &post in spawns.chest_posts(roll.site) {
            world.spawn_monster(t.guards.kind, Order::Guard { post }, post.center(), t);
        }
    }
    let mut scratch = Vec::new();
    for player in world.players.values_mut() {
        player.pos = move_circle(
            player.pos,
            t.player.radius,
            Vec2::ZERO,
            &world.tiles,
            &mut scratch,
        );
    }
}

/// 밤이 시작될 때 열지 않은 상자를 모두 치웁니다. 타일은 다시 비워지고, 상자를 지키던 몬스터는 `nightfall`이 마차로 향하게 합니다.
pub(crate) fn clear_chests(world: &mut World) {
    let ids: Vec<ChestId> = world.chests.iter().map(|chest| chest.id).collect();
    for id in ids {
        world.remove_chest(id);
    }
}

/// `me`가 F를 누르고 있으면 열 수 있는 가장 가까운 상자의 열기입니다. 거리가 같으면 번호가 작은 상자입니다.
/// 지키는 몬스터가 살아 있는지는 보지 않습니다.
pub(crate) fn nearest_openable_chest(
    world: &World,
    me: &Player,
    t: &Tuning,
) -> Option<Interaction> {
    let mut nearest: Option<(f32, ChestId)> = None;
    for chest in &world.chests {
        let gap = chest_gap(chest, me, t);
        if gap <= t.loot.open_reach && nearest.is_none_or(|(best, _)| gap < best) {
            nearest = Some((gap, chest.id));
        }
    }
    nearest.map(|(_, chest)| Interaction::Opening {
        chest,
        elapsed_ticks: 1,
    })
}

/// `me`가 열고 있는 상자 `chest`가 아직 있고 닿는 거리 안인지 봅니다. 다른 플레이어가 먼저 열었거나 밤이 되어 상자가 사라졌으면 아닙니다.
pub(crate) fn opening_still_valid(world: &World, me: &Player, chest: ChestId, t: &Tuning) -> bool {
    world
        .chests
        .iter()
        .find(|candidate| candidate.id == chest)
        .is_some_and(|candidate| chest_gap(candidate, me, t) <= t.loot.open_reach)
}

/// 플레이어의 가장자리에서 상자의 가장자리까지의 거리입니다.
fn chest_gap(chest: &Chest, me: &Player, t: &Tuning) -> f32 {
    Body::Box(chest.tile.rect()).gap_from(me.pos, t.player.radius)
}

/// 열기를 마친 플레이어마다 상자를 열어서 그 타일의 중심에 상자 안의 무기를 놓고, 그 플레이어의 열기를 끝냅니다.
/// 한 Tick의 피해를 모두 적용한 뒤에 부릅니다.
///
/// 여는 플레이어가 이번 Tick에 피해를 받았으면 열기가 이미 취소되어(`Player::take_damage`) 여기까지 오지 못합니다.
/// 같은 상자를 두 명이 같은 Tick에 다 열어도 상자는 한 번만 열리므로 무기는 하나만 놓이고, 두 명의 열기가 모두 끝납니다.
/// 열기가 끝나면 하고 있는 일이 없으므로, F를 계속 누르고 있어도 다음 Tick에 새 일을 고릅니다.
/// 무기를 줍는 일은 F를 누른 순간에만 하므로, 누르고 있던 F가 방금 놓인 무기를 곧바로 줍지는 않습니다.
pub(crate) fn finish_openings(world: &mut World, t: &Tuning) {
    let required = t.ticks(t.loot.open_secs);
    let mut done: Vec<(PlayerId, ChestId)> = Vec::new();
    for player in world.players.values() {
        if let Some(Interaction::Opening {
            chest,
            elapsed_ticks,
        }) = player.interaction()
            && *elapsed_ticks >= required
        {
            done.push((player.id, *chest));
        }
    }
    for (opener, chest) in done {
        if let Some(Player {
            life: Life::Alive { interaction, .. },
            ..
        }) = world.players.get_mut(&opener)
        {
            *interaction = None;
        }
        if let Some(opened) = world.remove_chest(chest) {
            world.drop_weapon(opened.tile.center(), opened.contents);
        }
    }
}

/// `me`가 F를 한 번 누르면 줍게 되는 가장 가까운 바닥의 무기입니다. 거리가 같으면 번호가 작은 무기입니다.
pub(crate) fn nearest_ground_weapon(
    world: &World,
    me: &Player,
    t: &Tuning,
) -> Option<GroundWeaponId> {
    let mut nearest: Option<(f32, GroundWeaponId)> = None;
    for weapon in &world.ground_weapons {
        let point = Body::Circle {
            center: weapon.pos,
            radius: 0.0,
        };
        let gap = point.gap_from(me.pos, t.player.radius);
        if gap <= t.loot.take_reach && nearest.is_none_or(|(best, _)| gap < best) {
            nearest = Some((gap, weapon.id));
        }
    }
    nearest.map(|(_, id)| id)
}

/// 이번 Tick에 F를 눌러서 바닥의 무기를 줍는 플레이어들을 플레이어 번호 순서로 처리합니다.
/// 빈 칸이 있으면 무기가 빈 칸에 들어가고 바닥에서 사라집니다. 빈 칸이 없으면 든 무기와 바뀌어서, 이전에 든 무기가 바닥의 그 자리에 놓입니다.
/// 다만 이전에 든 무기가 시작 무기(`PlayerTuning::start_loadout`)와 같은 종류이면 바닥에 놓이지 않고 사라집니다.
/// 입장하는 플레이어마다 새로 받는 무기이므로, 바닥에 놓이면 입장과 퇴장을 되풀이해서 바닥의 무기를 끝없이 늘릴 수 있기 때문입니다.
/// 그래서 바닥의 무기는 모두 상자에서 나온 것입니다.
/// 같은 Tick에 두 명이 같은 무기를 노리면 번호가 작은 플레이어가 줍고, 다른 플레이어의 F는 아무 일도 하지 않습니다.
pub(crate) fn apply_pickups(world: &mut World, pickups: &[(PlayerId, GroundWeaponId)], t: &Tuning) {
    let mut taken: BTreeSet<GroundWeaponId> = BTreeSet::new();
    for &(player_id, weapon_id) in pickups {
        if !taken.insert(weapon_id) {
            continue;
        }
        let Some(player) = world.players.get_mut(&player_id) else {
            continue;
        };
        let Some(index) = world.ground_weapons.iter().position(|w| w.id == weapon_id) else {
            continue;
        };
        match player.loadout.take(world.ground_weapons[index].kind) {
            Some(left_behind) if !t.player.start_loadout.holds(left_behind) => {
                world.ground_weapons[index].kind = left_behind;
            }
            None | Some(_) => {
                world.ground_weapons.remove(index);
            }
        }
    }
}
