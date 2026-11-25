//! 기본 수치가 규칙의 전제를 지키는지 확인합니다. 수치를 고치다가 전제가 깨지면 이 테스트가 실패합니다.

use super::support::{Scenario, nights};
use crate::kind::WeaponKind;
use crate::math::{Rect, Vec2};
use crate::tuning::{ArcTuning, Delivery, PLAYTEST_DAMAGE_MULTIPLIER, ProjectileTuning, Tuning};
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
    t.wagon.repair.heal.pulse_secs = f32::NAN;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.structures.watchtower.repair.heal.pulse_secs = -1.0;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.wagon.recovery.range = -1.0;
    assert!(t.validate().is_err());
}

#[test]
fn a_repair_that_costs_nothing_is_rejected_and_the_reason_names_it() {
    let mut t = Tuning::default();
    t.wagon.repair.cost_per_pulse = 0;
    let why = t.validate().expect_err("a free wagon repair");
    assert!(why.contains("wagon repair cost_per_pulse"), "{why}");

    let mut t = Tuning::default();
    t.structures.watchtower.repair.cost_per_pulse = 0;
    let why = t.validate().expect_err("a free tower repair");
    assert!(why.contains("Watchtower repair cost_per_pulse"), "{why}");
}

#[test]
fn the_default_repairs_heal_20_hp_a_second_and_a_full_repair_costs_10_for_a_tower_and_17_for_the_wagon()
 {
    let t = Tuning::default();
    let rate = |heal: &crate::tuning::HealTuning| heal.hp_per_pulse as f32 / heal.pulse_secs;
    assert_eq!(rate(&t.wagon.repair.heal), 20.0);
    assert_eq!(rate(&t.structures.watchtower.repair.heal), 20.0);

    let pulses = |max_hp: u32, heal: &crate::tuning::HealTuning| max_hp.div_ceil(heal.hp_per_pulse);
    let tower = t.structures.watchtower.clone();
    assert_eq!(
        pulses(tower.max_hp, &tower.repair.heal) * tower.repair.cost_per_pulse,
        tower.build_cost,
        "a full tower repair costs what building the tower costs"
    );
    assert_eq!(
        pulses(t.wagon.max_hp, &t.wagon.repair.heal) * t.wagon.repair.cost_per_pulse,
        17
    );
}

#[test]
fn a_sprint_slower_than_walking_or_not_a_number_is_rejected() {
    for bad in [0.99, 0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.sprint.speed_multiplier = bad;
        let why = t.validate().expect_err(&format!("a multiplier of {bad}"));
        assert!(why.contains("sprint speed_multiplier"), "{bad}: {why}");
    }
    let mut t = Tuning::default();
    t.sprint.speed_multiplier = 1.0;
    assert_eq!(
        t.validate(),
        Ok(()),
        "a sprint as fast as walking is allowed"
    );
}

#[test]
fn a_sprint_gauge_without_a_length_is_rejected() {
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.sprint.sprint_secs = bad;
        let why = t.validate().expect_err(&format!("sprint_secs of {bad}"));
        assert!(why.contains("sprint_secs"), "{bad}: {why}");

        let mut t = Tuning::default();
        t.sprint.refill_secs = bad;
        let why = t.validate().expect_err(&format!("refill_secs of {bad}"));
        assert!(why.contains("refill_secs"), "{bad}: {why}");
    }
}

#[test]
fn a_gauge_too_big_to_count_in_whole_numbers_is_rejected() {
    let mut t = Tuning::default();
    t.sprint.sprint_secs = 100_000.0;
    t.sprint.refill_secs = 100_000.0;
    let why = t.validate().expect_err("the gauge does not fit in 32 bits");
    assert!(why.contains("sprint gauge"), "{why}");
}

