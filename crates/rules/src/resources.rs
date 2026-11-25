//! 자원입니다. 자원은 한 종류이고, 언제나 다음 네 곳 가운데 한 곳에만 있습니다.
//! 채집 지점(`GatherNode::amount`), 플레이어가 든 자원(`Life::Alive`의 `carried`), 땅에 떨어진 자원(`Pile`), 마차의 보관함(`World::stash`).
//!
//! ```text
//!   채집 지점 --(F를 누르고 있음, 정해진 간격마다)--> 플레이어가 듦 --(마차에 가까이 감)--> 보관함 --(건설을 시작함)--> 짓는 중인 감시탑
//!                                                        |                                                         |
//!                                              (쓰러짐, 접속을 끊음)                                     (건설을 취소함: 낸 만큼 돌아옴)
//!                                                        v                                                         v
//!                                                 땅에 떨어진 자원 --(살아 있는 플레이어가 가까이 감)--> 플레이어가 듦          보관함
//! ```
//!
//! 수리도 보관함의 자원을 씁니다. 수리의 한 번마다 보관함에서 비용만큼 없어지고(`player::apply_repairs`), 돌아오지 않습니다.
//!
//! 총량은 채집 지점에서 줄어든 만큼만 늘고, 건설을 마칠 때와 수리할 때만 줄어듭니다. 옮기고, 떨어뜨리고, 줍고, 건설을 취소하는 일은 총량을 바꾸지 않습니다.
//! 채집 지점은 낮이 시작될 때마다 다시 채워집니다.

use crate::collision::Body;
use crate::ids::{NodeId, PileId, PlayerId};
use crate::math::Vec2;
use crate::player::{Interaction, Life, Player};
use crate::tuning::Tuning;
use crate::world::{Ctx, Tile, World};

/// 채집 지점입니다. 맵에 고정된 타일 하나를 차지하고 길을 막습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct GatherNode {
    pub id: NodeId,
    pub tile: Tile,
    /// 남은 자원입니다. 낮이 시작될 때마다 `ResourceTuning::node_amount`로 돌아갑니다.
    pub amount: u32,
}

/// 땅에 떨어진 자원 한 더미입니다. 길을 막지 않고, 원정이 끝날 때까지 그 자리에 남습니다.
/// 존재하는 동안 `amount`는 항상 1 이상입니다. 다 주워 가면 `deposit_and_pickup`이 지웁니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Pile {
    pub id: PileId,
    pub pos: Vec2,
    pub amount: u32,
}

/// 맵에 적힌 채집 지점을 가득 찬 상태로 만듭니다. 번호는 맵의 순서대로 1부터 붙습니다.
/// 야영지의 세계에는 채집 지점이 없으므로, 어느 단계에 있는 방이든 원정의 채집 지점을 알려면 이 함수를 부릅니다.
pub fn nodes_of(t: &Tuning) -> Vec<GatherNode> {
    t.map
        .nodes
        .iter()
        .enumerate()
        .map(|(index, tile)| GatherNode {
            id: NodeId(index as u32 + 1),
            tile: *tile,
            amount: t.resources.node_amount,
        })
        .collect()
}

/// 모든 채집 지점을 가득 채웁니다. 낮이 시작될 때 부릅니다.
pub(crate) fn refill_nodes(world: &mut World, t: &Tuning) {
    for node in &mut world.nodes {
        node.amount = t.resources.node_amount;
    }
}

/// `me`가 F로 채집할 수 있는 가장 가까운 채집 지점입니다. 범위 안에 있고, 자원이 남아 있고, 든 자원이 상한보다 적어야 합니다.
/// 거리가 같으면 번호가 작은 지점입니다.
pub(crate) fn nearest_gatherable_node(
    world: &World,
    me: &Player,
    t: &Tuning,
) -> Option<Interaction> {
    let Life::Alive { carried, .. } = &me.life else {
        return None;
    };
    if *carried >= t.resources.carry_capacity {
        return None;
    }
    let mut nearest: Option<(f32, NodeId)> = None;
    for node in &world.nodes {
        let gap = Body::Box(node.tile.rect()).gap_from(me.pos, t.player.radius);
        if node.amount > 0
            && gap <= t.resources.gather_reach
            && nearest.is_none_or(|(best, _)| gap < best)
        {
            nearest = Some((gap, node.id));
        }
    }
    nearest.map(|(_, node)| Interaction::Gathering {
        node,
        since_pulse_ticks: 0,
    })
}

/// 이번 Tick에 채집의 간격이 찬 플레이어마다 자원을 지점에서 손으로 옮깁니다. 플레이어 번호 순서로 하나씩 처리합니다.
/// 옮기는 양은 한 번의 양, 지점에 남은 양, 들 수 있는 남은 양 가운데 가장 적은 값이므로, 여럿이 같은 지점을 채집해도 지점이 음수가 되지 않습니다.
pub(crate) fn apply_gathers(world: &mut World, gathers: &[(PlayerId, NodeId)], t: &Tuning) {
    for (player_id, node_id) in gathers {
        let Ok(index) = world.nodes.binary_search_by_key(node_id, |node| node.id) else {
            continue;
        };
        let Some(Life::Alive { carried, .. }) = world
            .players
            .get_mut(player_id)
            .map(|player| &mut player.life)
        else {
            continue;
        };
        let node = &mut world.nodes[index];
        let room = t.resources.carry_capacity.saturating_sub(*carried);
        let moved = t.resources.gather_per_pulse.min(node.amount).min(room);
        node.amount -= moved;
        *carried += moved;
    }
}

/// 이동을 마친 살아 있는 플레이어가 키를 누르지 않고 자원을 옮깁니다. 플레이어 번호 순서로 하나씩 처리합니다.
///
/// - 마차에서 `deposit_reach` 안에 있으면 든 자원을 모두 보관함에 넣습니다.
/// - 떨어진 자원에서 `pickup_reach` 안에 있으면 들 수 있는 만큼 줍습니다. 다 줍지 못한 양은 그 자리에 남습니다.
///
/// 같은 자원에 두 플레이어가 닿으면 번호가 작은 플레이어가 먼저 가져갑니다. 쓰러진 플레이어는 넣지도 줍지도 못합니다.
pub(crate) fn deposit_and_pickup(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    let wagon = Body::Box(world.wagon.rect);
    // 플레이어, 보관함, 떨어진 자원은 서로 다른 필드이므로 함께 고칠 수 있습니다.
    let World {
        players,
        stash,
        piles,
        ..
    } = world;
    for player in players.values_mut() {
        let Life::Alive { carried, .. } = &mut player.life else {
            continue;
        };
        if *carried > 0 && wagon.gap_from(player.pos, t.player.radius) <= t.resources.deposit_reach
        {
            *stash += *carried;
            *carried = 0;
        }
        for pile in piles.iter_mut() {
            let room = t.resources.carry_capacity.saturating_sub(*carried);
            let point = Body::Circle {
                center: pile.pos,
                radius: 0.0,
            };
            if room > 0 && point.gap_from(player.pos, t.player.radius) <= t.resources.pickup_reach {
                let taken = pile.amount.min(room);
                pile.amount -= taken;
                *carried += taken;
            }
        }
    }
    piles.retain(|pile| pile.amount > 0);
}
