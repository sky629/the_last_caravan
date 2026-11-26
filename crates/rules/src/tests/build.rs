//! 건설을 확인합니다: 시작과 진행, 취소와 환불, 거절의 이유 다섯 가지, 동시에 진행되는 두 건설, 길을 막는 배치.
//! 테스트는 `&Sim`이 공개하는 값(포탑 목록, 보관함, 플레이어의 거절 기록)만 확인합니다.
//!
//! 시나리오는 20 x 20 맵이고 마차는 (17, 17)에서 (19, 19)까지입니다. 마차 둘레의 타일에서 짓습니다.

use super::support::Scenario;
use crate::build::{BuildRefusal, CannotBuild, front_tile};
use crate::ids::PlayerId;
use crate::input::PlayerInput;
use crate::kind::StructureKind;
use crate::math::{Rect, Vec2};
use crate::structures::TowerState;
use crate::world::Tile;

/// 건설을 마치는 데 걸리는 Tick 수입니다.
fn build_ticks(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().structures.watchtower.build_secs)
}

/// (14.5, 14.5)에서 동쪽을 보고 건설 키를 누르는 플레이어입니다. 앞 타일은 (15, 14)입니다.
fn builder_facing_east(s: &mut Scenario) -> PlayerId {
    let id = s.player_at(14.5, 14.5);
    s.hold_build(id, 1.0, 0.0);
    id
}

fn refusal(s: &Scenario, id: PlayerId) -> Option<BuildRefusal> {
    s.player(id).build_refusal().copied()
}

#[test]
fn holding_build_puts_a_site_on_the_tile_in_front_and_charges_the_stash_at_once() {
    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);

    s.run_ticks(1);

    let site = s
        .sim
        .towers()
        .next()
        .expect("the site exists from the first tick");
    assert_eq!(site.tile, Tile::new(15, 14));
    assert!(matches!(site.state, TowerState::Building { builder, .. } if builder == a));
    assert_eq!(s.sim.stash(), 0, "the cost is charged when building starts");
    assert_eq!(refusal(&s, a), None);
}

#[test]
fn a_tower_stands_once_the_key_was_held_for_the_build_time() {
    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);
    let required = build_ticks(&s);

    s.run_ticks(required - 1);
    let site = s.sim.towers().next().unwrap();
    assert!(matches!(site.state, TowerState::Building { .. }));
    assert!(s.player(a).interaction().is_some(), "still building");

    s.run_ticks(1);
    let tower = s.sim.towers().next().unwrap();
    assert!(tower.is_standing());
    assert_eq!(
        tower.hp(),
        s.sim.tuning().structures.watchtower.max_hp,
        "it stands with full HP"
    );
    assert!(s.player(a).interaction().is_none());
    assert_eq!(s.sim.stash(), 0, "the cost stays spent");
}

#[test]
fn a_new_tower_shoots_like_any_other() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 1000);
    let a = builder_facing_east(&mut s);
    s.run_ticks(build_ticks(&s));
    s.hold(a, PlayerInput::default());
    let monster = s.monster_at(16.5, 14.5);

    s.run_ticks(1);

    assert!(s.monster(monster).unwrap().hp < 1000);
}

#[test]
fn a_builder_can_neither_move_nor_attack() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    s.hold(
        a,
        PlayerInput {
            build: true,
            attack: true,
            move_dir: crate::input::MoveDir::new(0.0, -1.0),
            aim: crate::input::AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let monster = s.monster_at(14.5, 15.3);
    let start = s.player(a).pos;

    s.run_ticks(20);

    assert_eq!(s.player(a).pos, start);
    assert_eq!(s.player(a).last_attack.map(|attack| attack.tick), None);
    assert!(s.monster(monster).is_some_and(|m| m.hp > 0));
}

#[test]
fn a_site_blocks_walkers_from_the_first_tick() {
    let mut s = Scenario::open_field();
    builder_facing_east(&mut s);
    let walker = s.player_at(17.0, 14.5);
    s.hold_move(walker, -1.0, 0.0);

    s.run_ticks(s.ticks(2.0));

    let radius = s.sim.tuning().player.radius;
    assert!(
        s.player(walker).pos.x >= 16.0 + radius - 1e-3,
        "the walker stopped at the site's east edge, got {:?}",
        s.player(walker).pos
    );
}

#[test]
fn releasing_the_key_cancels_the_build_and_gives_the_stash_back() {
    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);
    s.run_ticks(build_ticks(&s) / 2);
    assert_eq!(s.sim.stash(), 0);

    s.hold(a, PlayerInput::default());
    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 0, "the site is gone");
    assert_eq!(s.sim.stash(), 10, "the cost came back");
    s.run_ticks(5);
    assert_eq!(s.sim.stash(), 10, "and it came back only once");
}

