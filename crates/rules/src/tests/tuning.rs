//! 기본 수치가 규칙의 전제를 지키는지 확인합니다. 수치를 고치다가 전제가 깨지면 이 테스트가 실패합니다.

use super::support::{Scenario, nights, watchtower_shot};
use crate::kind::WeaponKind;
use crate::math::{Rect, Vec2};
use crate::tuning::{
    AfflictTuning, ArcTuning, AuraTuning, Buff, Debuff, Delivery, HitscanTuning, ProjectileTuning,
    PulseTuning, TowerAction, Tuning, WaveScaling,
};
use crate::world::Tile;

#[test]
fn the_default_tuning_is_valid() {
    assert_eq!(Tuning::default().validate(), Ok(()));
}

#[test]
fn one_tick_of_a_walking_monster_is_shorter_than_a_body_radius_so_it_does_not_tunnel() {
    let t = Tuning::default();
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
fn the_table_of_waves_is_for_a_party_of_four_and_a_party_smaller_than_a_full_room_is_rejected() {
    let mut t = Tuning::default();
    assert_eq!(t.nights.scaling, WaveScaling::ByParty { full_party: 4 });
    assert_eq!(t.max_players, 4, "a full room plays the table as it is");

    for too_small in [0, 1, 3] {
        t.nights.scaling = WaveScaling::ByParty {
            full_party: too_small,
        };
        assert!(
            t.validate().unwrap_err().contains("full_party"),
            "{too_small} is less than a full room, which would get more than the table"
        );
    }

    t.nights.scaling = WaveScaling::ByParty { full_party: 5 };
    assert_eq!(t.validate(), Ok(()));
    t.nights.scaling = WaveScaling::Fixed;
    assert_eq!(t.validate(), Ok(()));
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
fn the_camp_spots_must_be_inside_the_map_clear_of_obstacles_and_the_wagon() {
    let rejected_scarecrow = |spot: Vec2| {
        let mut t = Tuning::default();
        t.map.camp.scarecrows = vec![spot];
        t.validate().expect_err("the scarecrow should be refused")
    };
    assert!(rejected_scarecrow(Vec2::new(32.0, 32.0)).contains("camp scarecrow"));
    assert!(
        rejected_scarecrow(Vec2::new(26.0, 24.5)).contains("camp scarecrow"),
        "on an obstacle"
    );
    assert!(
        rejected_scarecrow(Vec2::new(63.9, 10.0)).contains("camp scarecrow"),
        "the body would stick out of the map"
    );
    assert!(
        rejected_scarecrow(Vec2::new(70.0, 10.0)).contains("camp scarecrow"),
        "off the map"
    );

    let rejected_rack = |first_rack: Vec2| {
        let mut t = Tuning::default();
        t.map.camp.first_rack = first_rack;
        t.validate().expect_err("the rack should be refused")
    };
    assert!(rejected_rack(Vec2::new(32.0, 32.0)).contains("camp weapon rack"));
    assert!(
        rejected_rack(Vec2::new(26.0, 24.5)).contains("camp weapon rack"),
        "on an obstacle"
    );

    let mut t = Tuning::default();
    t.map.camp.scarecrows = Vec::new();
    assert_eq!(t.validate(), Ok(()), "a camp without scarecrows is fine");
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

/// 기본 맵에서 상자의 후보 자리를 `sites`로 바꾼 수치를 검사해서, 거절한 이유를 돌려줍니다.
fn chest_sites_rejected(sites: Vec<Tile>) -> String {
    let mut t = Tuning::default();
    t.map.chest_sites = sites;
    t.loot.chests_per_day = 1;
    t.validate().expect_err("the chest sites should be refused")
}

#[test]
fn a_chest_site_must_be_a_free_tile_of_the_map_outside_the_village() {
    assert!(
        chest_sites_rejected(vec![Tile::new(64, 10)]).contains("must be a free tile"),
        "outside the map"
    );
    assert!(
        chest_sites_rejected(vec![Tile::new(25, 24)]).contains("must be a free tile"),
        "on an obstacle"
    );
    assert!(
        chest_sites_rejected(vec![Tile::new(31, 31)]).contains("must be a free tile"),
        "on the wagon"
    );
    assert!(
        chest_sites_rejected(vec![Tile::new(54, 41)]).contains("must be a free tile"),
        "on a gather node"
    );
    assert!(
        chest_sites_rejected(vec![Tile::new(47, 32), Tile::new(47, 32)])
            .contains("must be a free tile"),
        "twice on one tile"
    );
    assert!(
        chest_sites_rejected(vec![Tile::new(36, 28)]).contains("inside the village"),
        "within the build range of the wagon"
    );
}

#[test]
fn a_chest_site_that_would_cut_the_map_off_when_it_is_blocked_is_rejected() {
    let mut t = Tuning::default();
    // 지도를 가로지르는 벽에 폭 1타일의 틈 (63, 5)가 하나 있습니다. 그 틈이 후보 자리이면 상자가 놓이는 날 벽 너머가 끊깁니다.
    t.map.obstacles = vec![Rect::new(0.0, 5.0, 63.0, 6.0)];
    t.map.chest_sites = vec![Tile::new(63, 5)];
    t.loot.chests_per_day = 1;

    let why = t.validate().expect_err("the only gap is a chest site");

    assert!(why.contains("stay connected"), "{why}");
    t.map.chest_sites = vec![Tile::new(62, 10)];
    assert_eq!(t.validate(), Ok(()), "a site beside the gap is fine");
}

#[test]
fn a_chest_site_with_no_free_side_tile_to_open_it_from_is_rejected() {
    let mut t = Tuning::default();
    t.village.build_range = 2.0;
    t.nights.spawn_distance = 18.0;
    t.guards.per_chest = 0;
    // 후보 자리 (0, 0)의 상하좌우 이웃 (1, 0)과 (0, 1)이 모두 다른 후보 자리이면 서서 열 타일이 없습니다.
    t.map.chest_sites = vec![Tile::new(0, 0), Tile::new(1, 0), Tile::new(0, 1)];
    t.loot.chests_per_day = 1;

    let why = t.validate().expect_err("no tile to stand on");

    assert!(why.contains("free side tile"), "{why}");
    assert!(why.contains("Tile { x: 0, y: 0 }"), "{why}");
    t.map.chest_sites = vec![Tile::new(0, 0), Tile::new(1, 0)];
    assert_eq!(t.validate(), Ok(()), "(0, 1) is a tile to stand on");
}

#[test]
fn a_chest_site_with_too_little_room_for_its_guards_is_rejected() {
    let mut t = Tuning::default();
    t.village.build_range = 2.0;
    t.nights.spawn_distance = 18.0;
    t.map.chest_sites = vec![Tile::new(0, 0)];
    t.loot.chests_per_day = 1;
    t.guards.per_chest = 3;
    assert_eq!(t.validate(), Ok(()), "three neighbours, three guards");

    t.guards.per_chest = 4;
    let why = t.validate().expect_err("a fourth guard has no tile");

    assert!(why.contains("chest site"), "{why}");
    assert!(why.contains("room for only 3 of the 4 guards"), "{why}");
}

#[test]
fn a_guard_tile_is_never_a_chest_site_or_shared_between_a_node_and_a_chest() {
    let mut t = Tuning::default();
    t.village.build_range = 2.0;
    t.nights.spawn_distance = 18.0;
    // 후보 자리 (0, 0)의 이웃은 (1, 0), (0, 1), (1, 1)이고, 후보 자리 (1, 0)이 하나를 차지합니다. 남은 이웃은 둘입니다.
    t.map.nodes = Vec::new();
    t.map.chest_sites = vec![Tile::new(0, 0), Tile::new(1, 0)];
    t.loot.chests_per_day = 1;
    t.guards.per_chest = 2;
    assert_eq!(t.validate(), Ok(()), "(0, 1) and (1, 1) are enough");

    t.guards.per_chest = 3;
    let why = t
        .validate()
        .expect_err("the other site is not a guard tile");
    assert!(why.contains("room for only 2 of the 3 guards"), "{why}");
}

#[test]
fn more_chests_a_day_than_sites_or_chests_that_can_hold_nothing_are_rejected() {
    let mut t = Tuning::default();
    t.loot.chests_per_day = 9;
    let why = t
        .validate()
        .expect_err("eight sites cannot hold nine chests");
    assert!(why.contains("chests per day"), "{why}");

    let mut t = Tuning::default();
    t.weapons.bow.chest_weight = 0;
    t.weapons.greatsword.chest_weight = 0;
    t.weapons.spear.chest_weight = 0;
    t.weapons.crossbow.chest_weight = 0;
    t.weapons.rifle.chest_weight = 0;
    let why = t.validate().expect_err("no weapon can come out of a chest");
    assert!(why.contains("chest_weight"), "{why}");
    t.loot.chests_per_day = 0;
    assert_eq!(
        t.validate(),
        Ok(()),
        "without chests the weights do not matter"
    );
}

#[test]
fn a_take_reach_that_leaves_the_farthest_opener_short_of_the_weapon_is_rejected() {
    let mut t = Tuning::default();
    // 상자를 열 수 있는 가장 먼 자리에서 타일의 중심까지의 거리는 1.0 + 0.7071입니다.
    t.loot.take_reach = 1.7;
    let why = t
        .validate()
        .expect_err("the weapon on the chest tile is out of reach");
    assert!(why.contains("take_reach 1.7"), "{why}");
    assert!(why.contains("without stepping"), "{why}");

    t.loot.take_reach = 1.71;
    assert_eq!(t.validate(), Ok(()), "1.71 covers 1.7071");

    t.loot.open_reach = 1.5;
    assert!(
        t.validate().is_err(),
        "a longer open reach needs a longer take reach"
    );
    t.loot.chests_per_day = 0;
    assert_eq!(
        t.validate(),
        Ok(()),
        "without chests nobody opens from afar"
    );
}

#[test]
fn a_map_without_chest_sites_is_valid_when_no_chest_is_placed() {
    let mut t = Tuning::default();
    t.map.chest_sites = Vec::new();
    t.loot.chests_per_day = 0;
    assert_eq!(t.validate(), Ok(()));

    t.loot.chests_per_day = 1;
    assert!(t.validate().is_err(), "one chest needs one site");
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
fn a_sprint_that_moves_a_player_a_body_radius_or_more_in_one_tick_is_valid() {
    // 걷는 속도 4에 배율 1.75를 곱한 속도 7은 한 Tick에 0.35타일이고, 이것이 몸의 반지름입니다.
    // 플레이어의 이동은 반지름보다 짧은 걸음으로 나뉘므로 그보다 빨라도 수치가 맞습니다.
    for multiplier in [1.74, 1.76, 3.0, 20.0] {
        let mut t = Tuning::default();
        t.sprint.speed_multiplier = multiplier;
        assert_eq!(t.validate(), Ok(()), "a sprint multiplier of {multiplier}");
    }
}

#[test]
fn a_player_who_moves_64_body_radii_in_one_tick_is_rejected_and_the_reason_names_it() {
    // 걸음의 수는 한 Tick의 거리를 반지름으로 나눈 값이므로, 이 비율에 상한이 없으면 Tick 하나가 끝나지 않거나 걸음의 수가 넘칩니다.
    let base = Tuning::default();
    let boosts = base.sprint.speed_multiplier * base.effects.swift_speed_factor;
    let limit = 64.0 * base.player.radius / base.dt();
    let mut just_below = Tuning::default();
    just_below.player.speed = limit / boosts * 0.99;
    assert_eq!(just_below.validate(), Ok(()));

    let fields: [(&str, SpeedField); 2] = [
        ("speed", |t| &mut t.player.speed),
        ("crawl_speed", |t| &mut t.player.crawl_speed),
    ];
    for (name, field) in fields {
        for bad in [limit * 1.01, 7e6, 1e30, f32::MAX] {
            let mut t = Tuning::default();
            *field(&mut t) = bad;
            let why = t.validate().expect_err(&format!("{name} of {bad}"));
            assert!(why.contains("player speed"), "{name} of {bad}: {why}");
        }
    }
    let mut boosted = Tuning::default();
    boosted.player.speed = limit / boosts * 1.01;
    assert!(boosted.validate().is_err(), "the sprint and the buff count");

    // 탄환의 반지름이 0이면 플레이어의 반지름에 다른 하한이 없으므로, 아주 작은 반지름은 이 검사가 거절합니다.
    for radius in [1e-5, 1e-30] {
        let mut tiny = Tuning::default();
        tiny.player.radius = radius;
        for weapon in [&mut tiny.weapons.bow, &mut tiny.weapons.crossbow] {
            if let Delivery::Projectile(shot) = &mut weapon.delivery {
                shot.radius = 0.0;
            }
        }
        let why = tiny.validate().expect_err(&format!("a radius of {radius}"));
        assert!(why.contains("player speed"), "a radius of {radius}: {why}");
    }
}

#[test]
fn a_player_without_a_positive_radius_is_rejected_and_the_reason_names_it() {
    // 반지름이 걸음의 수를 정하므로, 0은 나눗셈을 깨고 음수는 걸음을 나누지 않게 합니다.
    for bad in [0.0, -0.35, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.player.radius = bad;
        let why = t.validate().expect_err(&format!("a radius of {bad}"));
        assert!(why.contains("player radius"), "{bad}: {why}");
    }
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
fn a_projectile_that_can_hit_no_monster_is_rejected() {
    let t = with_bow(ProjectileTuning {
        pierce: 0,
        ..default_shot()
    });
    let why = t.validate().expect_err("a pierce of 0");
    assert!(why.contains("Bow projectile pierce"), "{why}");

    let t = with_bow(ProjectileTuning {
        pierce: 1,
        ..default_shot()
    });
    assert_eq!(t.validate(), Ok(()), "one monster is the least");
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
    // 명중 판정이 이동하는 선분 전체를 검사하므로 속도에 상한이 없습니다. 몸의 이동과 달리 한 번에 움직이는 거리가 반지름보다 길어도 됩니다.
    let t = with_bow(ProjectileTuning {
        speed: 5000.0,
        ..default_shot()
    });
    assert!(5000.0 * t.dt() > t.map.width);
    assert_eq!(t.validate(), Ok(()));
}

#[test]
fn the_default_weapons_deal_their_normal_damage() {
    let t = Tuning::default();
    for (kind, damage) in [
        (WeaponKind::Sword, 25),
        (WeaponKind::Bow, 15),
        (WeaponKind::Greatsword, 45),
        (WeaponKind::Spear, 30),
        (WeaponKind::Crossbow, 20),
        (WeaponKind::Rifle, 60),
    ] {
        assert_eq!(t.weapons.get(kind).damage, damage, "{kind:?}");
    }
}

#[test]
fn the_default_greatsword_and_spear_are_the_numbers_of_the_weapon_table() {
    let t = Tuning::default();
    let arc = |kind| match t.weapons.get(kind).delivery {
        Delivery::Arc(arc) => (arc.reach, arc.half_angle_deg),
        Delivery::Projectile(_) | Delivery::Hitscan(_) => panic!("{kind:?} swings an arc"),
    };

    assert_eq!(arc(WeaponKind::Greatsword), (1.3, 90.0));
    assert_eq!(t.weapons.greatsword.cooldown_secs, 1.0);
    assert_eq!(arc(WeaponKind::Spear), (2.2, 15.0));
    assert_eq!(t.weapons.spear.cooldown_secs, 0.6);
}

#[test]
fn the_default_crossbow_is_the_numbers_of_the_weapon_table() {
    let t = Tuning::default();
    let crossbow = t.weapons.get(WeaponKind::Crossbow);
    let Delivery::Projectile(shot) = crossbow.delivery else {
        panic!("the crossbow shoots projectiles");
    };

    assert_eq!(crossbow.cooldown_secs, 0.9);
    assert_eq!(
        (shot.speed, shot.range, shot.radius, shot.pierce),
        (18.0, 11.0, 0.15, 4)
    );
    assert!(crossbow.chest_weight > 0, "it comes out of chests");
}

#[test]
fn the_default_rifle_is_the_numbers_of_the_weapon_table() {
    let t = Tuning::default();
    let rifle = t.weapons.get(WeaponKind::Rifle);
    let Delivery::Hitscan(shot) = rifle.delivery else {
        panic!("the rifle hits along a line");
    };

    assert_eq!(rifle.cooldown_secs, 1.5);
    assert_eq!(shot.range, 12.0);
    assert!(rifle.chest_weight > 0, "it comes out of chests");
}

#[test]
fn a_hitscan_weapon_with_no_range_is_rejected_and_the_reason_names_it() {
    for bad in [0.0, -12.0, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.weapons.rifle.delivery = Delivery::Hitscan(HitscanTuning { range: bad });
        let why = t.validate().expect_err(&format!("a range of {bad}"));
        assert!(why.contains("Rifle hitscan range"), "{bad}: {why}");
    }
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
fn a_slow_that_does_not_slow_is_rejected_and_the_reason_names_the_row() {
    for bad in [0.0, 1.5, -0.5, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.weapons.bow.on_hit = Some(Debuff::Slow {
            factor: bad,
            secs: 2.0,
        });
        let why = t.validate().expect_err(&format!("a factor of {bad}"));
        assert!(why.contains("Bow slow factor"), "{bad}: {why}");
    }
    let mut t = Tuning::default();
    t.weapons.bow.on_hit = Some(Debuff::Slow {
        factor: 1.0,
        secs: 2.0,
    });
    assert_eq!(
        t.validate(),
        Ok(()),
        "a slow of 100% changes nothing but is valid"
    );
}

#[test]
fn an_effect_that_lasts_no_time_is_rejected_and_the_reason_names_the_row() {
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.weapons.crossbow.on_hit = Some(Debuff::Slow {
            factor: 0.5,
            secs: bad,
        });
        let why = t.validate().expect_err(&format!("a slow of {bad} s"));
        assert!(why.contains("Crossbow slow secs"), "{bad}: {why}");

        let mut t = Tuning::default();
        t.weapons.greatsword.on_hit = Some(Debuff::Stun { secs: bad });
        let why = t.validate().expect_err(&format!("a stun of {bad} s"));
        assert!(why.contains("Greatsword stun secs"), "{bad}: {why}");

        let mut t = Tuning::default();
        t.weapons.spear.on_hit = Some(Debuff::Bleed {
            damage: 6,
            secs: bad,
        });
        let why = t.validate().expect_err(&format!("a bleed of {bad} s"));
        assert!(why.contains("Spear bleed secs"), "{bad}: {why}");
    }
}

#[test]
fn a_bleed_that_never_hurts_is_rejected_and_the_reason_names_the_row() {
    let mut t = Tuning::default();
    t.weapons.sword.on_hit = Some(Debuff::Bleed {
        damage: 0,
        secs: 3.0,
    });
    let why = t.validate().expect_err("a bleed of 0 damage");
    assert!(why.contains("Sword bleed damage"), "{why}");

    // 출혈이 간격(1초)보다 짧으면 첫 피해가 나기 전에 끝납니다.
    let mut t = Tuning::default();
    t.weapons.sword.on_hit = Some(Debuff::Bleed {
        damage: 5,
        secs: 0.9,
    });
    let why = t.validate().expect_err("a bleed shorter than its interval");
    assert!(why.contains("Sword bleed secs"), "{why}");

    t.weapons.sword.on_hit = Some(Debuff::Bleed {
        damage: 5,
        secs: 1.0,
    });
    assert_eq!(
        t.validate(),
        Ok(()),
        "a bleed as long as the interval hurts once"
    );
}

#[test]
fn the_numbers_all_effects_share_must_be_longer_than_no_time() {
    for bad in [0.0, -1.0, f32::NAN] {
        let mut t = Tuning::default();
        t.effects.stun_immunity_secs = bad;
        let why = t.validate().expect_err(&format!("an immunity of {bad} s"));
        assert!(why.contains("effects stun_immunity_secs"), "{bad}: {why}");

        let mut t = Tuning::default();
        t.effects.bleed_interval_secs = bad;
        let why = t.validate().expect_err(&format!("an interval of {bad} s"));
        assert!(why.contains("effects bleed_interval_secs"), "{bad}: {why}");
    }
}

#[test]
fn a_haste_that_does_not_shorten_the_cooldown_is_rejected_and_the_reason_names_it() {
    for bad in [0.0, 1.5, -0.7, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.effects.haste_cooldown_factor = bad;
        let why = t.validate().expect_err(&format!("a haste of {bad}"));
        assert!(
            why.contains("effects haste_cooldown_factor"),
            "{bad}: {why}"
        );
    }
    for valid in [0.01, 1.0] {
        let mut t = Tuning::default();
        t.effects.haste_cooldown_factor = valid;
        assert_eq!(t.validate(), Ok(()), "a haste of {valid}");
    }
}

#[test]
fn a_swift_that_does_not_speed_up_the_player_is_rejected_and_the_reason_names_it() {
    for bad in [0.99, 0.0, -1.3, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.effects.swift_speed_factor = bad;
        let why = t.validate().expect_err(&format!("a swift of {bad}"));
        assert!(why.contains("effects swift_speed_factor"), "{bad}: {why}");
    }
    let mut t = Tuning::default();
    t.effects.swift_speed_factor = 1.0;
    assert_eq!(t.validate(), Ok(()), "a swift of 100% changes nothing");
}

#[test]
fn the_row_of_a_structure_is_checked_like_the_row_of_a_weapon() {
    let mut t = Tuning::default();
    watchtower_shot(&mut t).on_hit = Some(Debuff::Stun { secs: 0.0 });
    let why = t.validate().expect_err("a stun of no time");
    assert!(why.contains("Watchtower stun secs"), "{why}");

    watchtower_shot(&mut t).on_hit = Some(Debuff::Stun { secs: 0.5 });
    assert_eq!(t.validate(), Ok(()));
}

#[test]
fn a_tower_needs_a_range_and_a_cost() {
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        t.structures.watchtower.range = bad;
        let why = t.validate().expect_err(&format!("a range of {bad}"));
        assert!(why.contains("Watchtower range"), "{bad}: {why}");
    }

    let mut t = Tuning::default();
    t.structures.watchtower.build_cost = 0;
    let why = t.validate().expect_err("a tower that costs nothing");
    assert!(why.contains("Watchtower build_cost"), "{why}");
}

#[test]
fn the_cooldown_of_a_shot_is_finite_and_not_negative() {
    for bad in [-0.1, f32::NAN, f32::INFINITY] {
        let mut t = Tuning::default();
        watchtower_shot(&mut t).cooldown_secs = bad;
        let why = t.validate().expect_err(&format!("a cooldown of {bad}"));
        assert!(why.contains("Watchtower cooldown_secs"), "{bad}: {why}");
    }
    let mut t = Tuning::default();
    watchtower_shot(&mut t).cooldown_secs = 0.0;
    assert_eq!(t.validate(), Ok(()), "a tower that shoots every tick");
}

#[test]
fn a_pulse_longer_than_its_effect_is_rejected_and_one_as_long_is_accepted() {
    let slow = |pulse_secs| {
        TowerAction::Pulse(PulseTuning {
            pulse_secs,
            debuff: Debuff::Slow {
                factor: 0.7,
                secs: 1.0,
            },
        })
    };
    let mut t = Tuning::default();
    t.structures.slow_tower.action = slow(1.05);
    let why = t.validate().expect_err("a pulse that lets the slow lapse");
    assert!(why.contains("SlowTower pulses every"), "{why}");

    t.structures.slow_tower.action = slow(1.0);
    assert_eq!(t.validate(), Ok(()), "a pulse as long as the effect");

    for bad in [0.0, -0.5, f32::NAN, f32::INFINITY] {
        t.structures.slow_tower.action = slow(bad);
        let why = t.validate().expect_err(&format!("a pulse of {bad}"));
        assert!(why.contains("SlowTower pulse_secs"), "{bad}: {why}");
    }
}

#[test]
fn a_tower_on_every_monster_cannot_bleed_or_stun() {
    let mut t = Tuning::default();
    for (debuff, said) in [
        (
            Debuff::Bleed {
                damage: 5,
                secs: 3.0,
            },
            "cannot apply a bleed",
        ),
        (Debuff::Stun { secs: 0.8 }, "cannot stun"),
    ] {
        t.structures.slow_tower.action = TowerAction::Pulse(PulseTuning {
            pulse_secs: 0.5,
            debuff,
        });
        let why = t.validate().expect_err(&format!("{debuff:?}"));
        assert!(why.contains("SlowTower") && why.contains(said), "{why}");
    }
}

#[test]
fn a_tower_on_one_monster_needs_a_cooldown_and_cannot_bleed() {
    let afflict = |cooldown_secs, debuff| {
        TowerAction::Afflict(AfflictTuning {
            cooldown_secs,
            debuff,
        })
    };
    let stun = Debuff::Stun { secs: 0.8 };
    let mut t = Tuning::default();
    for bad in [0.0, -2.0, f32::NAN, f32::INFINITY] {
        t.structures.stun_tower.action = afflict(bad, stun);
        let why = t.validate().expect_err(&format!("a cooldown of {bad}"));
        assert!(why.contains("StunTower cooldown_secs"), "{bad}: {why}");
    }

    t.structures.stun_tower.action = afflict(
        2.0,
        Debuff::Bleed {
            damage: 5,
            secs: 3.0,
        },
    );
    let why = t.validate().expect_err("a tower that bleeds");
    assert!(
        why.contains("StunTower") && why.contains("cannot apply a bleed"),
        "{why}"
    );

    t.structures.stun_tower.action = afflict(
        2.0,
        Debuff::Slow {
            factor: 0.5,
            secs: 2.0,
        },
    );
    assert_eq!(t.validate(), Ok(()), "one monster at a time may be slowed");
}

#[test]
fn the_buff_of_a_buff_tower_lasts_longer_than_zero() {
    let aura = |buff| TowerAction::Aura(AuraTuning { buff });
    let mut t = Tuning::default();
    for bad in [0.0, -2.0, f32::NAN, f32::INFINITY] {
        t.structures.haste_tower.action = aura(Buff::Haste { secs: bad });
        let why = t.validate().expect_err(&format!("a haste of {bad}"));
        assert!(why.contains("HasteTower buff secs"), "{bad}: {why}");
    }
    t.structures.haste_tower.action = aura(Buff::Haste { secs: 2.0 });
    for bad in [0.0, -2.0, f32::NAN, f32::INFINITY] {
        t.structures.swift_tower.action = aura(Buff::Swift { secs: bad });
        let why = t.validate().expect_err(&format!("a swift of {bad}"));
        assert!(why.contains("SwiftTower buff secs"), "{bad}: {why}");
    }

    t.structures.swift_tower.action = aura(Buff::Swift { secs: 0.05 });
    assert_eq!(t.validate(), Ok(()), "a buff of one tick");
}
