//! 카메라가 내 캐릭터를 따라가는 방식과, 화면 밖에 있는 마차나 보스를 가리키는 표시입니다. Bevy 없이 화면 좌표(픽셀)로 계산합니다.
//!
//! 맵은 64 x 64 타일이라서 한 화면에 담기지 않으므로, 카메라는 내 캐릭터를 따라가고 마차와 보스가 화면 밖이면 그것이 있는 쪽의 가장자리에 표시를 둡니다.
//! 화면 좌표는 `iso`의 쿼터뷰 화면 좌표(픽셀)이고 y가 위쪽입니다.

use caravan_rules::Vec2;

/// 목표가 이 거리(픽셀)보다 멀리 뛰면 따라가지 않고 곧바로 옮깁니다. 새 원정에서 캐릭터가 등장 지점으로 돌아가는 때입니다.
const SNAP_DISTANCE_PX: f32 = 600.0;
/// 카메라가 목표를 따라잡는 빠르기입니다. 남은 거리가 1초에 `1 - e^-8`(약 99.97%) 줄어듭니다.
const FOLLOW_PER_SEC: f32 = 8.0;

/// 카메라의 중심을 `target`(내 캐릭터의 화면 위치) 쪽으로 옮기고 새 중심을 돌려줍니다.
/// 남은 거리가 지수로 줄어들므로 프레임 시간이 달라도 같은 속도감이고, 한 프레임이 길어도 목표를 지나치지 않습니다.
/// `current`가 없는 첫 프레임이나 목표가 `SNAP_DISTANCE_PX`보다 멀리 뛴 프레임에는 곧바로 목표에 맞춥니다.
pub fn follow(current: Option<Vec2>, target: Vec2, dt_secs: f32) -> Vec2 {
    match current {
        Some(current) if current.distance(target) <= SNAP_DISTANCE_PX => {
            let step = 1.0 - (-FOLLOW_PER_SEC * dt_secs.max(0.0)).exp();
            current + (target - current) * step
        }
        _ => target,
    }
}

/// 창에 보이는 화면의 범위입니다. 중심과 크기는 화면 좌표(픽셀)입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRect {
    pub center: Vec2,
    pub size: Vec2,
}

/// 화면 가장자리에서 화면 밖의 한 지점(마차나 보스)을 가리키는 표시입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeMarker {
    /// 창 안의 위치입니다. 가로와 세로가 0.0에서 1.0 사이이고 (0, 0)이 왼쪽 위입니다. UI 노드의 `left`와 `top`을 백분율로 정하는 값입니다.
    pub anchor: Vec2,
    /// 내 캐릭터에서 그 지점까지의 지면 거리(타일)를 반올림한 값입니다.
    pub distance_tiles: u32,
}