#[test]
fn a_build_that_is_started_again_begins_from_zero_progress() {
    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);
    let required = build_ticks(&s);
    s.run_ticks(required - 5);
    s.hold(a, PlayerInput::default());
    s.run_ticks(1);

    s.hold_build(a, 1.0, 0.0);
    s.run_ticks(required - 5);

    let site = s.sim.towers().next().unwrap();
    assert!(
        matches!(site.state, TowerState::Building { .. }),
        "the earlier progress did not carry over"
    );
}

#[test]
fn a_builder_who_goes_down_cancels_the_build_with_the_same_refund() {
    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);
    s.player_at(1.0, 1.0);
    s.run_ticks(10);

    s.knock_down(a);
    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 0);
    assert_eq!(s.sim.stash(), 10);
}

#[test]
fn a_builder_who_leaves_cancels_the_build_with_the_same_refund() {
    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);
    s.run_ticks(10);

    s.sim.remove_player(a);
    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 0);
    assert_eq!(s.sim.stash(), 10);
}

#[test]
fn being_attacked_alone_does_not_cancel_a_build() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.attack_damage = 1);
    let a = builder_facing_east(&mut s);
    s.monster_at(14.5, 13.7);

    s.run_ticks(build_ticks(&s));

    assert!(
        s.sim
            .towers()
            .next()
            .is_some_and(|tower| tower.is_standing())
    );
    assert!(s.player(a).hp() < 100, "the monster did hit the builder");
}

#[test]
fn a_tower_can_be_built_during_the_night() {
    let mut s = Scenario::open_field();
    s.sim.set_phase(crate::expedition::Phase::Night {
        day: 1,
        party: 1,
        step: crate::expedition::NightStep::Fighting {
            wave: 1,
            to_spawn: 1,
            spawn_in_ticks: u32::MAX,
        },
    });
    builder_facing_east(&mut s);

    s.run_ticks(build_ticks(&s));

    assert!(
        s.sim
            .towers()
            .next()
            .is_some_and(|tower| tower.is_standing())
    );
}

#[test]
fn a_builder_overlapping_the_new_tile_is_pushed_out_of_it() {
    let mut s = Scenario::open_field();
    // 몸이 앞 타일 (15, 14)에 0.2만큼 걸쳐 있습니다.
    let a = s.player_at(14.85, 14.5);
    s.hold_build(a, 1.0, 0.0);

    s.run_ticks(1);

    let radius = s.sim.tuning().player.radius;
    assert_eq!(s.sim.towers().next().unwrap().tile, Tile::new(15, 14));
    assert!(
        s.player(a).pos.x <= 15.0 - radius + 1e-3,
        "the builder stands outside the site, got {:?}",
        s.player(a).pos
    );
}

#[test]
fn a_site_that_a_body_still_covers_when_it_is_done_is_cancelled_and_refunded() {
    let mut s = Scenario::open_field();
    builder_facing_east(&mut s);
    s.run_ticks(1);
    // 사망한 플레이어는 움직이지 않으므로, 짓는 자리 안에 놓인 몸이 밀려나지 않습니다.
    let body = s.player_at(15.5, 14.5);
    s.kill(body);

    s.run_ticks(build_ticks(&s));

    assert_eq!(s.sim.towers().count(), 0, "the tower was not built");
    assert_eq!(s.sim.stash(), 10, "the cost came back");
}

