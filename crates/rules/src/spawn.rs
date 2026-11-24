//! 밤의 몬스터가 나타나는 자리입니다.
//!
//! 자리는 마차를 둘러싼 여덟 방향마다 정합니다. 마차의 가장자리에서 `spawn_distance`만큼 떨어진 곳에,
//! 방향에 수직으로 `spawn_lanes`줄이 나란히 섭니다. 자리는 바뀌지 않는 지형(장애물, 마차)만 보고 정하므로 원정을 시작할 때 한 번 만듭니다.
//! 감시탑은 마을의 범위 안에만 서고 자리는 그 범위 밖에 있으므로, 감시탑이 자리를 막지 못합니다.

use crate::math::Vec2;
use crate::tuning::Tuning;
use crate::world::TileMap;

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
}

impl SpawnPlan {
    /// 타일 지도와 수치에서 자리를 정합니다. 자리가 있는 방향이 하나도 없으면 그 이유를 돌려줍니다.
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
        Ok(SpawnPlan { night_spots })
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

/// 반지름 `radius`인 몸이 `p`에 서도 맵 안에 있고 막힌 타일에 걸치지 않는지 봅니다.
fn fits(tiles: &TileMap, p: Vec2, radius: f32) -> bool {
    if !tiles.bounds().shrink(radius).contains(p) {
        return false;
    }
    let mut near = Vec::new();
    tiles.blocked_rects_near(p, radius, &mut near);
    near.iter().all(|solid| solid.distance_to(p) >= radius)
}