/// 지점의 화면 위치(`point_screen`)가 `view`를 `inset_px`만큼 안쪽으로 줄인 범위 밖이면 표시를 돌려주고, 안이면 `None`입니다.
/// 표시는 화면의 중심에서 지점으로 그은 반직선이 안쪽으로 줄인 범위의 테두리와 만나는 점에 놓이므로, 지점이 있는 쪽을 가리킵니다.
pub fn edge_marker(
    view: ViewRect,
    point_screen: Vec2,
    me_ground: Vec2,
    point_ground: Vec2,
    inset_px: f32,
) -> Option<EdgeMarker> {
    let inner = Vec2::new(view.size.x * 0.5 - inset_px, view.size.y * 0.5 - inset_px);
    let offset = point_screen - view.center;
    if offset.x.abs() <= inner.x && offset.y.abs() <= inner.y {
        return None;
    }
    // 반직선이 어느 테두리에 먼저 닿는지입니다. 한 축의 거리가 0이면 그 축의 테두리에는 닿지 않습니다.
    let reach = |half: f32, distance: f32| {
        if distance == 0.0 {
            f32::INFINITY
        } else {
            half / distance.abs()
        }
    };
    let scale = reach(inner.x, offset.x).min(reach(inner.y, offset.y));
    let edge = offset * scale;
    Some(EdgeMarker {
        anchor: Vec2::new(
            (edge.x + view.size.x * 0.5) / view.size.x,
            (view.size.y * 0.5 - edge.y) / view.size.y,
        ),
        distance_tiles: me_ground.distance(point_ground).round() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_starts_on_its_target_and_then_eases_toward_it() {
        assert_eq!(
            follow(None, Vec2::new(100.0, 40.0), 0.016),
            Vec2::new(100.0, 40.0)
        );

        let next = follow(Some(Vec2::new(100.0, 0.0)), Vec2::new(200.0, 0.0), 0.05);
        assert!(next.x > 100.0 && next.x < 200.0, "{next:?}");
    }

    #[test]
    fn a_longer_frame_moves_the_camera_closer_but_never_past_the_target() {
        let from = Some(Vec2::ZERO);
        let target = Vec2::new(300.0, 0.0);

        let short = follow(from, target, 0.016).x;
        let long = follow(from, target, 0.5).x;
        let stalled = follow(from, target, 30.0).x;

        assert!(
            0.0 < short && short < long && long <= stalled,
            "{short} {long} {stalled}"
        );
        assert!(stalled <= 300.0 + 1e-3);
        assert_eq!(follow(from, target, 0.0), Vec2::ZERO, "no time, no move");
    }

    #[test]
    fn two_short_frames_come_as_close_as_one_frame_of_the_same_total_time() {
        let target = Vec2::new(300.0, 0.0);
        let once = follow(Some(Vec2::ZERO), target, 0.1);
        let twice = follow(Some(follow(Some(Vec2::ZERO), target, 0.05)), target, 0.05);

        assert!((once.x - twice.x).abs() < 1e-3, "{once:?} {twice:?}");
    }

    #[test]
    fn a_target_that_jumps_far_is_followed_at_once() {
        let jump = Vec2::new(SNAP_DISTANCE_PX + 1.0, 0.0);
        assert_eq!(follow(Some(Vec2::ZERO), jump, 0.016), jump);
    }

    fn view() -> ViewRect {
        ViewRect {
            center: Vec2::new(10.0, 20.0),
            size: Vec2::new(1280.0, 720.0),
        }
    }

    fn marker(wagon_screen: Vec2) -> Option<EdgeMarker> {
        edge_marker(view(), wagon_screen, Vec2::ZERO, Vec2::new(3.0, 4.0), 80.0)
    }

    #[test]
    fn a_wagon_on_the_screen_needs_no_marker() {
        assert_eq!(marker(Vec2::new(10.0, 20.0)), None, "in the middle");
        assert_eq!(
            marker(Vec2::new(10.0 + 560.0, 20.0 + 280.0)),
            None,
            "right at the corner of the inner rectangle"
        );
    }

    #[test]
    fn a_wagon_far_to_the_right_is_marked_on_the_right_edge_at_its_height() {
        let marker = marker(Vec2::new(2000.0, 20.0)).unwrap();

        assert!(
            (marker.anchor.x - (1.0 - 80.0 / 1280.0)).abs() < 1e-5,
            "{marker:?}"
        );
        assert!((marker.anchor.y - 0.5).abs() < 1e-5, "{marker:?}");
    }

    #[test]
    fn a_wagon_straight_up_is_marked_at_the_top_and_straight_down_at_the_bottom() {
        let up = marker(Vec2::new(10.0, 3000.0)).unwrap();
        assert!(
            (up.anchor.x - 0.5).abs() < 1e-5 && (up.anchor.y - 80.0 / 720.0).abs() < 1e-5,
            "{up:?}"
        );

        let down = marker(Vec2::new(10.0, -3000.0)).unwrap();
        assert!(
            (down.anchor.x - 0.5).abs() < 1e-5
                && (down.anchor.y - (1.0 - 80.0 / 720.0)).abs() < 1e-5,
            "{down:?}"
        );
    }

    #[test]
    fn a_wagon_up_and_to_the_left_is_marked_where_the_line_meets_the_inner_edge() {
        // 중심에서 (-3000, 3000)으로 가는 선은 위쪽 테두리(세로의 절반 280)에 먼저 닿으므로 x가 -280만큼 갑니다.
        let marker = marker(Vec2::new(10.0 - 3000.0, 20.0 + 3000.0)).unwrap();

        assert!(
            (marker.anchor.x - (640.0 - 280.0) / 1280.0).abs() < 1e-5,
            "{marker:?}"
        );
        assert!((marker.anchor.y - 80.0 / 720.0).abs() < 1e-5, "{marker:?}");
    }

    #[test]
    fn the_marker_stays_inside_the_window_wherever_the_wagon_is() {
        for (x, y) in [(900.0, 10.0), (-900.0, -300.0), (5.0, 400.0), (2e6, -2e6)] {
            let marker = marker(Vec2::new(x, y)).unwrap();
            assert!((0.0..=1.0).contains(&marker.anchor.x), "{marker:?}");
            assert!((0.0..=1.0).contains(&marker.anchor.y), "{marker:?}");
        }
    }

    #[test]
    fn the_marker_tells_how_many_tiles_away_the_wagon_is() {
        let far = edge_marker(
            view(),
            Vec2::new(2000.0, 20.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(32.0, 32.0),
            80.0,
        )
        .unwrap();

        assert_eq!(far.distance_tiles, 31, "31.1 tiles rounds down");
    }
}