#[test]
fn the_f_key_comes_before_the_build_key_when_there_is_something_to_do() {
    let mut s = Scenario::open_field();
    let reviver = s.player_at(14.5, 14.5);
    let downed = s.player_at(13.5, 14.5);
    s.player_at(1.0, 1.0);
    s.knock_down(downed);
    s.hold(
        reviver,
        PlayerInput {
            interact: true,
            build: true,
            aim: crate::input::AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );

    s.run_ticks(5);

    assert!(s.sim.revive_progress(downed).is_some(), "reviving");
    assert_eq!(s.sim.towers().count(), 0, "nothing was built");
}

// 거절의 이유 다섯 가지. 어느 경우에도 보관함이 그대로이고 포탑이 늘지 않으며, 거절은 놓으려던 타일과 함께 남습니다.

fn assert_refused(s: &Scenario, id: PlayerId, tile: Tile, why: CannotBuild, stash: u32) {
    assert_eq!(refusal(s, id), Some(BuildRefusal { tile, why }));
    assert_eq!(s.sim.stash(), stash, "nothing was spent");
    assert!(
        s.sim.towers().all(|tower| tower.is_standing()),
        "no site was started"
    );
    assert!(s.player(id).interaction().is_none());
}

#[test]
fn a_build_without_enough_stash_is_refused() {
    let mut s = Scenario::open_field();
    s.set_stash(9);
    let a = builder_facing_east(&mut s);

    s.run_ticks(3);

    assert_refused(&s, a, Tile::new(15, 14), CannotBuild::NotEnoughStash, 9);
}

#[test]
fn a_build_on_a_tile_that_is_taken_is_refused() {
    let mut s = Scenario::with_tuning(|t| {
        t.map.obstacles = vec![Tile::new(15, 14).rect()];
    });
    let a = builder_facing_east(&mut s);
    s.run_ticks(3);
    assert_refused(&s, a, Tile::new(15, 14), CannotBuild::TileTaken, 10);

    let mut s = Scenario::open_field();
    s.tower_at(15, 14);
    let a = builder_facing_east(&mut s);
    s.run_ticks(3);
    assert_refused(&s, a, Tile::new(15, 14), CannotBuild::TileTaken, 10);
}

#[test]
fn a_build_on_a_tile_with_a_body_on_it_is_refused() {
    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);
    s.player_at(15.5, 14.5);
    s.run_ticks(3);
    assert_refused(&s, a, Tile::new(15, 14), CannotBuild::TileTaken, 10);

    let mut s = Scenario::open_field();
    let a = builder_facing_east(&mut s);
    s.monster_at(15.5, 14.5);
    s.run_ticks(1);
    assert_refused(&s, a, Tile::new(15, 14), CannotBuild::TileTaken, 10);
}

#[test]
fn a_build_too_far_from_the_wagon_is_refused() {
    let mut s = Scenario::with_tuning(|t| t.village.build_range = 2.0);
    let a = s.player_at(10.5, 10.5);
    s.hold_build(a, 1.0, 0.0);

    s.run_ticks(3);

    assert_refused(&s, a, Tile::new(11, 10), CannotBuild::OutsideVillage, 10);
}

#[test]
fn a_build_when_the_tower_limit_is_reached_is_refused() {
    let mut s = Scenario::with_tuning(|t| t.village.tower_limit = 2);
    s.tower_at(1, 1);
    s.tower_at(3, 1);
    let a = builder_facing_east(&mut s);

    s.run_ticks(3);

    assert_refused(&s, a, Tile::new(15, 14), CannotBuild::TowerLimit, 10);
}

#[test]
fn a_site_under_construction_counts_toward_the_tower_limit() {
    let mut s = Scenario::with_tuning(|t| t.village.tower_limit = 2);
    s.set_stash(100);
    s.tower_at(1, 1);
    let first = builder_facing_east(&mut s);
    s.run_ticks(1);
    assert_eq!(s.sim.towers().count(), 2);

    let second = s.player_at(14.5, 10.5);
    s.hold_build(second, 1.0, 0.0);
    s.run_ticks(1);

    assert_eq!(
        refusal(&s, second).map(|r| r.why),
        Some(CannotBuild::TowerLimit)
    );
    assert_eq!(refusal(&s, first), None);
}

#[test]
fn the_reason_stays_while_the_key_is_held_and_goes_when_it_is_released() {
    let mut s = Scenario::open_field();
    s.set_stash(0);
    let a = builder_facing_east(&mut s);
    s.run_ticks(5);
    assert!(refusal(&s, a).is_some());

    s.hold(a, PlayerInput::default());
    s.run_ticks(1);

    assert_eq!(refusal(&s, a), None);
}

