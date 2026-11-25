//! 몬스터가 나타나는 자리입니다. 밤의 몬스터가 나타나는 자리와, 낮의 몬스터가 채집 지점과 무기 상자를 지키는 자리가 있습니다.
//!
//! 밤의 자리는 마차를 둘러싼 여덟 방향마다 정합니다. 마차의 가장자리에서 `spawn_distance`만큼 떨어진 곳에,
//! 방향에 수직으로 `spawn_lanes`줄이 나란히 섭니다. 낮의 자리는 채집 지점과 상자의 후보 자리마다 그 타일에 이웃한 빈 타일입니다.
//! 자리는 바뀌지 않는 지형(장애물, 마차, 채집 지점, 상자의 후보 자리)만 보고 정하므로 원정을 시작할 때 한 번 만듭니다.
//! 감시탑은 마을의 범위 안에만 서고 밤의 자리는 그 범위 밖에 있으며, 채집 지점도 마을의 범위 밖에 있으므로, 감시탑이 어느 자리도 막지 못합니다.

use std::collections::BTreeSet;

use crate::math::Vec2;
use crate::tuning::Tuning;
use crate::world::{Tile, TileMap};

/// 밤의 무리가 나타나는 여덟 방향입니다. 삼각함수를 쓰지 않도록 상수로 적습니다.
const DIRECTIONS: [Vec2; 8] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(0.707_106_77, 0.707_106_77),
    Vec2::new(0.0, 1.0),
    Vec2::new(-0.707_106_77, 0.707_106_77),
    Vec2::new(-1.0, 0.0),
    Vec2::new(-0.707_106_77, -0.707_106_77),
    Vec2::new(0.0, -1.0),
    Vec2::new(0.707_106_77, -0.707_106_77),
];

pub(crate) struct SpawnPlan {
    /// 자리가 있는 방향마다 그 방향의 자리들입니다. 마차가 맵의 한쪽에 치우친 맵에서는 맵 밖으로 나가는 방향이 빠지고,
    /// 기본 맵에서는 여덟 방향이 모두 있습니다.
    night_spots: Vec<Vec<Vec2>>,
    /// 채집 지점마다 낮의 몬스터가 서는 타일입니다. 지점의 순서(맵에 적힌 순서)입니다.
    guard_posts: Vec<Vec<Tile>>,
    /// 상자의 후보 자리마다 낮의 몬스터가 서는 타일입니다. 후보 자리의 순서(맵에 적힌 순서)입니다.
    /// 상자는 후보 자리 가운데 일부에만 놓이므로, 놓일 수 있는 모든 자리의 몫을 미리 정해 둡니다.
    chest_posts: Vec<Vec<Tile>>,
}

impl SpawnPlan {
    /// 타일 지도와 수치에서 자리를 정합니다. 밤의 자리가 있는 방향이 하나도 없거나, 채집 지점이나 상자의 후보 자리 곁에 낮의 몬스터가 설 빈 타일이 모자라면 그 이유를 돌려줍니다.
    /// `Tuning::validate`와 `Sim::new`가 같은 함수를 부르므로, 검사를 통과한 수치에서는 `Sim::new`가 실패하지 않습니다.
    pub fn new(tiles: &TileMap, t: &Tuning) -> Result<SpawnPlan, String> {
        let wagon = t.map.wagon;
        let half = Vec2::new(
            (wagon.max.x - wagon.min.x) * 0.5,
            (wagon.max.y - wagon.min.y) * 0.5,
        );
        let radius = t.monsters.largest_radius();
        let lanes = t.nights.spawn_lanes.max(1);
        let near_village = t.village.build_range + 1.0;

        let mut night_spots = Vec::new();
        for direction in DIRECTIONS {
            // 그 방향으로 잰 마차의 가장자리까지의 거리를 더해서, 거리가 마차의 중심이 아니라 가장자리에서 재어지게 합니다.
            let reach = direction.x.abs() * half.x + direction.y.abs() * half.y;
            let anchor = wagon.center() + direction * (t.nights.spawn_distance + reach);
            let along = Vec2::new(-direction.y, direction.x);

            let mut spots = Vec::new();
            for lane in 0..lanes {
                let offset = (lane as f32 - (lanes - 1) as f32 * 0.5) * t.nights.spawn_lane_spacing;
                let wanted = anchor + along * offset;
                if !tiles.bounds().contains(wanted) {
                    continue;
                }
                let spot = if fits(tiles, wanted, radius) {
                    wanted
                } else {
                    match tiles.nearest_free(wanted) {
                        Some(tile) => tile.center(),
                        None => continue,
                    }
                };
                if wagon.distance_to(spot) >= near_village {
                    spots.push(spot);
                }
            }
            if !spots.is_empty() {
                night_spots.push(spots);
            }
        }
        if night_spots.is_empty() {
            return Err(
                "no direction has a free spot at spawn_distance from the wagon".to_string(),
            );
        }

        // 자리는 마을의 범위 밖에서만 고릅니다. 범위 안의 타일에는 감시탑이 설 수 있기 때문입니다.
        // 한 타일에는 몬스터가 하나만 서므로, 이웃한 두 지점이 같은 타일을 고르지 않게 이미 고른 타일은 건너뜁니다.
        // 후보 자리의 타일에는 상자가 놓일 수 있으므로 어느 몬스터도 서지 않습니다.
        let mut taken: BTreeSet<Tile> = t.map.chest_sites.iter().copied().collect();
        let guard_posts = posts_around(
            &t.map.nodes,
            t.guards.per_node,
            "gather node",
            tiles,
            t,
            &mut taken,
        )?;
        let chest_posts = posts_around(
            &t.map.chest_sites,
            t.guards.per_chest,
            "chest site",
            tiles,
            t,
            &mut taken,
        )?;
        Ok(SpawnPlan {
            night_spots,
            guard_posts,
            chest_posts,
        })
    }

