//! 낮에 찾는 무기입니다. 바닥에 놓인 무기를 플레이어가 F를 한 번 눌러서 줍습니다.
//!
//! ```text
//!   바닥의 무기 --(F를 누른 순간)--> 플레이어의 빈 칸, 또는 든 무기와 맞바꿈(이전 무기가 그 자리에 놓임)
//!   바닥의 무기 --(원정이 끝남)--> 사라짐
//! ```
//!
//! 바닥의 무기는 땅에 떨어진 자원(`Pile`)처럼 길을 막지 않고, 원정이 끝날 때까지 그 자리에 남습니다.
//! 자원은 걸어서 지나가면 자동으로 줍지만 무기는 누른 순간에만 줍습니다. 자동으로 줍으면 서 있기만 해도 두 무기가 계속 서로 바뀌기 때문입니다.

mod dice;

use std::collections::BTreeSet;

pub use dice::Seed;

use crate::collision::Body;
use crate::ids::{GroundWeaponId, PlayerId};
use crate::kind::WeaponKind;
use crate::math::Vec2;
use crate::player::Player;
use crate::tuning::Tuning;
use crate::world::World;

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