#[test]
fn the_reason_is_in_the_first_tick_and_the_build_starts_once_it_is_possible() {
    let mut s = Scenario::open_field();
    s.set_stash(0);
    let a = builder_facing_east(&mut s);
    s.run_ticks(1);
    assert_eq!(
        refusal(&s, a).map(|r| r.why),
        Some(CannotBuild::NotEnoughStash)
    );

    s.set_stash(10);
    s.run_ticks(1);

    assert_eq!(refusal(&s, a), None);
    assert_eq!(s.sim.towers().count(), 1);
}

// 길을 막는 배치.

/// 마차 둘레를 지도의 가장자리와 함께 둘러싸는 ㄴ자 모양의 타일 일곱 개입니다.
/// 이 가운데 여섯 개가 서 있으면 마지막 하나가 마차 둘레를 지도의 나머지에서 떼어 놓습니다.
const WALL: [(u16, u16); 7] = [
    (16, 19),
    (16, 18),
    (16, 17),
    (16, 16),
    (17, 16),
    (18, 16),
    (19, 16),
];

/// `WALL`의 타일 하나를 건설하는 플레이어입니다. 그 타일의 남서쪽 이웃 타일에 서서 북동쪽을 봅니다.
fn builder_toward(s: &mut Scenario, tile: (u16, u16)) -> PlayerId {
    let id = s.player_at(f32::from(tile.0) - 0.5, f32::from(tile.1) - 0.5);
    s.hold_build(id, 1.0, 1.0);
    id
}

#[test]
fn the_wagon_cannot_be_walled_in_whatever_tile_comes_last() {
    for (last, (x, y)) in WALL.into_iter().enumerate() {
        let mut s = Scenario::open_field();
        for (index, (other_x, other_y)) in WALL.into_iter().enumerate() {
            if index != last {
                s.tower_at(other_x, other_y);
            }
        }
        s.set_stash(100);
        let a = builder_toward(&mut s, (x, y));

        s.run_ticks(3);

        assert_eq!(
            refusal(&s, a),
            Some(BuildRefusal {
                tile: Tile::new(x, y),
                why: CannotBuild::WouldBlockPath
            }),
            "the last tile {:?} closes the wall",
            (x, y)
        );
        assert_eq!(s.sim.stash(), 100, "nothing was spent");
    }
}

/// 지도를 서쪽과 동쪽으로 가르는 벽에 구멍이 둘 있는 시나리오입니다. 마차는 동쪽에 있습니다.
/// 위쪽 구멍은 (10, 3)이고 마을의 범위 밖이며, 아래쪽 구멍은 (10, 15)이고 마을의 범위 안입니다. `chest_site`가 후보 자리입니다.
fn wall_with_two_gaps(chest_site: Option<Tile>) -> Scenario {
    Scenario::with_tuning(|t| {
        t.map.obstacles = vec![
            Rect::new(10.0, 0.0, 11.0, 3.0),
            Rect::new(10.0, 4.0, 11.0, 15.0),
            Rect::new(10.0, 16.0, 11.0, 20.0),
        ];
        t.map.player_spawns = vec![Vec2::new(14.0, 10.0)];
        t.map.chest_sites = chest_site.into_iter().collect();
    })
}

#[test]
fn a_tower_that_would_leave_a_chest_site_as_the_only_way_through_is_refused() {
    let mut s = wall_with_two_gaps(Some(Tile::new(10, 3)));
    s.set_stash(100);
    let a = builder_toward(&mut s, (10, 15));

    s.run_ticks(3);

    assert_eq!(
        refusal(&s, a),
        Some(BuildRefusal {
            tile: Tile::new(10, 15),
            why: CannotBuild::WouldBlockPath
        }),
        "on a day the chest stands there, the west side would be cut off"
    );
    assert_eq!(s.sim.stash(), 100, "nothing was spent");
}

#[test]
fn the_same_tower_is_built_when_no_chest_can_ever_stand_in_the_other_gap() {
    let mut s = wall_with_two_gaps(None);
    s.set_stash(100);
    let a = builder_toward(&mut s, (10, 15));

    s.run_ticks(3);

    assert_eq!(refusal(&s, a), None);
    assert_eq!(s.sim.towers().count(), 1);
}

