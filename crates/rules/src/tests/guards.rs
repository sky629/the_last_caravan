//! 낮의 몬스터를 확인합니다: 낮이 시작될 때 채집 지점마다 놓이는 자리, 자리를 지키는 행동, 플레이어를 쫓다가 돌아가는 행동,
//! 밤이 되면 마차를 향하는 전환, 낮마다 다시 놓이는 것.
//!
//! 시나리오는 20 x 20 맵이고 마차는 (17, 17)에서 (19, 19)까지입니다. 채집 지점 (2, 2)는 마을의 범위(10타일) 밖에 있고,
//! 지점마다 몬스터가 둘 지킵니다. 지키는 자리는 지점의 이웃 타일 가운데 상하좌우가 먼저이므로 동쪽 (3, 2)와 서쪽 (1, 2)입니다.

use super::support::{Scenario, nights, phase};
use crate::expedition::{NightStep, Phase};
use crate::math::Vec2;
use crate::monster::{Monster, Order};
use crate::world::Tile;

/// 채집 지점 (2, 2)가 하나 있고 몬스터가 둘 지키는 시나리오입니다.
fn guarded_node() -> Scenario {
    Scenario::with_tuning(|t| {
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 2;
    })
}

fn guards(s: &Scenario) -> Vec<Monster> {
    s.sim.monsters().cloned().collect()
}

fn post_of(monster: &Monster) -> Tile {
    match monster.order {
        Order::Guard { post } => post,
        Order::Raid | Order::Stand => panic!("{monster:?} is not a guard"),
    }
}

#[test]
fn a_day_starts_with_the_set_number_of_guards_on_free_tiles_beside_every_node() {
    let mut s = Scenario::with_tuning(|t| {
        t.map.nodes = vec![Tile::new(2, 2), Tile::new(2, 12)];
        t.guards.per_node = 2;
    });

    s.run_ticks(1);

    let found = guards(&s);
    assert_eq!(found.len(), 4, "two for each node, none twice");
    let mut posts: Vec<Tile> = found.iter().map(post_of).collect();
    posts.sort();
    assert_eq!(
        posts,
        [
            Tile::new(1, 2),
            Tile::new(1, 12),
            Tile::new(3, 2),
            Tile::new(3, 12)
        ],
        "the east and the west neighbour of each node"
    );
    for guard in &found {
        let post = post_of(guard);
        assert!(
            guard.pos.distance(post.center()) < 0.1,
            "{guard:?} stands on its post"
        );
    }
}

#[test]
fn guards_stay_on_their_posts_while_nobody_comes_near() {
    let mut s = guarded_node();
    let before = guards(&s);

    s.run_ticks(s.ticks(5.0));

    assert_eq!(guards(&s), before, "nothing moved or attacked");
    assert_eq!(s.sim.wagon().hp, 500);
}

#[test]
fn a_guard_chases_a_player_who_comes_within_the_chase_range_of_its_post_and_hits_them() {
    let mut s = guarded_node();
    // 동쪽 자리 (3, 2)의 중심 (3.5, 2.5)에서 4타일 떨어져 있습니다.
    let a = s.player_at(7.5, 2.5);

    s.run_until(200, |sim| sim.player(a).is_some_and(|p| p.hp() < 100));

    let damage = s.sim.tuning().monsters.grunt.attack_damage;
    assert_eq!(s.player(a).hp(), 100 - damage);
    assert_eq!(s.sim.wagon().hp, 500, "the wagon was not the target");
}

#[test]
fn a_guard_ignores_a_player_outside_the_chase_range_of_its_post() {
    let mut s = guarded_node();
    let before = guards(&s);
    // 동쪽 자리에서 7타일, 서쪽 자리에서 9타일이라 둘 다 추적 범위(6타일) 밖입니다.
    let a = s.player_at(10.5, 2.5);

    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.player(a).hp(), 100);
    assert_eq!(guards(&s), before);
}

