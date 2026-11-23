//! 위치와 방향을 정수로 줄였다가 되돌릴 때의 오차가 `MAX_ERROR_*`를 넘지 않는지 확인합니다.

use std::f32::consts::TAU;

use caravan_protocol::{WireAngle, WirePos, WireRect};
use caravan_rules::{Rect, Vec2};

/// 두 각도(라디안) 사이의 차이를 0 이상 반 바퀴 이하로 구합니다.
fn angle_between(a: Vec2, b: Vec2) -> f32 {
    let from_a = a.y.atan2(a.x);
    let from_b = b.y.atan2(b.x);
    let turn = (from_a - from_b).rem_euclid(TAU);
    turn.min(TAU - turn)
}

#[test]
fn a_position_anywhere_on_the_map_comes_back_within_the_stated_error() {
    // 0.0137은 눈금(1/128)의 배수가 아니므로 모든 눈금 위치를 골고루 지납니다.
    for i in 0..=1752 {
        for j in (0..=1752).step_by(97) {
            let original = Vec2::new(i as f32 * 0.0137, j as f32 * 0.0137);
            let back = WirePos::from_tiles(original).to_tiles();
            assert!(
                (back.x - original.x).abs() <= WirePos::MAX_ERROR_TILES
                    && (back.y - original.y).abs() <= WirePos::MAX_ERROR_TILES,
                "{original:?} came back as {back:?}"
            );
        }
    }
}

#[test]
fn positions_on_the_grid_are_exact_and_the_whole_map_fits_in_two_bytes() {
    let spawn = Vec2::new(12.0, 9.5);
    assert_eq!(
        WirePos::from_tiles(spawn),
        WirePos { x: 1536, y: 1216 },
        "128 units per tile"
    );
    assert_eq!(WirePos::from_tiles(spawn).to_tiles(), spawn);

    let far_corner = WirePos::from_tiles(Vec2::new(24.0, 24.0));
    assert!(
        far_corner.x < 16384 && far_corner.y < 16384,
        "a varint below 16384 is two bytes"
    );
}

#[test]
fn positions_outside_the_range_are_pinned_and_broken_numbers_become_zero() {
    let pin = |x: f32, y: f32| WirePos::from_tiles(Vec2::new(x, y));
    assert_eq!(pin(-3.0, -0.001), WirePos { x: 0, y: 0 });
    assert_eq!(
        pin(1.0e9, f32::INFINITY),
        WirePos {
            x: u16::MAX,
            y: u16::MAX
        }
    );
    assert_eq!(pin(f32::NAN, f32::NEG_INFINITY), WirePos { x: 0, y: 0 });
}

#[test]
fn a_rect_keeps_its_corners() {
    let wagon = Rect::new(10.0, 11.0, 14.0, 13.0);
    assert_eq!(WireRect::from_rect(wagon).to_rect(), wagon);
}

#[test]
fn a_direction_in_any_quadrant_comes_back_within_the_stated_error() {
    let mut degrees = 0.0_f32;
    while degrees < 360.0 {
        let radians = degrees.to_radians();
        let original = Vec2::new(radians.cos(), radians.sin());
        let back = WireAngle::from_direction(original).to_direction();
        let error = angle_between(original, back);
        assert!(
            error <= WireAngle::MAX_ERROR_RAD + 1.0e-4,
            "{degrees} degrees was off by {error} rad"
        );
        assert!(
            (back.length() - 1.0).abs() < 1.0e-5,
            "a decoded direction has length 1"
        );
        degrees += 0.37;
    }
}

#[test]
fn the_four_compass_directions_land_on_exact_steps() {
    let step = |x: f32, y: f32| WireAngle::from_direction(Vec2::new(x, y)).0;
    assert_eq!(step(1.0, 0.0), 0, "east");
    assert_eq!(step(0.0, 1.0), 64, "north");
    assert_eq!(step(-1.0, 0.0), 128, "west");
    assert_eq!(step(0.0, -1.0), 192, "south");
}

#[test]
fn only_the_direction_matters_and_unusable_vectors_face_east() {
    let step = |x: f32, y: f32| WireAngle::from_direction(Vec2::new(x, y)).0;
    assert_eq!(step(7.0, 0.0), step(1.0, 0.0), "length is ignored");
    assert_eq!(step(0.0, 0.0), 0);
    assert_eq!(step(f32::NAN, 1.0), 0);
}

#[test]
fn every_one_of_the_256_steps_survives_a_round_trip() {
    for raw in 0..=255u8 {
        let again = WireAngle::from_direction(WireAngle(raw).to_direction());
        assert_eq!(again, WireAngle(raw), "step {raw}");
    }
}