#[test]
fn a_monster_still_reaches_the_wagon_after_the_last_tile_is_refused() {
    // 마차를 둘러싼 벽에서 한 곳(16, 19)만 폭 1타일의 틈으로 열려 있습니다.
    let mut s = Scenario::with_tuning(|t| {
        t.map.obstacles = WALL[1..]
            .iter()
            .map(|(x, y)| Tile::new(*x, *y).rect())
            .collect();
    });
    let a = builder_toward(&mut s, WALL[0]);
    s.run_ticks(3);
    assert!(refusal(&s, a).is_some(), "the gap is not closed");
    s.sim.remove_player(a);
    let monster = s.monster_at(10.5, 18.5);

    s.run_until(600, |sim| sim.wagon().hp < 500);

    assert!(s.monster(monster).unwrap().pos.x > 16.0);
}

#[test]
fn two_builds_started_in_the_same_tick_cannot_close_the_last_gap_together() {
    let mut s = Scenario::open_field();
    // 두 곳이 남아 있습니다: (16, 19)와 (16, 18). 하나씩은 막아도 되지만 둘이 함께는 막으면 안 됩니다.
    for (x, y) in &WALL[2..] {
        s.tower_at(*x, *y);
    }
    s.set_stash(100);
    let first = builder_toward(&mut s, WALL[0]);
    let second = builder_toward(&mut s, WALL[1]);

    s.run_ticks(1);

    assert_eq!(refusal(&s, first), None, "the lower id starts");
    assert!(
        s.sim.towers().any(|tower| tower.tile == Tile::new(16, 19)),
        "its site exists"
    );
    assert_eq!(
        refusal(&s, second),
        Some(BuildRefusal {
            tile: Tile::new(16, 18),
            why: CannotBuild::WouldBlockPath
        }),
        "together the two would block the way to the wagon"
    );
}

#[test]
fn two_builders_aiming_at_one_tile_start_one_site_and_refuse_the_other() {
    let mut s = Scenario::open_field();
    s.set_stash(100);
    let first = builder_facing_east(&mut s);
    let second = s.player_at(15.5, 13.5);
    s.hold_build(second, 0.0, 1.0);

    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 1);
    assert_eq!(refusal(&s, first), None);
    assert_eq!(
        refusal(&s, second).map(|r| r.why),
        Some(CannotBuild::TileTaken)
    );
    assert_eq!(s.sim.stash(), 90, "only one cost was charged");
}

#[test]
fn one_stash_pays_for_one_site_when_two_start_in_the_same_tick() {
    let mut s = Scenario::open_field();
    let first = builder_facing_east(&mut s);
    let second = s.player_at(14.5, 10.5);
    s.hold_build(second, 1.0, 0.0);

    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 1);
    assert_eq!(refusal(&s, first), None);
    assert_eq!(
        refusal(&s, second).map(|r| r.why),
        Some(CannotBuild::NotEnoughStash)
    );
}

// 놓일 자리. 서버와 클라이언트의 미리 보기가 같은 함수(`front_tile`)로 정합니다.

/// 서 있는 타일 (5, 5)에서 여덟 방향을 바라볼 때의 앞 타일입니다.
const AIMED_FROM_5_5: [((f32, f32), (u16, u16)); 8] = [
    ((1.0, 0.0), (6, 5)),
    ((0.7, 0.7), (6, 6)),
    ((0.0, 1.0), (5, 6)),
    ((-0.7, 0.7), (4, 6)),
    ((-1.0, 0.0), (4, 5)),
    ((-0.7, -0.7), (4, 4)),
    ((0.0, -1.0), (5, 4)),
    ((0.7, -0.7), (6, 4)),
];

#[test]
fn the_front_tile_is_the_neighbour_in_the_aimed_direction() {
    for ((x, y), (tile_x, tile_y)) in AIMED_FROM_5_5 {
        let facing = Vec2::new(x, y).normalize_or_zero();

        assert_eq!(
            front_tile(Vec2::new(5.9, 5.1), facing, (20, 20)),
            Some(Tile::new(tile_x, tile_y)),
            "facing ({x}, {y})"
        );
    }
}