#[test]
fn the_chase_range_is_measured_from_the_post_so_a_guard_far_from_its_post_turns_back() {
    let mut s = guarded_node();
    let east = guards(&s)
        .into_iter()
        .find(|m| post_of(m) == Tile::new(3, 2))
        .expect("the east guard exists");
    s.sim
        .world_mut()
        .monsters
        .iter_mut()
        .find(|m| m.id == east.id)
        .expect("the east guard exists")
        .pos = Vec2::new(6.0, 2.5);
    // 플레이어는 몬스터에서 4타일(몬스터의 추적 거리 안)이지만 자리에서는 6.5타일(자리의 추적 범위 밖)입니다.
    let a = s.player_at(10.0, 2.5);

    s.run_ticks(s.ticks(1.0));

    let guard = s.monster(east.id).expect("the guard is alive");
    assert!(guard.pos.x < 6.0, "it walked back, got {:?}", guard.pos);
    assert_eq!(s.player(a).hp(), 100);
}

#[test]
fn a_guard_that_lost_its_player_goes_back_to_its_post() {
    let mut s = guarded_node();
    let a = s.player_at(7.5, 2.5);
    s.run_until(200, |sim| {
        sim.monsters()
            .any(|m| m.pos.distance(post_of(m).center()) > 1.0)
    });

    s.hold_move(a, 1.0, 0.0);
    s.run_until(s.ticks(30.0), |sim| {
        sim.monsters()
            .all(|m| m.pos.distance(post_of(m).center()) < 0.1)
    });

    let hp = s.player(a).hp();
    s.run_ticks(s.ticks(3.0));
    assert_eq!(s.player(a).hp(), hp, "it does not follow beyond its range");
}

#[test]
fn a_guard_does_not_chase_a_downed_player() {
    let mut s = guarded_node();
    let downed = s.player_at(6.5, 2.5);
    s.player_at(15.0, 12.0);
    s.knock_down(downed);
    let before = guards(&s);

    s.run_ticks(s.ticks(3.0));

    assert_eq!(guards(&s), before);
}

#[test]
fn at_nightfall_a_guard_heads_for_the_wagon_instead_of_going_back_to_its_post() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 1.0;
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 2;
        t.nights.first_wave_delay_secs = 60.0;
    });
    let wagon = s.sim.wagon().rect;
    let at_the_start: Vec<f32> = guards(&s)
        .iter()
        .map(|m| wagon.distance_to(m.pos))
        .collect();
    assert!(
        guards(&s)
            .iter()
            .all(|m| matches!(m.order, Order::Guard { .. }))
    );

    s.run_until(100, |sim| matches!(phase(sim), Phase::Night { .. }));
    assert!(
        guards(&s).iter().all(|m| m.order == Order::Raid),
        "the day's monsters take the order of the night's"
    );
    s.run_ticks(s.ticks(4.0));

    for (guard, start) in guards(&s).iter().zip(at_the_start) {
        assert!(
            wagon.distance_to(guard.pos) < start - 5.0,
            "{guard:?} walked toward the wagon"
        );
    }
}

#[test]
fn a_guard_that_reached_the_wagon_at_night_attacks_it_like_a_night_monster() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 1;
        t.nights.first_wave_delay_secs = 600.0;
    });

    s.run_until(s.ticks(60.0), |sim| sim.wagon().hp < 500);

    assert!(s.sim.wagon().hp < 500);
}

#[test]
fn guards_that_were_killed_by_day_are_not_there_at_night() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 1.0;
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 2;
        t.nights.first_wave_delay_secs = 600.0;
    });
    let killed = guards(&s)[0].id;
    s.sim.world_mut().monsters.retain(|m| m.id != killed);

    s.run_until(100, |sim| matches!(phase(sim), Phase::Night { .. }));

    assert_eq!(guards(&s).len(), 1);
}

/// 낮이 짧고 밤마다 몬스터가 하나인 두 날짜리 원정에, 몬스터가 둘 지키는 채집 지점 (2, 2)가 있는 시나리오입니다.
fn two_short_days() -> Scenario {
    let plans = nights(&[&[1], &[1]]);
    Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.nights.plans = plans;
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 2;
    })
}