#[test]
fn a_sprint_that_moves_a_player_a_body_radius_or_more_in_one_tick_is_rejected() {
    // 걷는 속도 4에 배율 1.75를 곱한 속도 7은 한 Tick에 0.35타일이고, 이것이 몸의 반지름입니다.
    let mut t = Tuning::default();
    t.sprint.speed_multiplier = 1.74;
    assert_eq!(t.validate(), Ok(()), "just under the radius");

    t.sprint.speed_multiplier = 1.76;
    let why = t.validate().expect_err("longer than the radius");
    assert!(why.contains("a player moves"), "{why}");

    t.sprint.speed_multiplier = 3.0;
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

#[test]
fn a_weapon_with_a_bad_cooldown_or_arc_is_rejected_and_the_reason_names_it() {
    for bad in [-0.1, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.weapons.sword.cooldown_secs = bad;
        let why = t.validate().expect_err(&format!("a cooldown of {bad}"));
        assert!(why.contains("Sword cooldown_secs"), "{bad}: {why}");

        let mut t = Tuning::default();
        t.weapons.sword.delivery = Delivery::Arc(ArcTuning {
            reach: bad,
            half_angle_deg: 60.0,
        });
        let why = t.validate().expect_err(&format!("a reach of {bad}"));
        assert!(why.contains("Sword reach"), "{bad}: {why}");
    }
    for bad in [-1.0, 180.5, f32::NAN] {
        let mut t = Tuning::default();
        t.weapons.sword.delivery = Delivery::Arc(ArcTuning {
            reach: 1.0,
            half_angle_deg: bad,
        });
        let why = t.validate().expect_err(&format!("an angle of {bad}"));
        assert!(why.contains("Sword half_angle_deg"), "{bad}: {why}");
    }
}

#[test]
fn an_arc_from_a_line_to_a_full_circle_is_valid() {
    for angle in [0.0, 180.0] {
        let mut t = Tuning::default();
        t.weapons.sword.delivery = Delivery::Arc(ArcTuning {
            reach: 0.0,
            half_angle_deg: angle,
        });
        assert_eq!(t.validate(), Ok(()), "an angle of {angle}");
    }
}

fn with_bow(shot: ProjectileTuning) -> Tuning {
    let mut t = Tuning::default();
    t.weapons.bow.delivery = Delivery::Projectile(shot);
    t
}

fn default_shot() -> ProjectileTuning {
    let Delivery::Projectile(shot) = Tuning::default().weapons.bow.delivery else {
        panic!("the bow shoots projectiles");
    };
    shot
}

#[test]
fn a_projectile_that_never_moves_or_never_ends_is_rejected() {
    for bad in [0.0, -14.0, f32::NAN, f32::INFINITY] {
        let t = with_bow(ProjectileTuning {
            speed: bad,
            ..default_shot()
        });
        let why = t.validate().expect_err(&format!("a speed of {bad}"));
        assert!(why.contains("Bow projectile speed"), "{bad}: {why}");

        let t = with_bow(ProjectileTuning {
            range: bad,
            ..default_shot()
        });
        let why = t.validate().expect_err(&format!("a range of {bad}"));
        assert!(why.contains("Bow projectile range"), "{bad}: {why}");
    }
}

#[test]
fn a_projectile_as_wide_as_the_player_is_rejected() {
    let player_radius = Tuning::default().player.radius;
    // 0.75배는 플레이어보다 가늘지만, 장애물의 모서리에 붙어 서서 쏘면 쏜 자리에서 멈추는 굵기입니다.
    for bad in [player_radius, player_radius * 0.75, 1.0, -0.1, f32::NAN] {
        let t = with_bow(ProjectileTuning {
            radius: bad,
            ..default_shot()
        });
        let why = t.validate().expect_err(&format!("a radius of {bad}"));
        assert!(why.contains("Bow projectile radius"), "{bad}: {why}");
    }
    let t = with_bow(ProjectileTuning {
        radius: 0.0,
        ..default_shot()
    });
    assert_eq!(t.validate(), Ok(()), "a point of a projectile is valid");
}

#[test]
fn a_projectile_fast_enough_to_cross_the_map_in_one_tick_is_still_valid() {
    // 명중 판정이 이동하는 선분 전체를 검사하므로 속도에 상한이 없습니다. 이동과 달리 한 Tick의 거리가 반지름보다 길어도 됩니다.
    let t = with_bow(ProjectileTuning {
        speed: 5000.0,
        ..default_shot()
    });
    assert!(5000.0 * t.dt() > t.map.width);
    assert_eq!(t.validate(), Ok(()));
}

#[test]
fn the_playtest_multiplier_raises_every_weapon_by_the_same_ratio() {
    let t = Tuning::default();
    let sword = t.weapons.get(WeaponKind::Sword).damage;
    let bow = t.weapons.get(WeaponKind::Bow).damage;

    assert_eq!(
        sword,
        25 * PLAYTEST_DAMAGE_MULTIPLIER,
        "the normal sword damage is 25"
    );
    assert_eq!(
        bow,
        15 * PLAYTEST_DAMAGE_MULTIPLIER,
        "the normal bow damage is 15"
    );
    assert_eq!(
        sword * 15,
        bow * 25,
        "the bow is 60% of the sword at any multiplier"
    );
    assert_eq!(
        t.weapons.get(WeaponKind::Greatsword).damage,
        45 * PLAYTEST_DAMAGE_MULTIPLIER,
        "the normal greatsword damage is 45"
    );
    assert_eq!(
        t.weapons.get(WeaponKind::Spear).damage,
        30 * PLAYTEST_DAMAGE_MULTIPLIER,
        "the normal spear damage is 30"
    );
}

#[test]
fn the_default_greatsword_and_spear_are_the_numbers_of_the_weapon_table() {
    let t = Tuning::default();
    let arc = |kind| match t.weapons.get(kind).delivery {
        Delivery::Arc(arc) => (arc.reach, arc.half_angle_deg),
        Delivery::Projectile(_) => panic!("{kind:?} swings an arc"),
    };

    assert_eq!(arc(WeaponKind::Greatsword), (1.3, 90.0));
    assert_eq!(t.weapons.greatsword.cooldown_secs, 1.0);
    assert_eq!(arc(WeaponKind::Spear), (2.2, 15.0));
    assert_eq!(t.weapons.spear.cooldown_secs, 0.6);
}

#[test]
fn a_chest_weight_above_the_limit_is_rejected_and_the_reason_names_the_weapon() {
    let mut t = Tuning::default();
    t.weapons.spear.chest_weight = 1000;
    assert_eq!(t.validate(), Ok(()), "the limit itself is valid");

    t.weapons.spear.chest_weight = 1001;
    let why = t.validate().expect_err("a weight above the limit");
    assert!(why.contains("Spear chest_weight"), "{why}");
}

#[test]
#[ignore = "the playtest damage is on; run `cargo test -p caravan_rules -- --ignored` before a release"]
fn before_a_release_the_damage_is_the_normal_damage() {
    assert_eq!(
        PLAYTEST_DAMAGE_MULTIPLIER, 1,
        "lower PLAYTEST_DAMAGE_MULTIPLIER to 1"
    );
    let t = Tuning::default();
    assert_eq!(t.weapons.get(WeaponKind::Sword).damage, 25);
    assert_eq!(t.weapons.get(WeaponKind::Bow).damage, 15);
    assert_eq!(t.weapons.get(WeaponKind::Greatsword).damage, 45);
    assert_eq!(t.weapons.get(WeaponKind::Spear).damage, 30);
}