#[test]
fn the_front_tile_does_not_depend_on_where_inside_the_tile_the_player_stands() {
    let facing = Vec2::new(1.0, 0.0);

    for pos in [(5.01, 5.01), (5.5, 5.5), (5.99, 5.99)] {
        assert_eq!(
            front_tile(Vec2::new(pos.0, pos.1), facing, (20, 20)),
            Some(Tile::new(6, 5))
        );
    }
}

#[test]
fn there_is_no_front_tile_past_the_edge_of_the_map() {
    let size = (20, 20);

    assert_eq!(
        front_tile(Vec2::new(0.5, 5.5), Vec2::new(-1.0, 0.0), size),
        None
    );
    assert_eq!(
        front_tile(Vec2::new(19.5, 5.5), Vec2::new(1.0, 0.0), size),
        None
    );
    assert_eq!(
        front_tile(Vec2::new(5.5, 0.5), Vec2::new(0.0, -1.0), size),
        None
    );
    assert_eq!(
        front_tile(Vec2::new(5.5, 19.5), Vec2::new(0.0, 1.0), size),
        None
    );
    assert_eq!(
        front_tile(
            Vec2::new(19.5, 19.5),
            Vec2::new(-1.0, -1.0).normalize_or_zero(),
            size
        ),
        Some(Tile::new(18, 18))
    );
}

#[test]
fn the_server_builds_on_the_tile_the_front_tile_names() {
    for ((x, y), _) in AIMED_FROM_5_5 {
        let mut s = Scenario::open_field();
        let a = s.player_at(14.9, 14.1);
        s.hold_build(a, x, y);

        s.run_ticks(1);

        let facing = s.player(a).facing;
        let site = s.sim.towers().next().expect("the site exists");
        assert_eq!(
            Some(site.tile),
            front_tile(s.player(a).pos, facing, (20, 20)),
            "aiming ({x}, {y})"
        );
    }
}

#[test]
fn the_kinds_are_chosen_in_the_order_of_all_and_the_last_is_followed_by_the_first() {
    assert_eq!(
        StructureKind::ALL,
        [
            StructureKind::Watchtower,
            StructureKind::SlowTower,
            StructureKind::StunTower,
        ]
    );
    for (index, kind) in StructureKind::ALL.into_iter().enumerate() {
        let after = StructureKind::ALL[(index + 1) % StructureKind::ALL.len()];
        assert_eq!(kind.next(), after, "{kind:?}");
    }
}

#[test]
fn each_press_of_e_moves_the_choice_to_the_next_kind_and_the_last_goes_back_to_the_first() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    assert_eq!(s.player(a).build_kind, StructureKind::Watchtower);

    let mut chosen = Vec::new();
    for _ in 0..StructureKind::ALL.len() {
        s.tap_tower_key(a);
        chosen.push(s.player(a).build_kind);
    }

    assert_eq!(
        chosen,
        [
            StructureKind::SlowTower,
            StructureKind::StunTower,
            StructureKind::Watchtower,
        ]
    );
    s.run_ticks(5);
    assert_eq!(
        s.player(a).build_kind,
        StructureKind::Watchtower,
        "only a press changes the choice"
    );
}

#[test]
fn a_press_of_e_that_the_server_repeats_in_late_ticks_changes_the_choice_once() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    let pressed = PlayerInput {
        cycle_tower: true,
        ..PlayerInput::default()
    };

    s.hold(a, pressed);
    s.run_ticks(1);
    s.hold(a, pressed.held());
    s.run_ticks(10);

    assert_eq!(s.player(a).build_kind, StructureKind::SlowTower);
}

#[test]
fn two_presses_of_e_then_holding_b_builds_a_stun_tower_for_20_and_a_cancel_returns_the_20() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    s.set_stash(30);
    s.tap_tower_key(a);
    s.tap_tower_key(a);

    s.hold_build(a, 1.0, 0.0);
    s.run_ticks(1);

    let site = s.sim.towers().next().expect("a site");
    assert_eq!(site.kind, StructureKind::StunTower);
    assert_eq!(s.sim.stash(), 10, "the stun tower costs 20");
    s.hold(a, PlayerInput::default());
    s.run_ticks(1);
    assert_eq!(s.sim.towers().count(), 0, "the site is gone");
    assert_eq!(s.sim.stash(), 30, "the 20 came back");
}

