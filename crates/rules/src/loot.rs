//! 낮에 찾는 무기입니다. 낮이 시작될 때 후보 자리 가운데 몇 곳에 무기 상자가 놓이고 상자마다 몬스터가 지킵니다.
//! 바닥에 놓인 무기를 플레이어가 F를 한 번 눌러서 줍습니다.
//!
//! ```text
//!   낮이 시작됨 --> 상자(자리와 내용이 씨앗에서 정해짐) + 그 둘레의 몬스터 3마리
//!   상자 --(밤이 시작됨)--> 사라짐(몬스터는 마차를 향함)
//!   바닥의 무기 --(F를 누른 순간)--> 플레이어의 빈 칸, 또는 든 무기와 맞바꿈(이전 무기가 그 자리에 놓임)
//!   바닥의 무기 --(원정이 끝남)--> 사라짐
//! ```
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
use crate::player::Player;
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
/// 같은 Tick에 두 명이 같은 무기를 노리면 번호가 작은 플레이어가 줍고, 다른 플레이어의 F는 아무 일도 하지 않습니다.
pub(crate) fn apply_pickups(world: &mut World, pickups: &[(PlayerId, GroundWeaponId)]) {
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
            None => {
                world.ground_weapons.remove(index);
            }
            Some(left_behind) => world.ground_weapons[index].kind = left_behind,
        }
    }
}
