//! 몬스터의 목표 선택, 이동, 공격 (C4, C11, D17, D33).

use super::support::Scenario;
use crate::expedition::{Outcome, Phase};
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

#[test]
fn a_crowd_queues_behind_the_front_line_instead_of_piling_onto_one_spot() {
    let mut s = Scenario::open_field();
    for row in 0..10 {
        for column in 0..10 {
            s.monster_at(6.0 + 0.5 * column as f32, 9.0 + 0.5 * row as f32);
        }
    }

    s.run_ticks(s.ticks(25.0));

    let positions: Vec<Vec2> = s.sim.monsters().map(|m| m.pos).collect();
    let half_body = s.sim.tuning().monsters.grunt.radius;
    let mut deep_overlaps = 0;
    for (i, a) in positions.iter().enumerate() {
        for b in &positions[i + 1..] {
            if a.distance(*b) < half_body {
                deep_overlaps += 1;
            }
        }
    }
    let nearest_to_wagon = positions
        .iter()
        .map(|p| s.sim.wagon().rect.distance_to(*p))
        .fold(f32::MAX, f32::min);
    assert!(
        nearest_to_wagon < 1.0,
        "the front of the crowd reached the wagon"
    );
    assert!(
        deep_overlaps <= 5,
        "{deep_overlaps} pairs sit on top of each other"
    );
}

/// 몬스터 300마리가 1 Tick 간격으로 나타나는 한 무리입니다.
/// 마차와 감시탑은 버티고, 맵 전체를 덮는 감시탑이 2 Tick마다 한 마리씩 처치합니다.
fn crowd_of_300() -> Scenario {
    Scenario::with_waves(|t| {
        t.map = crate::tuning::MapSpec::stage1();
        t.waves.sizes = vec![300];
        t.waves.first_delay_secs = 0.05;
        t.waves.spawn_interval_secs = 0.05;
        t.wagon.max_hp = u32::MAX;
        let tower = &mut t.structures.watchtower;
        tower.max_hp = u32::MAX;
        tower.range = 40.0;
        tower.damage = t.monsters.grunt.max_hp;
        tower.cooldown_secs = 0.1;
    })
}

#[test]
fn a_wave_of_300_monsters_runs_to_victory() {
    // 몬스터가 수백 마리여도 Tick이 끝까지 진행되는지 확인합니다. 근접 공격, 감시탑의 목표 선택, 몬스터 사이의 밀어내기가
    // 모두 격자 조회를 거치므로, 모든 쌍을 비교하는 방식이었다면 이 테스트는 눈에 띄게 느려집니다.
    let mut s = crowd_of_300();
    let mut most_alive = 0;

    s.run_until(3000, |sim| {
        most_alive = most_alive.max(sim.monsters().count());
        matches!(sim.phase(), Phase::Over { .. })
    });

    assert!(
        matches!(
            s.sim.phase(),
            Phase::Over {
                outcome: Outcome::Victory,
                ..
            }
        ),
        "the wave is cleared"
    );
    assert!(
        most_alive >= 150,
        "the field was crowded, at most {most_alive} alive"
    );
}

/// 몬스터가 `count`마리 마차 둘레에 모여 있을 때, Tick 1,000번의 실행 시간을 재서 (평균, 가장 느린 Tick)을 돌려줍니다.
fn tick_time_with(count: u32) -> (std::time::Duration, std::time::Duration) {
    let mut s = Scenario::with_waves(|t| {
        t.map = crate::tuning::MapSpec::stage1();
        t.waves.sizes = vec![count];
        t.waves.first_delay_secs = 0.05;
        t.waves.spawn_interval_secs = 0.05;
        t.wagon.max_hp = u32::MAX;
        t.structures.watchtower.max_hp = u32::MAX;
        t.structures.watchtower.damage = 0;
    });
    s.run_until(count + 100, |sim| sim.monsters().count() == count as usize);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), count as usize, "nobody died");

    let mut total = std::time::Duration::ZERO;
    let mut slowest = std::time::Duration::ZERO;
    for _ in 0..1000 {
        let started = std::time::Instant::now();
        s.run_ticks(1);
        let took = started.elapsed();
        total += took;
        slowest = slowest.max(took);
    }
    (total / 1000, slowest)
}

/// `cargo test -p caravan_rules --release tick_time_grows -- --ignored --nocapture`로 실행합니다.
/// 몬스터 수가 두 배가 되면 Tick 하나의 실행 시간도 두 배 가까이 늘어나는지(몬스터 수에 비례하는지) 봅니다.
#[test]
#[ignore = "benchmark: run in release mode with --ignored --nocapture"]
fn tick_time_grows_in_proportion_to_the_monster_count() {
    for count in [75, 150, 300, 600] {
        let (average, slowest) = tick_time_with(count);
        println!("{count} monsters: average {average:?} per tick, slowest {slowest:?}");
    }
}

#[test]
fn zz_probe_queue() {
    let mut s = Scenario::open_field();
    for row in 0..10 {
        for column in 0..10 {
            s.monster_at(6.0 + 0.5 * column as f32, 9.0 + 0.5 * row as f32);
        }
    }
    for round in 0..6 {
        s.run_ticks(s.ticks(5.0));
        let mut d: Vec<f32> = s
            .sim
            .monsters()
            .map(|m| s.sim.wagon().rect.distance_to(m.pos))
            .collect();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let (mut minx, mut maxx, mut miny, mut maxy) = (99.0f32, -99.0f32, 99.0f32, -99.0f32);
        for m in s.sim.monsters() {
            minx = minx.min(m.pos.x);
            maxx = maxx.max(m.pos.x);
            miny = miny.min(m.pos.y);
            maxy = maxy.max(m.pos.y);
        }
        eprintln!(
            "Q round={round} n={} d_min={:.1} d_med={:.1} d_max={:.1} bbox=({minx:.1},{miny:.1})-({maxx:.1},{maxy:.1})",
            d.len(),
            d[0],
            d[d.len() / 2],
            d[d.len() - 1]
        );
    }
}