/// 밤에 나타난 몬스터를 모두 처치한 상황을 만듭니다. 낮의 몬스터였던 몬스터도 함께 처치합니다.
fn clear_the_night(s: &mut Scenario, day: u32) {
    s.run_until(2000, |sim| {
        matches!(
            phase(sim),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });
    s.clear_monsters();
    s.run_until(
        2000,
        |sim| matches!(phase(sim), Phase::Day { day: d, .. } if d == day),
    );
}

#[test]
fn every_new_day_places_the_guards_again_exactly_once() {
    let mut s = two_short_days();
    assert_eq!(guards(&s).len(), 2, "the first day starts with its guards");

    clear_the_night(&mut s, 2);
    s.run_ticks(3);

    assert_eq!(guards(&s).len(), 2, "the second day: two, not four");
    assert!(
        guards(&s)
            .iter()
            .all(|m| matches!(m.order, Order::Guard { .. }))
    );
}

#[test]
fn a_new_expedition_starts_with_its_guards_too() {
    let mut s = two_short_days();
    s.player_at(10.0, 10.0);
    s.set_wagon_hp(0);

    s.run_until(2000, |sim| sim.expedition_number() == 2);
    s.run_ticks(3);

    assert_eq!(guards(&s).len(), 2, "two, not four");
    assert!(matches!(phase(&s.sim), Phase::Day { day: 1, .. }));
}

#[test]
fn a_night_is_not_cleared_while_a_guard_who_became_a_raider_is_alive() {
    let mut s = two_short_days();
    let guard_ids: Vec<_> = guards(&s).iter().map(|m| m.id).collect();
    s.run_until(2000, |sim| {
        matches!(
            phase(sim),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });

    s.sim
        .world_mut()
        .monsters
        .retain(|m| guard_ids.contains(&m.id));
    s.run_ticks(5);

    assert!(
        matches!(
            phase(&s.sim),
            Phase::Night {
                step: NightStep::Fighting { .. },
                ..
            }
        ),
        "the former guards still count, got {:?}",
        phase(&s.sim)
    );
}

#[test]
fn guards_count_toward_the_limit_on_monsters_alive_at_night() {
    let plans = nights(&[&[6]]);
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.nights.plans = plans;
        t.nights.max_alive = 3;
        t.nights.first_wave_delay_secs = 0.05;
        t.nights.spawn_interval_secs = 0.05;
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 2;
        t.wagon.max_hp = u32::MAX;
    });

    for _ in 0..200 {
        s.run_ticks(1);
        assert!(s.sim.monsters().count() <= 3, "the cap holds");
    }
}

#[test]
fn a_guard_goes_back_to_a_post_that_is_a_long_way_off_through_the_gap_in_a_wall() {
    // 지키는 자리로 돌아가는 길도 마차로 가는 길과 같은 타일 지도를 읽으므로, 벽을 돌아서 갑니다.
    let mut s = Scenario::with_tuning(|t| {
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 1;
        t.map.obstacles = vec![crate::math::Rect::new(0.0, 5.0, 8.0, 6.0)];
    });
    let guard = guards(&s)[0].id;
    let post = post_of(&guards(&s)[0]);
    // 벽 너머로 끌려 나온 상황을 만듭니다.
    s.sim
        .world_mut()
        .monsters
        .iter_mut()
        .find(|m| m.id == guard)
        .expect("the guard exists")
        .pos = Vec2::new(4.5, 8.5);

    s.run_until(s.ticks(30.0), |sim| {
        sim.monsters()
            .find(|m| m.id == guard)
            .is_some_and(|m| m.pos.distance(post.center()) < 0.1)
    });
}

#[test]
fn a_guard_post_is_never_inside_the_village_where_a_tower_could_stand() {
    // 이 지점은 마을의 범위 바로 밖에 있고, 동쪽 이웃 타일은 범위 안입니다.
    let s = Scenario::with_cycle(|t| {
        t.map.nodes = vec![Tile::new(6, 17)];
        t.guards.per_node = 1;
    });
    let t = s.sim.tuning();

    let posts: Vec<Tile> = guards(&s).iter().map(post_of).collect();

    assert_eq!(posts.len(), 1);
    assert!(
        t.map.wagon.distance_to(posts[0].center()) > t.village.build_range,
        "the post {:?} is outside the build range",
        posts[0]
    );
}
