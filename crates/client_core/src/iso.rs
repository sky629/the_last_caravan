//! 지면 좌표를 쿼터뷰 화면 좌표로 바꾸는 투영입니다.
//!
//! 지면은 x가 동쪽, y가 북쪽이고 단위는 타일입니다. 화면은 픽셀이고 y가 위쪽입니다.
//! 타일 하나는 가로 48, 세로 24 픽셀의 마름모(2:1)로 그려집니다. 지면의 +x는 화면의 오른쪽 위로, +y는 왼쪽 위로 향합니다.
//! 따라서 지면의 (0, 0)은 맵 마름모의 아래쪽 꼭짓점이고 (너비, 높이)는 위쪽 꼭짓점입니다.

use caravan_rules::{Rect, Vec2};

pub const TILE_WIDTH_PX: f32 = 48.0;
pub const TILE_HEIGHT_PX: f32 = 24.0;
const HALF_TILE_WIDTH: f32 = TILE_WIDTH_PX / 2.0;
const HALF_TILE_HEIGHT: f32 = TILE_HEIGHT_PX / 2.0;

/// 맵 마름모의 위쪽에 남기는 여백입니다. 키가 큰 감시탑의 윗부분과 화면 위쪽의 HUD가 들어갈 자리입니다.
const VIEW_MARGIN_TOP_PX: f32 = 130.0;
/// 맵 마름모의 아래쪽 여백입니다. 화면 아래쪽의 진행 막대와 방 코드가 들어갈 자리입니다.
const VIEW_MARGIN_BOTTOM_PX: f32 = 80.0;
const VIEW_MARGIN_SIDE_PX: f32 = 40.0;

/// 지면 좌표(타일)를 화면 좌표(픽셀)로 바꿉니다.
pub fn ground_to_screen(ground: Vec2) -> Vec2 {
    Vec2::new(
        (ground.x - ground.y) * HALF_TILE_WIDTH,
        (ground.x + ground.y) * HALF_TILE_HEIGHT,
    )
}

/// `ground_to_screen`의 역변환입니다. 마우스가 가리키는 화면 위치에서 지면 위치를 구할 때 씁니다.
pub fn screen_to_ground(screen: Vec2) -> Vec2 {
    let x_minus_y = screen.x / HALF_TILE_WIDTH;
    let x_plus_y = screen.y / HALF_TILE_HEIGHT;
    Vec2::new((x_plus_y + x_minus_y) * 0.5, (x_plus_y - x_minus_y) * 0.5)
}

/// 지면에서 반지름이 `radius`(타일)인 원이 화면에서 그려지는 타원의 가로, 세로 반지름(픽셀)입니다.
/// 투영이 마름모 모양이므로 원은 가로로 긴 타원이 됩니다. 캐릭터의 충돌 원을 지면에 그릴 때 씁니다.
pub fn circle_to_screen_radii(radius: f32) -> Vec2 {
    Vec2::new(
        radius * HALF_TILE_WIDTH * std::f32::consts::SQRT_2,
        radius * HALF_TILE_HEIGHT * std::f32::consts::SQRT_2,
    )
}

/// 화면에서 누른 방향(W는 위쪽, D는 오른쪽)을 지면의 이동 방향으로 바꿉니다. 길이가 0이 아니면 길이 1입니다.
pub fn screen_dir_to_ground(dir: Vec2) -> Vec2 {
    Vec2::new(dir.x + dir.y, dir.y - dir.x).normalize_or_zero()
}

/// 그리는 순서를 정하는 값입니다. 클수록 화면에서 아래쪽에 있어서 앞에 그려집니다.
/// 발 위치를 넘기면 됩니다. 화면의 세로 위치는 `x + y`에 비례하기 때문입니다.
pub fn depth(foot: Vec2) -> f32 {
    -(foot.x + foot.y)
}

/// 사각형을 덮는 타일 하나하나의 중심입니다. 큰 상자를 타일 단위로 나누어 그릴 때 씁니다.
/// 타일마다 자기 중심으로 순서를 정해야, 캐릭터가 긴 벽의 한쪽 끝 옆에 서도 앞뒤가 맞습니다.
pub fn tile_centers(rect: Rect) -> Vec<Vec2> {
    let mut centers = Vec::with_capacity(rect.tile_count());
    let mut y = rect.min.y.floor();
    while y < rect.max.y {
        let mut x = rect.min.x.floor();
        while x < rect.max.x {
            centers.push(Vec2::new(x + 0.5, y + 0.5));
            x += 1.0;
        }
        y += 1.0;
    }
    centers
}

/// 카메라가 보여 줄 화면 영역입니다. 중심과 크기는 모두 화면 좌표(픽셀)입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapView {
    pub center: Vec2,
    pub size: Vec2,
}

