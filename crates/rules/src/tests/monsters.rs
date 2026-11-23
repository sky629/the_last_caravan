//! 몬스터의 목표 선택, 이동, 공격 (C4, C11, D17, D33).

use super::support::Scenario;
use crate::math::{Rect, Vec2};

#[test]
fn a_monster_walks_to_the_wagon_and_hits_it() {
    let mut s = Scenario::open_field();
    s.monster_at(12.0, 18.0);

    let waited = s.run_until(200, |sim| sim.wagon().hp < 500);

    let damage = s.sim.tuning().monsters.grunt.attack_damage;
    assert!(
        waited > 20,
        "it had to walk about four tiles first, took {waited}"
    );
    assert_eq!(s.sim.wagon().hp, 500 - damage, "C4");

    s.run_ticks(s.ticks(1.0));
    assert_eq!(s.sim.wagon().hp, 500 - 2 * damage, "one hit per cooldown");
}

#[test]
fn a_monster_chases_a_player_inside_the_aggro_range_and_hits_them() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    let monster = s.monster_at(8.0, 5.0);

    s.run_until(200, |sim| sim.player(a).is_some_and(|p| p.hp() < 100));

    let damage = s.sim.tuning().monsters.grunt.attack_damage;
    assert_eq!(s.player(a).hp(), 100 - damage, "C4");
    assert!(
        s.monster(monster).unwrap().pos.x < 6.5,
        "it walked to the player"
    );
    assert_eq!(s.sim.wagon().hp, 500, "the wagon was not the target");
}

#[test]
fn a_monster_ignores_a_player_outside_the_aggro_range() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 2.0);
    let monster = s.monster_at(12.0, 10.0);

    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.player(a).hp(), 100);
    let pos = s.monster(monster).unwrap().pos;
    assert!(
        pos.x > 12.0 && pos.y > 10.0,
        "it heads for the wagon, got {pos:?}"
    );
}

#[test]
fn a_monster_goes_back_to_the_wagon_when_the_player_leaves_the_aggro_range() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 7.0);
    let monster = s.monster_at(10.0, 10.0);
    s.run_ticks(s.ticks(0.5));
    assert!(
        s.monster(monster).unwrap().pos.y < 10.0,
        "chasing the player first"
    );

    s.hold_move(a, -1.0, 0.0);
    s.run_ticks(s.ticks(8.0));

    let wagon = s.sim.wagon().rect;
    let distance = wagon.distance_to(s.monster(monster).unwrap().pos);
    assert!(
        distance < 4.0,
        "D17: it is back on the way to the wagon, {distance} away"
    );
}

#[test]
fn a_monster_picks_the_nearest_of_two_players() {
    let mut s = Scenario::open_field();
    let near = s.player_at(5.0, 5.0);
    let far = s.player_at(5.0, 9.0);
    s.monster_at(5.0, 6.5);

    s.run_ticks(40);

    assert!(s.player(near).hp() < 100);
    assert_eq!(s.player(far).hp(), 100);
}

#[test]
fn a_monster_does_not_chase_or_hit_a_downed_player() {
    let mut s = Scenario::open_field();
    let downed = s.player_at(5.0, 5.0);
    s.player_at(15.0, 2.0); // 살아 있는 플레이어가 있어야 원정이 실패하지 않습니다.
    s.knock_down(downed);
    let monster = s.monster_at(5.8, 5.0);

    s.run_ticks(s.ticks(3.0));

    assert!(s.player(downed).is_downed(), "D33");
    assert_eq!(s.monster(monster).unwrap().last_attack_tick, None);
    assert!(
        s.monster(monster).unwrap().pos.x > 7.0,
        "it walked on toward the wagon"
    );
}

#[test]
fn a_monster_does_not_chase_or_hit_a_dead_player() {
    let mut s = Scenario::open_field();
    let dead = s.player_at(5.0, 5.0);
    s.player_at(15.0, 2.0);
    s.kill(dead);
    let monster = s.monster_at(5.8, 5.0);

    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.monster(monster).unwrap().last_attack_tick, None, "D33");
    assert!(s.monster(monster).unwrap().pos.x > 7.0);
}

#[test]
fn a_monster_walks_around_a_wall_to_reach_the_wagon() {
    // x = 13..14의 벽이 맵 위쪽 끝까지 이어져서, 길은 맵 아래쪽(y < 3)으로만 열려 있습니다.
    let mut s = Scenario::with_tuning(|t| t.map.obstacles = vec![Rect::new(13.0, 3.0, 14.0, 20.0)]);
    let monster = s.monster_at(10.0, 18.0);

    s.run_until(900, |sim| sim.wagon().hp < 500);

    let pos = s.monster(monster).unwrap().pos;
    assert!(
        pos.x > 14.0,
        "it ended up on the wagon's side of the wall, {pos:?}"
    );
}

#[test]
fn monsters_never_enter_obstacles_or_leave_the_map() {
    // 기본 맵에서 실제 무리가 나타나서 마차를 공격하는 동안 모든 Tick의 모든 몬스터 위치를 검사합니다 (C11).
    let mut s = Scenario::with_waves(|t| t.map = crate::tuning::MapSpec::stage1());
    let t = s.sim.tuning().clone();
    let radius = t.monsters.grunt.radius;
    let mut solids = t.map.obstacles.clone();
    solids.push(t.map.wagon);
    solids.extend(t.map.structures.iter().map(|placed| placed.rect));

    let mut seen = 0;
    for _ in 0..s.ticks(40.0) {
        s.run_ticks(1);
        for monster in s.sim.monsters() {
            seen += 1;
            let p = monster.pos;
            assert!(
                p.x >= radius - 1e-3 && p.x <= t.map.width - radius + 1e-3,
                "{p:?}"
            );
            assert!(
                p.y >= radius - 1e-3 && p.y <= t.map.height - radius + 1e-3,
                "{p:?}"
            );
            for solid in &solids {
                assert!(
                    solid.distance_to(p) >= radius - 1e-3,
                    "{p:?} is inside {solid:?}"
                );
            }
        }
    }
    assert!(seen > 500, "the check ran against many monsters, {seen}");
}

#[test]
fn monsters_spread_around_their_target_instead_of_stacking() {
    let mut s = Scenario::open_field();
    for _ in 0..8 {
        s.monster_at(13.0, 18.0);
    }

    s.run_ticks(s.ticks(15.0));

    let positions: Vec<Vec2> = s.sim.monsters().map(|m| m.pos).collect();
    let mut closest = f32::MAX;
    for (i, a) in positions.iter().enumerate() {
        for b in &positions[i + 1..] {
            closest = closest.min(a.distance(*b));
        }
    }
    let radius = s.sim.tuning().monsters.grunt.radius;
    assert!(
        closest > radius,
        "monsters overlap more than half a body, closest {closest}"
    );
    assert!(
        positions
            .iter()
            .all(|p| s.sim.wagon().rect.distance_to(*p) < 2.0),
        "and they are all at the wagon"
    );
}
