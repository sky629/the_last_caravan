//! 기본 수치가 규칙의 전제를 지키는지 확인합니다. 수치를 고치다가 전제가 깨지면 이 테스트가 실패합니다.

use super::support::{Scenario, nights};
use crate::math::{Rect, Vec2};
use crate::tuning::Tuning;
use crate::world::Tile;

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
fn a_gather_node_must_be_on_a_free_tile_of_the_map_outside_the_village() {
    let rejected = |nodes: Vec<Tile>| {
        let mut t = Tuning::default();
        t.map.nodes = nodes;
        t.validate().expect_err("the node should be refused")
    };

    assert!(rejected(vec![Tile::new(64, 10)]).contains("outside the map"));
    assert!(
        rejected(vec![Tile::new(25, 24)]).contains("overlaps a solid"),
        "on an obstacle"
    );
    assert!(
        rejected(vec![Tile::new(31, 31)]).contains("overlaps a solid"),
        "on the wagon"
    );
    assert!(
        rejected(vec![Tile::new(9, 22), Tile::new(9, 22)]).contains("another node"),
        "twice on one tile"
    );
    assert!(
        rejected(vec![Tile::new(36, 28)]).contains("inside the village"),
        "within the build range of the wagon"
    );

    let mut t = Tuning::default();
    t.map.nodes = Vec::new();
    assert_eq!(t.validate(), Ok(()), "a map without nodes is fine");
}

#[test]
fn a_node_with_too_little_room_for_its_guards_is_rejected() {
    let mut t = Tuning::default();
    // 지도의 구석 (0, 0)에는 이웃한 빈 타일이 셋뿐입니다. 하지만 마을 밖이어야 하므로 마을의 범위를 줄입니다.
    t.village.build_range = 2.0;
    t.nights.spawn_distance = 18.0;
    t.map.nodes = vec![Tile::new(0, 0)];
    t.guards.per_node = 3;
    assert_eq!(t.validate(), Ok(()), "three neighbours, three guards");

    t.guards.per_node = 4;
    let why = t.validate().expect_err("a fourth guard has no tile");
    assert!(why.contains("room for only 3 of the 4 guards"), "{why}");

    t.guards.per_node = 0;
    assert_eq!(t.validate(), Ok(()), "no guards need no room");
}

#[test]
fn two_nodes_do_not_share_a_guard_tile() {
    let mut t = Tuning::default();
    t.village.build_range = 2.0;
    t.nights.spawn_distance = 18.0;
    // 지점 (0, 0)은 이웃한 빈 타일 (1, 0), (0, 1), (1, 1)을 모두 쓰고, 지점 (2, 0)은 오른쪽이 막혀서 이웃이 (1, 0), (2, 1), (1, 1)뿐입니다.
    t.map.nodes = vec![Tile::new(0, 0), Tile::new(2, 0)];
    t.map.obstacles = vec![Rect::new(3.0, 0.0, 4.0, 2.0)];
    t.guards.per_node = 2;
    assert_eq!(
        t.validate(),
        Ok(()),
        "(2, 1) and the shared (1, 0) is enough"
    );

    t.guards.per_node = 3;
    let why = t.validate().expect_err("the shared tiles are taken");
    assert!(why.contains("room for only 1 of the 3 guards"), "{why}");
}

#[test]
fn a_negative_chase_range_is_rejected() {
    let mut t = Tuning::default();
    t.guards.chase_range = -1.0;
    assert!(t.validate().is_err());
}

#[test]
fn resource_numbers_that_make_gathering_meaningless_are_rejected() {
    let mut t = Tuning::default();
    t.resources.carry_capacity = 0;
    assert!(t.validate().is_err(), "nothing could be carried");

    let mut t = Tuning::default();
    t.resources.gather_per_pulse = 0;
    assert!(t.validate().is_err(), "a pulse that moves nothing");

    let mut t = Tuning::default();
    t.resources.node_amount = 0;
    assert!(t.validate().is_err(), "nodes that start empty");

    let mut t = Tuning::default();
    t.resources.gather_reach = f32::NAN;
    assert!(t.validate().is_err());
}

#[test]
fn a_healing_pulse_needs_a_length_and_a_reach() {
    let mut t = Tuning::default();
    t.wagon.recovery.pulse_secs = 0.0;
    assert!(t.validate().is_err(), "a recovery that never happens");

    let mut t = Tuning::default();
    t.wagon.repair.pulse_secs = f32::NAN;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.structures.watchtower.repair.pulse_secs = -1.0;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.wagon.recovery.range = -1.0;
    assert!(t.validate().is_err());
}

#[test]
fn start_towers_must_be_free_tiles_of_the_village_within_the_limit() {
    let valid = |sites: Vec<Tile>| {
        let mut t = Tuning::default();
        t.village.start_towers = sites;
        t.validate()
    };
    let near_the_wagon = Tile::new(27, 29);
    assert_eq!(valid(vec![near_the_wagon]), Ok(()));
    assert!(
        valid(vec![near_the_wagon, near_the_wagon]).is_err(),
        "twice"
    );
    assert!(valid(vec![Tile::new(31, 31)]).is_err(), "on the wagon");
    assert!(valid(vec![Tile::new(24, 24)]).is_err(), "on an obstacle");
    assert!(
        valid(vec![Tile::new(60, 60)]).is_err(),
        "outside the village"
    );
    assert!(valid(vec![Tile::new(64, 31)]).is_err(), "outside the map");

    let mut t = Tuning::default();
    t.village.tower_limit = 1;
    t.village.start_towers = vec![Tile::new(27, 29), Tile::new(25, 29)];
    assert!(t.validate().is_err(), "more than the limit");
}

#[test]
fn start_towers_that_wall_in_a_free_tile_are_rejected() {
    // 타일 (26, 29)의 네 이웃을 모두 막으면 그 타일은 마차에서 걸어갈 수 없습니다.
    let mut t = Tuning::default();
    t.village.start_towers = vec![
        Tile::new(25, 29),
        Tile::new(27, 29),
        Tile::new(26, 28),
        Tile::new(26, 30),
    ];
    let err = t.validate().expect_err("one tile is cut off");
    assert!(err.contains("connected to the wagon"), "{err}");
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