#[test]
fn a_tower_that_is_built_to_the_end_is_the_kind_that_was_chosen_when_the_build_began() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    s.set_stash(30);
    s.tap_tower_key(a);

    s.hold_build(a, 1.0, 0.0);
    s.run_ticks(s.ticks(s.sim.tuning().structures.slow_tower.build_secs));

    let tower = s.sim.towers().next().expect("the tower");
    assert!(tower.is_standing());
    assert_eq!(tower.kind, StructureKind::SlowTower);
    assert_eq!(s.sim.stash(), 15, "the slow tower costs 15");
}

#[test]
fn e_and_b_in_the_same_tick_build_the_kind_the_press_chose() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    s.set_stash(30);
    s.hold_build(a, 1.0, 0.0);

    s.tap_tower_key(a);

    let site = s.sim.towers().next().expect("a site");
    assert_eq!(site.kind, StructureKind::SlowTower);
}

#[test]
fn a_press_of_e_while_building_does_not_change_the_tower_being_built() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    s.set_stash(30);
    s.tap_tower_key(a);
    s.hold_build(a, 1.0, 0.0);
    s.run_ticks(1);

    s.tap_tower_key(a);

    let site = s.sim.towers().next().expect("the site");
    assert_eq!(
        site.kind,
        StructureKind::SlowTower,
        "the site keeps its kind"
    );
    assert_eq!(
        s.player(a).build_kind,
        StructureKind::StunTower,
        "the next build"
    );
    assert_eq!(s.sim.stash(), 15, "nothing was paid again");
}

#[test]
fn a_kind_that_costs_more_than_the_stash_is_refused_for_the_stash() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    s.set_stash(19);
    s.tap_tower_key(a);
    s.tap_tower_key(a);

    s.hold_build(a, 1.0, 0.0);
    s.run_ticks(1);

    assert_eq!(
        s.sim.towers().count(),
        0,
        "19 is not enough for a stun tower"
    );
    assert_eq!(s.sim.stash(), 19);
    assert_eq!(
        refusal(&s, a).map(|refusal| refusal.why),
        Some(CannotBuild::NotEnoughStash)
    );
}

#[test]
fn the_choice_is_per_player_and_a_new_expedition_or_a_new_player_starts_at_the_watchtower() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 14.5);
    let b = s.player_at(12.5, 14.5);

    s.tap_tower_key(a);

    assert_eq!(s.player(a).build_kind, StructureKind::SlowTower);
    assert_eq!(
        s.player(b).build_kind,
        StructureKind::Watchtower,
        "b chose nothing"
    );

    s.restart_expedition();
    assert_eq!(
        s.player(a).build_kind,
        StructureKind::Watchtower,
        "a new expedition"
    );
    let late = s.player_at(10.5, 10.5);
    assert_eq!(
        s.player(late).build_kind,
        StructureKind::Watchtower,
        "a new player"
    );
}

#[test]
fn e_changes_nothing_for_a_downed_player_and_the_choice_survives_a_revive() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    let helper = s.player_at(5.5, 5.0);
    s.tap_tower_key(a);
    s.knock_down(a);

    s.tap_tower_key(a);
    assert_eq!(
        s.player(a).build_kind,
        StructureKind::SlowTower,
        "no change while downed"
    );

    s.hold_interact(helper);
    s.run_until(200, |sim| sim.player(a).is_some_and(|p| p.is_alive()));
    assert_eq!(
        s.player(a).build_kind,
        StructureKind::SlowTower,
        "the choice is kept"
    );
}

#[test]
fn e_changes_nothing_in_the_camp() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(4.0, 4.0);

    s.tap_tower_key(a);

    assert_eq!(s.player(a).build_kind, StructureKind::Watchtower);
}

#[test]
fn the_default_costs_of_the_kinds_are_10_15_and_20() {
    let t = crate::tuning::Tuning::default();
    let costs = StructureKind::ALL.map(|kind| t.structures.get(kind).build_cost);
    assert_eq!(costs, [10, 15, 20]);
}
