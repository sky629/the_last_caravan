//! 기본 수치가 규칙의 전제를 지키는지 확인합니다. 수치를 고치다가 전제가 깨지면 이 테스트가 실패합니다.

use crate::math::{Rect, Vec2};
use crate::tuning::Tuning;

#[test]
fn the_default_tuning_is_valid() {
    assert_eq!(Tuning::default().validate(), Ok(()));
}

#[test]
fn one_tick_of_walking_is_shorter_than_a_body_radius_so_nothing_tunnels() {
    let t = Tuning::default();
    assert!(t.player.speed * t.dt() < t.player.radius);
    assert!(t.monsters.grunt.speed * t.dt() < t.monsters.grunt.radius);
}

#[test]
fn a_tick_rate_that_would_let_bodies_tunnel_is_rejected() {
    let t = Tuning {
        tick_hz: 5,
        ..Tuning::default()
    };
    assert!(t.validate().is_err());
}

#[test]
fn a_spawn_point_inside_a_wall_is_rejected() {
    let mut t = Tuning::default();
    t.map.player_spawns = vec![Vec2::new(4.5, 4.5)];
    assert!(t.map.obstacles.contains(&Rect::new(4.0, 4.0, 8.0, 5.0)));
    assert!(t.validate().is_err());
}

#[test]
fn a_wave_with_no_monsters_is_rejected() {
    let mut t = Tuning::default();
    t.waves.sizes = vec![6, 0];
    assert!(t.validate().is_err());
}

#[test]
fn a_body_with_zero_max_hp_is_rejected() {
    let mut t = Tuning::default();
    t.monsters.grunt.max_hp = 0;
    assert!(t.validate().is_err());
}

#[test]
fn durations_are_written_in_seconds_and_become_whole_ticks_in_one_place() {
    let t = Tuning::default();
    assert_eq!(t.ticks(8.0), 160);
    assert_eq!(t.ticks(0.8), 16, "0.8 s does not round up to 17 ticks");
    assert_eq!(t.ticks(0.0), 0);
    assert_eq!(t.ticks(0.001), 1, "a real duration is at least one tick");

    let slow = Tuning {
        tick_hz: 10,
        ..Tuning::default()
    };
    assert_eq!(slow.ticks(8.0), 80);
}

#[test]
fn the_room_capacity_comes_from_the_tuning() {
    assert_eq!(Tuning::default().max_players, 4);
}