    /// 낮이 시작될 때 낮의 몬스터가 서는 모든 타일입니다. 채집 지점의 순서이고, 지점 안에서는 정한 순서입니다.
    pub fn guard_posts(&self) -> impl Iterator<Item = Tile> + '_ {
        self.guard_posts.iter().flatten().copied()
    }

    /// `site`번째 후보 자리(`MapSpec::chest_sites`의 순서)에 상자가 놓일 때 그 상자를 지키는 몬스터가 서는 타일입니다.
    pub fn chest_posts(&self, site: usize) -> &[Tile] {
        &self.chest_posts[site]
    }

    /// 원정의 `serial`번째 무리(0부터 셉니다)의 `nth`번째 몬스터가 나타날 자리입니다.
    ///
    /// 무리마다 방향을 세 칸씩 건너뛰며 고릅니다. 방향이 여덟이면 3과 8이 서로소이므로 이어지는 두 무리는 135도 떨어지고,
    /// 여덟 방향을 모두 쓴 뒤에야 같은 방향이 다시 나옵니다. 방향의 수가 3의 배수이면 한 칸씩 건너뜁니다(그래야 모든 방향을 씁니다).
    pub fn night_spot(&self, serial: u32, nth: u32) -> Vec2 {
        let count = self.night_spots.len();
        let stride = if count.is_multiple_of(3) { 1 } else { 3 };
        let spots = &self.night_spots[(serial as usize * stride) % count];
        spots[nth as usize % spots.len()]
    }
}

/// `places`의 타일마다 이웃한 빈 타일 `wanted`개를 낮의 몬스터가 설 자리로 정합니다. 마을의 범위 밖에서만 고르고,
/// 이미 다른 곳이 고른 타일(`taken`)은 건너뜁니다. 한 타일에는 몬스터가 하나만 서기 때문입니다. 고른 타일은 `taken`에 더합니다.
fn posts_around(
    places: &[Tile],
    wanted: u32,
    name: &str,
    tiles: &TileMap,
    t: &Tuning,
    taken: &mut BTreeSet<Tile>,
) -> Result<Vec<Vec<Tile>>, String> {
    let wagon = t.map.wagon;
    let wanted = wanted as usize;
    let mut all = Vec::new();
    for place in places {
        let posts: Vec<Tile> = tiles
            .free_around(*place)
            .into_iter()
            .filter(|tile| !taken.contains(tile))
            .filter(|tile| wagon.distance_to(tile.center()) > t.village.build_range)
            .take(wanted)
            .collect();
        if posts.len() < wanted {
            return Err(format!(
                "{name} {place:?} has room for only {} of the {wanted} guards",
                posts.len()
            ));
        }
        taken.extend(posts.iter().copied());
        all.push(posts);
    }
    Ok(all)
}

/// 반지름 `radius`인 몸이 `p`에 서도 맵 안에 있고 막힌 타일에 걸치지 않는지 봅니다.
fn fits(tiles: &TileMap, p: Vec2, radius: f32) -> bool {
    if !tiles.bounds().shrink(radius).contains(p) {
        return false;
    }
    let mut near = Vec::new();
    tiles.blocked_rects_near(p, radius, &mut near);
    near.iter().all(|solid| solid.distance_to(p) >= radius)
}
