//! 선분을 따라 움직이는 원이 무엇에 닿는지 구합니다. 탄환과 소총의 명중 판정이 쓰는 연속 충돌입니다.
//!
//! `collision`의 이동은 「이동한 뒤 밀어내기」라서 한 번의 이동이 몸의 반지름보다 짧아야 합니다. 이 파일은 이동한 뒤의 자리가 아니라
//! 이동하는 선분 전체를 검사하므로, 한 Tick에 얼마나 멀리 움직여도 얇은 몬스터나 벽을 건너뛸 수 없습니다. 그래서 속도에 상한이 없습니다.
//!
//! 모든 함수는 시작점 `from`과 이동량 `delta`를 받고, 닿는 자리를 `from + delta * t`의 `t`(0.0에서 1.0)로 돌려줍니다.
//! 시작할 때 이미 닿아 있으면 0.0이고, 닿지 않으면 `None`입니다. 사칙연산과 제곱근만 씁니다.

use crate::ids::{MonsterId, Team};
use crate::math::{Rect, Vec2};
use crate::monster::Monster;
use crate::world::{Ctx, TileMap};

/// 중심이 `from`에서 `from + delta`로 움직이는 점이 중심이 `center`이고 반지름이 `radius`인 원에 처음 닿는 `t`입니다.
/// 움직이는 원의 반지름이 `r`이면 `radius`에 `r`을 더해서 부릅니다.
pub(crate) fn circle_entry(from: Vec2, delta: Vec2, center: Vec2, radius: f32) -> Option<f32> {
    let offset = from - center;
    let c = offset.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    // |offset + delta * t|^2 = radius^2 의 작은 쪽 해입니다.
    let a = delta.length_squared();
    if a == 0.0 {
        return None;
    }
    let half_b = offset.dot(delta);
    let discriminant = half_b * half_b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = (-half_b - discriminant.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

/// 움직이는 점이 사각형에 처음 닿는 `t`입니다. 축마다 사각형의 두 변을 지나는 `t`의 구간을 구하고, 두 구간이 겹치는 처음이 닿는 때입니다.
pub(crate) fn rect_entry(from: Vec2, delta: Vec2, rect: Rect) -> Option<f32> {
    if rect.contains(from) {
        return Some(0.0);
    }
    let mut enter = 0.0_f32;
    let mut leave = 1.0_f32;
    for (p, d, low, high) in [
        (from.x, delta.x, rect.min.x, rect.max.x),
        (from.y, delta.y, rect.min.y, rect.max.y),
    ] {
        if d == 0.0 {
            if p < low || p > high {
                return None;
            }
            continue;
        }
        let (a, b) = ((low - p) / d, (high - p) / d);
        enter = enter.max(a.min(b));
        leave = leave.min(a.max(b));
        if enter > leave {
            return None;
        }
    }
    Some(enter)
}

/// 움직이는 점이 `area`의 안쪽에서 바깥으로 나가는 `t`입니다. 끝점이 `area` 안에 있으면 `None`입니다.
pub(crate) fn area_exit(from: Vec2, delta: Vec2, area: Rect) -> Option<f32> {
    let mut leave = f32::INFINITY;
    for (p, d, low, high) in [
        (from.x, delta.x, area.min.x, area.max.x),
        (from.y, delta.y, area.min.y, area.max.y),
    ] {
        if d > 0.0 {
            leave = leave.min((high - p) / d);
        } else if d < 0.0 {
            leave = leave.min((low - p) / d);
        }
    }
    (leave < 1.0).then_some(leave.max(0.0))
}

/// 반지름이 `radius`인 원이 `from`에서 `from + delta`로 움직일 때 막힌 타일이나 맵 경계에 처음 닿는 `t`입니다.
/// 막힌 곳은 타일 지도(`TileMap`)가 아는 곳 전부입니다. 장애물, 마차, 채집 지점, 포탑(짓는 중인 것도)입니다.
///
/// 원이 사각형에 닿는 것은 사각형을 `radius`만큼 키우고 점이 닿는 것으로 바꿔서 구합니다. 키운 사각형의 모서리는 둥글지 않고 각져 있으므로,
/// 모서리 근처에서는 원이 실제보다 `radius * (1.41 - 1)`까지 일찍 멈춥니다. `scratch`는 막힌 타일의 사각형을 담는 작업용 배열입니다.
pub(crate) fn first_blocked(
    tiles: &TileMap,
    from: Vec2,
    delta: Vec2,
    radius: f32,
    scratch: &mut Vec<Rect>,
) -> Option<f32> {
    // 선분 위의 모든 점은 중점에서 길이의 절반 안에 있으므로, 그 둘레의 타일만 보면 됩니다.
    let middle = from + delta * 0.5;
    tiles.blocked_rects_near(middle, delta.length() * 0.5 + radius, scratch);
    let walls = scratch.iter().filter_map(|solid| {
        let grown = Rect::new(
            solid.min.x - radius,
            solid.min.y - radius,
            solid.max.x + radius,
            solid.max.y + radius,
        );
        rect_entry(from, delta, grown)
    });
    let edge = area_exit(from, delta, tiles.bounds().shrink(radius));
    walls.chain(edge).min_by(f32::total_cmp)
}

/// `monsters_along`의 작업용 배열입니다. 부를 때마다 비우고 다시 채우므로, 한 Tick 동안 하나를 계속 써도 됩니다.
#[derive(Default)]
pub(crate) struct Strikes {
    /// 격자에서 찾은 후보 몬스터의 순서입니다.
    nearby: Vec<usize>,
    /// 닿는 `t`와 몬스터의 번호입니다. `t`가 작은 것이 앞이고, `t`가 같으면 번호가 작은 몬스터가 앞입니다.
    pub(crate) found: Vec<(f32, MonsterId)>,
}

/// 반지름이 `radius`인 원이 `from`에서 `from + delta`로 움직일 때 닿는 `team`과 적대적인 몬스터를 닿는 순서대로 `out.found`에 채웁니다.
/// 탄환은 앞에서부터 맞힐 수 있는 수만큼 맞히고, 곧바로 맞히는 무기는 첫 항목만 맞힙니다.
/// 중심이 몬스터의 반지름에 `radius`를 더한 거리 안으로 지나가면 닿은 것입니다.
pub(crate) fn monsters_along(
    team: Team,
    radius: f32,
    from: Vec2,
    delta: Vec2,
    monsters: &[Monster],
    ctx: &Ctx,
    out: &mut Strikes,
) {
    out.found.clear();
    // 선분 위의 모든 점은 중점에서 길이의 절반 안에 있으므로, 그 둘레의 몬스터만 후보입니다.
    let middle = from + delta * 0.5;
    let search_radius = delta.length() * 0.5 + radius + ctx.t.monsters.largest_radius();
    ctx.grid.query(middle, search_radius, &mut out.nearby);
    for &index in &out.nearby {
        let monster = &monsters[index];
        if !team.is_hostile_to(monster.team) {
            continue;
        }
        let reach = ctx.t.monsters.get(monster.kind).radius + radius;
        if let Some(at) = circle_entry(from, delta, monster.pos, reach) {
            out.found.push((at, monster.id));
        }
    }
    out.found
        .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_fast_point_cannot_jump_over_a_thin_circle() {
        // 한 번에 10타일을 움직이는 점이 반지름 0.1인 원의 한가운데를 지납니다. 끝점만 검사하면 지나칩니다.
        let from = Vec2::new(0.0, 0.0);
        let delta = Vec2::new(10.0, 0.0);
        let center = Vec2::new(4.9, 0.0);

        let t = circle_entry(from, delta, center, 0.1).expect("the line passes through the circle");

        assert!(close(t, 0.48), "t is {t}");
        assert!(
            (center - (from + delta)).length() > 0.1,
            "the end point is outside the circle"
        );
    }

    #[test]
    fn a_circle_off_the_line_or_behind_or_too_far_is_not_touched() {
        let from = Vec2::new(0.0, 0.0);
        let delta = Vec2::new(2.0, 0.0);

        assert_eq!(circle_entry(from, delta, Vec2::new(1.0, 1.5), 0.5), None);
        assert_eq!(circle_entry(from, delta, Vec2::new(-2.0, 0.0), 0.5), None);
        assert_eq!(circle_entry(from, delta, Vec2::new(5.0, 0.0), 0.5), None);
        assert_eq!(
            circle_entry(from, Vec2::ZERO, Vec2::new(1.0, 0.0), 0.5),
            None,
            "a point that does not move touches nothing outside it"
        );
        assert!(
            circle_entry(from, delta, Vec2::new(1.0, 0.5), 0.5).is_some(),
            "a graze still touches"
        );
    }

    #[test]
    fn a_point_that_starts_inside_touches_at_zero() {
        let delta = Vec2::new(1.0, 0.0);

        assert_eq!(
            circle_entry(Vec2::new(1.0, 1.0), delta, Vec2::new(1.2, 1.0), 0.5),
            Some(0.0)
        );
        assert_eq!(
            rect_entry(Vec2::new(1.5, 1.5), delta, Rect::new(1.0, 1.0, 2.0, 2.0)),
            Some(0.0)
        );
    }

    #[test]
    fn a_rect_is_entered_through_the_nearest_side() {
        let rect = Rect::new(4.0, 2.0, 5.0, 4.0);

        let t = rect_entry(Vec2::new(0.0, 3.0), Vec2::new(10.0, 0.0), rect)
            .expect("the line crosses the rect");
        assert!(close(t, 0.4), "t is {t}");
        assert_eq!(
            rect_entry(Vec2::new(0.0, 5.0), Vec2::new(10.0, 0.0), rect),
            None,
            "passes above"
        );
        assert_eq!(
            rect_entry(Vec2::new(0.0, 3.0), Vec2::new(3.0, 0.0), rect),
            None,
            "stops short"
        );
    }

    #[test]
    fn the_exit_of_an_area_is_where_the_point_crosses_its_edge() {
        let area = Rect::new(0.0, 0.0, 10.0, 10.0);

        let t = area_exit(Vec2::new(8.0, 5.0), Vec2::new(4.0, 0.0), area)
            .expect("the point leaves through the east side");
        assert!(close(t, 0.5), "t is {t}");
        assert_eq!(
            area_exit(Vec2::new(2.0, 5.0), Vec2::new(4.0, 0.0), area),
            None,
            "the end point is inside"
        );
        assert_eq!(
            area_exit(Vec2::new(2.0, 5.0), Vec2::ZERO, area),
            None,
            "a point that does not move stays"
        );
    }
}
