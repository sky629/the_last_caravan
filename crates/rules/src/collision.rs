//! 충돌 처리입니다. 플레이어와 몬스터는 원이고, 장애물, 마차, 감시탑은 타일 하나하나의 정사각형입니다.
//! 맵 경계는 원이 그 안에 머물러야 하는 사각형 하나로 다룹니다. 원끼리는 충돌하지 않습니다.
//!
//! 방식은 「이동한 뒤 밀어내기」입니다. 원이 한 Tick의 이동 거리만큼 이동한 뒤, 겹친 사각형에서 가장 짧은 방향으로 밀려납니다.
//! 막힌 곳은 타일 지도에서 읽고, 원이 걸치는 타일(대개 3 x 3)만 봅니다. 그래서 감시탑이 몇 개이든 이동 한 번의 비용이 같습니다.
//! 벽에 비스듬히 부딪히면 벽에 수직인 성분만 사라지므로 벽을 따라 미끄러집니다.
//! 이 방식에는 한 Tick에 움직일 수 있는 거리(걷는 거리와 몬스터끼리 밀려나는 거리의 합)가 반지름보다 짧아야 한다는 전제가 있습니다.
//! 그렇지 않으면 얇은 벽을 건너뛸 수 있습니다. `Tuning::validate`가 이 전제를 검사합니다.

use crate::math::{Rect, Vec2};
use crate::world::TileMap;

/// 거리를 잴 때 쓰는 모양입니다. 근접 공격, 추적 시작, 수리의 거리 판정이 모두 `gap_from` 하나를 거칩니다.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Body {
    Circle { center: Vec2, radius: f32 },
    Box(Rect),
}

impl Body {
    /// 이 모양의 가장자리나 안쪽에서 `p`와 가장 가까운 점입니다.
    pub fn closest_point(&self, p: Vec2) -> Vec2 {
        match self {
            Body::Circle { center, radius } => {
                let offset = p - *center;
                if offset.length() <= *radius {
                    p
                } else {
                    *center + offset.normalize_or_zero() * *radius
                }
            }
            Body::Box(rect) => rect.closest_point(p),
        }
    }

    /// 중심이 `center`이고 반지름이 `radius`인 원의 가장자리에서 이 모양의 가장자리까지의 거리입니다.
    /// 닿거나 겹치면 0입니다.
    pub fn gap_from(&self, center: Vec2, radius: f32) -> f32 {
        let distance = match self {
            Body::Circle {
                center: own_center,
                radius: own_radius,
            } => center.distance(*own_center) - own_radius,
            Body::Box(rect) => rect.distance_to(center),
        };
        (distance - radius).max(0.0)
    }
}

/// 원을 `delta`만큼 옮긴 뒤 막힌 타일과 맵 경계에 맞추어 보정하고, 새 중심을 돌려줍니다.
/// `scratch`는 막힌 타일의 사각형을 담는 작업용 배열입니다. 호출하는 쪽이 재사용하면 이동마다 메모리를 새로 할당하지 않습니다.
pub(crate) fn move_circle(
    pos: Vec2,
    radius: f32,
    delta: Vec2,
    tiles: &TileMap,
    scratch: &mut Vec<Rect>,
) -> Vec2 {
    let area = tiles.bounds().shrink(radius);
    let mut p = pos + delta;
    // 한 사각형에서 밀려난 원이 이웃한 사각형으로 들어갈 수 있으므로 두 번 반복합니다.
    for _ in 0..2 {
        tiles.blocked_rects_near(p, radius, scratch);
        for solid in scratch.iter() {
            p = push_out(p, radius, *solid);
        }
        p = clamp_inside(p, area);
    }
    p
}

/// 원이 `solid`와 겹쳐 있으면 겹치지 않는 가장 가까운 위치로 옮깁니다.
fn push_out(p: Vec2, radius: f32, solid: Rect) -> Vec2 {
    let closest = solid.closest_point(p);
    let offset = p - closest;
    let distance = offset.length();
    if distance >= radius {
        return p;
    }
    if distance > 0.0 {
        return closest + offset * (radius / distance);
    }
    // 원의 중심이 사각형 안에 있는 경우입니다. 가장 가까운 변을 지나 반지름만큼 바깥으로 보냅니다.
    let to_left = p.x - solid.min.x;
    let to_right = solid.max.x - p.x;
    let to_bottom = p.y - solid.min.y;
    let to_top = solid.max.y - p.y;
    let nearest = to_left.min(to_right).min(to_bottom).min(to_top);
    if nearest == to_left {
        Vec2::new(solid.min.x - radius, p.y)
    } else if nearest == to_right {
        Vec2::new(solid.max.x + radius, p.y)
    } else if nearest == to_bottom {
        Vec2::new(p.x, solid.min.y - radius)
    } else {
        Vec2::new(p.x, solid.max.y + radius)
    }
}

fn clamp_inside(p: Vec2, area: Rect) -> Vec2 {
    Vec2::new(
        p.x.max(area.min.x).min(area.max.x),
        p.y.max(area.min.y).min(area.max.y),
    )
}