/// 맵 전체와 HUD 여백이 들어가는 가장 작은 영역입니다. 창이 이보다 크면 카메라가 비율을 지키며 더 보여 줍니다.
pub fn map_view(map_size: Vec2) -> MapView {
    let corners = [
        Vec2::ZERO,
        Vec2::new(map_size.x, 0.0),
        Vec2::new(0.0, map_size.y),
        map_size,
    ]
    .map(ground_to_screen);
    let left = corners.iter().map(|c| c.x).fold(f32::INFINITY, f32::min) - VIEW_MARGIN_SIDE_PX;
    let right = corners
        .iter()
        .map(|c| c.x)
        .fold(f32::NEG_INFINITY, f32::max)
        + VIEW_MARGIN_SIDE_PX;
    let bottom = corners.iter().map(|c| c.y).fold(f32::INFINITY, f32::min) - VIEW_MARGIN_BOTTOM_PX;
    let top = corners
        .iter()
        .map(|c| c.y)
        .fold(f32::NEG_INFINITY, f32::max)
        + VIEW_MARGIN_TOP_PX;
    MapView {
        center: Vec2::new((left + right) * 0.5, (bottom + top) * 0.5),
        size: Vec2::new(right - left, top - bottom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn a_ground_point_survives_the_trip_to_the_screen_and_back() {
        for ground in [
            Vec2::new(0.0, 0.0),
            Vec2::new(3.5, 7.25),
            Vec2::new(23.9, 0.1),
        ] {
            assert!(close(screen_to_ground(ground_to_screen(ground)), ground));
        }
    }

    #[test]
    fn east_goes_up_right_and_north_goes_up_left() {
        let east = ground_to_screen(Vec2::new(1.0, 0.0));
        let north = ground_to_screen(Vec2::new(0.0, 1.0));
        assert_eq!(east, Vec2::new(24.0, 12.0));
        assert_eq!(north, Vec2::new(-24.0, 12.0));
    }

    #[test]
    fn w_moves_the_character_straight_up_the_screen() {
        let ground = screen_dir_to_ground(Vec2::new(0.0, 1.0));
        let on_screen = ground_to_screen(ground);
        assert!(on_screen.x.abs() < 1e-4 && on_screen.y > 0.0);
        assert!((ground.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn d_moves_right_s_moves_down_and_a_moves_left_on_the_screen() {
        let right = ground_to_screen(screen_dir_to_ground(Vec2::new(1.0, 0.0)));
        let down = ground_to_screen(screen_dir_to_ground(Vec2::new(0.0, -1.0)));
        let left = ground_to_screen(screen_dir_to_ground(Vec2::new(-1.0, 0.0)));
        assert!(right.x > 0.0 && right.y.abs() < 1e-4);
        assert!(down.y < 0.0 && down.x.abs() < 1e-4);
        assert!(left.x < 0.0 && left.y.abs() < 1e-4);
    }

    #[test]
    fn a_ground_circle_is_drawn_as_the_ellipse_that_holds_all_its_points() {
        let radius = 0.4;
        let radii = circle_to_screen_radii(radius);
        for step in 0..16 {
            let angle = step as f32 * std::f32::consts::TAU / 16.0;
            let on_circle = Vec2::new(radius * angle.cos(), radius * angle.sin());
            let screen = ground_to_screen(on_circle);
            let ellipse = (screen.x / radii.x).powi(2) + (screen.y / radii.y).powi(2);
            assert!((ellipse - 1.0).abs() < 1e-4, "step {step}: {ellipse}");
        }
    }

    #[test]
    fn no_screen_direction_means_no_ground_direction() {
        assert_eq!(screen_dir_to_ground(Vec2::ZERO), Vec2::ZERO);
    }

    #[test]
    fn the_point_lower_on_the_screen_is_drawn_in_front() {
        let south_of_wall = Vec2::new(10.5, 9.6);
        let wall_tile = Vec2::new(10.5, 10.5);
        let east_of_wall = Vec2::new(11.4, 10.5);
        assert!(depth(south_of_wall) > depth(wall_tile));
        assert!(depth(east_of_wall) < depth(wall_tile));
        assert!(ground_to_screen(south_of_wall).y < ground_to_screen(wall_tile).y);
    }

    #[test]
    fn a_rect_is_split_into_one_centre_per_tile() {
        let wagon = Rect::new(10.0, 11.0, 14.0, 13.0);
        let centers = tile_centers(wagon);
        assert_eq!(centers.len(), 8);
        assert_eq!(wagon.tile_count(), 8);
        assert!(centers.contains(&Vec2::new(10.5, 11.5)));
        assert!(centers.contains(&Vec2::new(13.5, 12.5)));
        assert_eq!(
            tile_centers(Rect::new(15.0, 11.0, 16.0, 12.0)),
            vec![Vec2::new(15.5, 11.5)]
        );
    }

    #[test]
    fn the_view_holds_the_whole_map_diamond_with_room_for_the_hud() {
        let view = map_view(Vec2::new(24.0, 24.0));
        let half = Vec2::new(view.size.x * 0.5, view.size.y * 0.5);
        for corner in [
            Vec2::ZERO,
            Vec2::new(24.0, 0.0),
            Vec2::new(0.0, 24.0),
            Vec2::new(24.0, 24.0),
        ] {
            let offset = ground_to_screen(corner) - view.center;
            assert!(offset.x.abs() <= half.x && offset.y.abs() <= half.y);
        }
        let top_of_map = ground_to_screen(Vec2::new(24.0, 24.0)).y;
        assert!(view.center.y + half.y - top_of_map >= VIEW_MARGIN_TOP_PX - 1e-3);
    }
}
