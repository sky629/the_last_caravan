//! 기본 수치가 규칙의 전제를 지키는지 확인합니다. 수치를 고치다가 전제가 깨지면 이 테스트가 실패합니다.

use super::support::{Scenario, nights};
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
    t.map.player_spawns = vec![Vec2::new(24.5, 24.5)];
    assert!(t.map.obstacles.contains(&Rect::new(24.0, 24.0, 28.0, 25.0)));
    assert!(t.validate().is_err());
}

#[test]
fn a_map_whose_free_tiles_are_cut_off_from_the_wagon_is_rejected() {
    let mut t = Tuning::default();
    t.map.obstacles = vec![Rect::new(0.0, 5.0, 64.0, 6.0)];
    assert!(t.validate().is_err(), "a wall across the whole map");

    t.map.obstacles = vec![Rect::new(0.0, 5.0, 63.0, 6.0)];
    assert_eq!(t.validate(), Ok(()), "a gap of one tile keeps it connected");
}

#[test]
fn an_obstacle_that_does_not_fit_the_tile_grid_is_rejected() {
    let mut t = Tuning::default();
    t.map.obstacles = vec![Rect::new(4.0, 4.0, 8.5, 5.0)];
    assert!(t.validate().is_err());
}

#[test]
fn a_body_as_wide_as_a_tile_is_rejected() {
    let mut t = Tuning::default();
    t.player.radius = 0.5;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.monsters.grunt.radius = 0.5;
    assert!(t.validate().is_err());
}

#[test]
fn a_night_without_waves_and_a_wave_without_monsters_are_rejected() {
    let mut t = Tuning::default();
    t.nights.plans = nights(&[&[6, 0]]);
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.nights.plans = nights(&[&[6], &[]]);
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.nights.plans = Vec::new();
    assert!(t.validate().is_err());
}

#[test]
fn night_monsters_must_appear_beyond_the_village_and_somewhere_on_the_map() {
    let mut t = Tuning::default();
    t.nights.spawn_distance = t.village.build_range + 1.0;
    assert!(t.validate().is_err(), "inside the village range");

    let mut t = Tuning::default();
    t.nights.spawn_distance = 500.0;
    assert!(t.validate().is_err(), "off the map in every direction");

    let mut t = Tuning::default();
    t.nights.spawn_lanes = 0;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.nights.max_alive = 0;
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

/// 두께 1 타일의 벽 바로 앞에서 겹친 몬스터 둘을 한 Tick 진행하고, 벽에 닿은 쪽 몬스터의 y를 돌려줍니다.
fn y_after_a_push_against_a_thin_wall(separation_speed: f32) -> f32 {
    let mut s = Scenario::with_tuning(|t| {
        t.map.obstacles = vec![Rect::new(6.0, 8.0, 14.0, 9.0)];
        t.monsters.grunt.separation_speed = separation_speed;
    });
    let pushed = s.monster_at(10.0, 7.6);
    let _behind = s.monster_at(10.0, 7.59);
    s.run_ticks(1);
    s.monster(pushed).expect("the monster is alive").pos.y
}

#[test]
fn a_separation_push_that_could_carry_a_monster_across_a_wall_is_rejected() {
    let mut t = Tuning::default();
    t.map.obstacles = vec![Rect::new(6.0, 8.0, 14.0, 9.0)];
    t.monsters.grunt.separation_speed = 20.0;
    assert!(
        t.monsters.grunt.speed * t.dt() < t.monsters.grunt.radius,
        "walking alone is within the limit"
    );
    assert!(t.validate().is_err());
}

#[test]
fn walking_plus_the_largest_separation_push_stays_shorter_than_a_body_radius() {
    let grunt = Tuning::default().monsters.grunt;
    let dt = Tuning::default().dt();
    let largest_push = grunt.radius / dt - grunt.speed;

    let mut within = Tuning::default();
    within.monsters.grunt.separation_speed = largest_push * 0.95;
    assert_eq!(within.validate(), Ok(()));
    let y = y_after_a_push_against_a_thin_wall(largest_push * 0.95);
    assert!(y < 8.0, "the monster stayed in front of the wall, at y {y}");

    let mut beyond = Tuning::default();
    beyond.monsters.grunt.separation_speed = largest_push * 1.05;
    assert!(beyond.validate().is_err());
}

/// 수치표에서 속도 하나를 고르는 함수입니다.
type SpeedField = fn(&mut Tuning) -> &mut f32;

#[test]
fn a_speed_that_is_negative_or_not_finite_is_rejected_and_the_reason_names_it() {
    let speeds: [(&str, SpeedField); 4] = [
        ("player speed", |t| &mut t.player.speed),
        ("player crawl_speed", |t| &mut t.player.crawl_speed),
        ("Grunt speed", |t| &mut t.monsters.grunt.speed),
        ("Grunt separation_speed", |t| {
            &mut t.monsters.grunt.separation_speed
        }),
    ];
    for (name, field) in speeds {
        // 음수 걷기 속도는 몬스터를 목표의 반대쪽으로 걷게 하고, 음수 밀어내기 속도는 절댓값으로 검사하면 통과했습니다.
        for bad in [-0.1, -40.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut t = Tuning::default();
            *field(&mut t) = bad;
            let why = t.validate().expect_err(&format!("{name} of {bad}"));
            assert!(why.contains(name), "{name} of {bad}: {why}");
        }
        let mut t = Tuning::default();
        *field(&mut t) = 0.0;
        assert_eq!(t.validate(), Ok(()), "{name} of zero is a valid standstill");
    }
}
