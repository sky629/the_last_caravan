//! 이동과 충돌을 확인합니다.

use super::support::{Scenario, close};
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::math::{Rect, Vec2};

/// x = 12..13을 지나는 세로 벽입니다. 맵의 위아래 끝까지 이어집니다.
fn with_wall() -> Scenario {
    Scenario::with_tuning(|t| t.map.obstacles = vec![Rect::new(12.0, 0.0, 13.0, 20.0)])
}

#[test]
fn a_player_walks_at_the_configured_speed() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 10.0);
    s.hold_move(a, 1.0, 0.0);

    s.run_ticks(s.ticks(1.0));

    let speed = s.sim.tuning().player.speed;
    assert!(close(s.player(a).pos.x, 5.0 + speed));
    assert!(close(s.player(a).pos.y, 10.0));
}

#[test]
fn moving_diagonally_is_no_faster_than_moving_straight() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.hold_move(a, 1.0, 1.0);

    s.run_ticks(s.ticks(1.0));

    let walked = s.player(a).pos.distance(Vec2::new(5.0, 5.0));
    assert!(close(walked, s.sim.tuning().player.speed));
}

#[test]
fn a_player_with_no_input_stands_still() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);

    s.run_ticks(10);

    assert_eq!(s.player(a).pos, Vec2::new(5.0, 5.0));
}

#[test]
fn a_move_request_longer_than_one_is_clamped_to_full_speed() {
    assert!(close(MoveDir::new(100.0, 0.0).get().length(), 1.0));
    assert!(close(MoveDir::new(3.0, 4.0).get().length(), 1.0));
    assert_eq!(MoveDir::new(0.3, 0.4).get(), Vec2::new(0.3, 0.4));
}

#[test]
fn a_move_request_that_is_not_a_number_means_standing_still() {
    assert_eq!(MoveDir::new(f32::NAN, 1.0).get(), Vec2::ZERO);
    assert_eq!(MoveDir::new(f32::INFINITY, 0.0).get(), Vec2::ZERO);
}

#[test]
fn an_aim_request_is_normalized_and_an_empty_one_keeps_the_facing() {
    assert!(close(AimDir::new(0.0, 7.0).get().unwrap().length(), 1.0));
    assert_eq!(AimDir::new(0.0, 0.0).get(), None);
    assert_eq!(AimDir::new(f32::NAN, 1.0).get(), None);

    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.hold(
        a,
        PlayerInput {
            aim: AimDir::new(0.0, 5.0),
            ..PlayerInput::default()
        },
    );
    s.run_ticks(1);
    assert_eq!(s.player(a).facing, Vec2::new(0.0, 1.0));

    s.hold(a, PlayerInput::default());
    s.run_ticks(1);
    assert_eq!(s.player(a).facing, Vec2::new(0.0, 1.0), "no aim keeps it");
}

#[test]
fn the_sequence_number_of_the_last_input_is_recorded() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.hold(
        a,
        PlayerInput {
            seq: 7,
            ..PlayerInput::default()
        },
    );

    s.run_ticks(1);

    assert_eq!(s.player(a).last_input_seq, 7);
}

#[test]
fn a_player_cannot_walk_through_an_obstacle() {
    let mut s = with_wall();
    let a = s.player_at(10.0, 10.0);
    s.hold_move(a, 1.0, 0.0);

    s.run_ticks(s.ticks(5.0));

    let radius = s.sim.tuning().player.radius;
    assert!(close(s.player(a).pos.x, 12.0 - radius), "C11");
}

#[test]
fn a_player_slides_along_a_wall_when_moving_into_it_at_an_angle() {
    let mut s = with_wall();
    let a = s.player_at(11.0, 5.0);
    s.hold_move(a, 1.0, 1.0);

    s.run_ticks(s.ticks(1.0));

    let p = s.player(a).pos;
    let radius = s.sim.tuning().player.radius;
    assert!(close(p.x, 12.0 - radius), "blocked along x");
    assert!(
        p.y > 5.0 + 2.5,
        "still moves along the wall, got y = {}",
        p.y
    );
}

#[test]
fn a_player_cannot_leave_the_map() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let radius = s.sim.tuning().player.radius;

    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        s.hold_move(a, dx, dy);
        s.run_ticks(s.ticks(10.0));
        let p = s.player(a).pos;
        assert!(
            p.x >= radius - 1e-3 && p.x <= 20.0 - radius + 1e-3,
            "x = {}",
            p.x
        );
        assert!(
            p.y >= radius - 1e-3 && p.y <= 20.0 - radius + 1e-3,
            "y = {}",
            p.y
        );
    }
}

#[test]
fn the_wagon_and_a_tower_block_players() {
    let mut s = Scenario::open_field();
    let radius = s.sim.tuning().player.radius;

    let toward_wagon = s.player_at(15.0, 18.0);
    s.hold_move(toward_wagon, 1.0, 0.0);
    let toward_tower = s.player_at(4.0, 17.5);
    s.hold_move(toward_tower, -1.0, 0.0);
    s.run_ticks(s.ticks(5.0));

    assert!(close(s.player(toward_wagon).pos.x, 17.0 - radius));
    assert!(close(s.player(toward_tower).pos.x, 2.0 + radius));
}

#[test]
fn a_player_pressed_into_a_corner_stays_out_of_both_walls() {
    let mut s = Scenario::with_tuning(|t| {
        t.map.obstacles = vec![
            Rect::new(12.0, 0.0, 13.0, 20.0),
            Rect::new(0.0, 12.0, 20.0, 13.0),
        ];
    });
    let a = s.player_at(10.0, 10.0);
    s.hold_move(a, 1.0, 1.0);

    s.run_ticks(s.ticks(5.0));

    let p = s.player(a).pos;
    let radius = s.sim.tuning().player.radius;
    assert!(
        p.x <= 12.0 - radius + 1e-3 && p.y <= 12.0 - radius + 1e-3,
        "{p:?}"
    );
}

#[test]
fn a_fast_player_never_tunnels_through_a_one_tile_wall() {
    let mut s = with_wall();
    let a = s.player_at(11.0, 10.0);
    s.hold_move(a, 1.0, 0.3);

    for _ in 0..400 {
        s.run_ticks(1);
        assert!(
            s.player(a).pos.x < 12.0,
            "crossed the wall: {:?}",
            s.player(a).pos
        );
    }
}

#[test]
fn players_walk_through_each_other() {
    let mut s = Scenario::open_field();
    let walker = s.player_at(5.0, 10.0);
    let standing = s.player_at(6.0, 10.0);
    s.hold_move(walker, 1.0, 0.0);

    s.run_ticks(s.ticks(1.0));

    assert!(s.player(walker).pos.x > 8.0, "passed the other player");
    assert_eq!(s.player(standing).pos, Vec2::new(6.0, 10.0));
}
