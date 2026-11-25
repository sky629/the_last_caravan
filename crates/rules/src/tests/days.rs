//! 날의 흐름을 확인합니다: 낮과 밤, 밤의 무리와 등장하는 자리, 새벽에 낮이 시작될 때 이어지는 것과 회복되는 것, 성공과 새 원정.

use super::support::{Scenario, nights};
use crate::expedition::{NightStep, Outcome, Phase};
use crate::tuning::Tuning;

/// 밤의 무리가 모두 나타날 때까지 진행합니다.
fn run_until_fully_spawned(s: &mut Scenario) {
    s.run_until(2000, |sim| {
        matches!(
            sim.phase(),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });
}

/// 한 무리가 모두 나타난 뒤 그 몬스터를 모두 처치한 상황을 만들고, 한 Tick 진행합니다.
fn defeat_the_current_wave(s: &mut Scenario) {
    run_until_fully_spawned(s);
    s.clear_monsters();
    s.run_ticks(1);
}

/// 낮이 짧고 밤의 구성이 `counts`인 시나리오입니다.
fn short_days(counts: &[&[u32]]) -> Scenario {
    let plans = nights(counts);
    Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.nights.plans = plans;
    })
}

fn monster_count(s: &Scenario) -> usize {
    s.sim.monsters().count()
}

#[test]
fn the_expedition_starts_on_the_first_day() {
    let s = Scenario::default_game(|_| {});

    assert_eq!(
        s.sim.phase(),
        Phase::Day {
            day: 1,
            ticks_left: s.ticks(90.0)
        }
    );
    assert_eq!(s.sim.tuning().day_count(), 3, "one night for each day");
}

#[test]
fn the_first_night_begins_when_the_day_time_is_over() {
    let mut s = Scenario::with_cycle(|t| t.cycle.day_secs = 2.0);
    let day = s.ticks(2.0);

    s.run_ticks(day - 1);
    assert_eq!(
        s.sim.phase(),
        Phase::Day {
            day: 1,
            ticks_left: 1
        }
    );

    s.run_ticks(1);
    assert!(matches!(
        s.sim.phase(),
        Phase::Night {
            day: 1,
            step: NightStep::Waiting { next_wave: 1, .. }
        }
    ));
}

#[test]
fn the_first_wave_appears_after_the_delay_one_monster_at_a_time() {
    let mut s = short_days(&[&[6]]);
    s.run_until(100, |sim| matches!(sim.phase(), Phase::Night { .. }));
    let delay = s.ticks(s.sim.tuning().nights.first_wave_delay_secs);
    let interval = s.ticks(s.sim.tuning().nights.spawn_interval_secs);

    s.run_ticks(delay - 1);
    assert_eq!(monster_count(&s), 0, "nothing before the delay");

    s.run_ticks(1);
    assert_eq!(monster_count(&s), 1, "the stream starts with one monster");
    s.run_ticks(interval - 1);
    assert_eq!(monster_count(&s), 1);
    s.run_ticks(1);
    assert_eq!(monster_count(&s), 2);

    s.run_ticks(interval * 10);
    assert_eq!(monster_count(&s), 6, "the first wave has six monsters");
}

#[test]
fn a_wave_is_not_cleared_while_monsters_are_still_to_come() {
    let mut s = short_days(&[&[3]]);
    s.run_until(400, |sim| sim.monsters().count() == 1);

    s.clear_monsters();
    s.run_ticks(1);

    assert!(matches!(
        s.sim.phase(),
        Phase::Night {
            step: NightStep::Fighting { wave: 1, .. },
            ..
        }
    ));
}

#[test]
fn the_next_wave_comes_after_a_pause_once_the_wave_is_cleared() {
    let mut s = short_days(&[&[2, 4]]);
    defeat_the_current_wave(&mut s);

    let pause = s.ticks(s.sim.tuning().nights.between_waves_secs);
    assert!(matches!(
        s.sim.phase(),
        Phase::Night {
            day: 1,
            step: NightStep::Waiting { next_wave: 2, .. }
        }
    ));
    s.run_ticks(pause - 2);
    assert_eq!(monster_count(&s), 0, "still waiting");

    s.run_ticks(2);
    assert!(matches!(
        s.sim.phase(),
        Phase::Night {
            step: NightStep::Fighting { wave: 2, .. },
            ..
        }
    ));
    run_until_fully_spawned(&mut s);
    assert_eq!(monster_count(&s), 4, "the wave sizes come from the tuning");
}

#[test]
fn clearing_the_last_wave_of_a_night_brings_the_dawn_and_then_the_next_day() {
    let mut s = short_days(&[&[1], &[1]]);

    defeat_the_current_wave(&mut s);

    let dawn = s.ticks(s.sim.tuning().cycle.dawn_secs);
    assert_eq!(
        s.sim.phase(),
        Phase::Dawn {
            next_day: 2,
            ticks_left: dawn
        }
    );
    s.run_ticks(dawn - 1);
    assert!(matches!(s.sim.phase(), Phase::Dawn { next_day: 2, .. }));

    s.run_ticks(1);
    assert_eq!(
        s.sim.phase(),
        Phase::Day {
            day: 2,
            ticks_left: s.ticks(0.5)
        }
    );
}

#[test]
fn the_next_day_keeps_the_village_and_restores_every_player() {
    let mut s = short_days(&[&[1], &[1]]);
    let hurt = s.player_at(3.0, 3.0);
    let downed = s.player_at(4.0, 4.0);
    let dead = s.player_at(5.0, 5.0);
    let hurt_life = crate::player::Life::alive(40, s.sim.tuning());
    s.sim.world_mut().players.get_mut(&hurt).unwrap().life = hurt_life;
    s.knock_down(downed);
    s.kill(dead);
    let tower = s.tower_at(8, 8);
    s.set_tower_hp(120);
    s.set_stash(7);
    s.set_wagon_hp(300);
    let positions: Vec<_> = [hurt, downed, dead].map(|id| s.player(id).pos).to_vec();

    defeat_the_current_wave(&mut s);
    s.run_until(400, |sim| matches!(sim.phase(), Phase::Day { day: 2, .. }));

    for (id, pos) in [hurt, downed, dead].into_iter().zip(positions) {
        assert!(s.player(id).is_alive(), "player {id:?} is up again");
        assert_eq!(s.player(id).hp(), 100);
        assert_eq!(s.player(id).pos, pos, "nobody is moved");
    }
    let kept = s.sim.towers().next().expect("the tower carries over");
    assert_eq!((kept.id, kept.hp()), (tower, 120));
    assert_eq!(s.sim.stash(), 7);
    assert_eq!(s.sim.wagon().hp, 300, "the wagon does not heal by itself");
}

#[test]
fn clearing_every_night_is_a_victory_and_a_new_expedition_starts_on_the_first_day() {
    let mut s = short_days(&[&[1], &[1]]);
    s.player_at(2.0, 2.0);
    defeat_the_current_wave(&mut s);
    s.run_until(400, |sim| matches!(sim.phase(), Phase::Day { day: 2, .. }));
    assert!(
        !matches!(s.sim.phase(), Phase::Over { .. }),
        "a night is left"
    );

    defeat_the_current_wave(&mut s);

    let restart = s.ticks(s.sim.tuning().cycle.restart_secs);
    assert!(
        matches!(
            s.sim.phase(),
            Phase::Over {
                outcome: Outcome::Victory,
                ..
            }
        ),
        "got {:?}",
        s.sim.phase()
    );
    s.run_ticks(restart - 2);
    assert_eq!(
        s.sim.expedition_number(),
        1,
        "the result is still on the screen"
    );
    s.run_ticks(2);
    assert_eq!(s.sim.expedition_number(), 2);
    assert!(matches!(s.sim.phase(), Phase::Day { day: 1, .. }));
}

#[test]
fn a_failure_by_day_starts_a_new_expedition_on_the_first_day() {
    let mut s = short_days(&[&[1], &[1]]);
    s.player_at(2.0, 2.0);
    defeat_the_current_wave(&mut s);
    s.run_until(400, |sim| matches!(sim.phase(), Phase::Day { day: 2, .. }));

    s.set_wagon_hp(0);
    s.run_ticks(1);
    assert!(matches!(
        s.sim.phase(),
        Phase::Over {
            outcome: Outcome::Defeat,
            ..
        }
    ));

    s.run_ticks(s.ticks(s.sim.tuning().cycle.restart_secs) + 1);
    assert_eq!(s.sim.expedition_number(), 2);
    assert!(matches!(s.sim.phase(), Phase::Day { day: 1, .. }));
}

#[test]
fn night_monsters_appear_at_the_spawn_distance_from_the_wagon() {
    let mut s = Scenario::default_game(|t| {
        t.cycle.day_secs = 0.05;
        t.nights.plans = nights(&[&[10]]);
        t.guards.per_node = 0;
        t.loot.chests_per_day = 0;
    });
    let wagon = s.sim.wagon().rect;
    let distance = s.sim.tuning().nights.spawn_distance;
    let mut seen = 0;

    while seen < 10 {
        s.run_until(200, |sim| sim.monsters().count() > seen);
        seen += 1;
        let newest = s.sim.monsters().max_by_key(|monster| monster.id).unwrap();
        let away = wagon.distance_to(newest.pos);
        assert!(
            (away - distance).abs() < 1.0,
            "{away} tiles from the wagon, wanted about {distance}"
        );
    }
}

#[test]
fn every_wave_comes_from_another_direction_and_all_eight_are_used() {
    let mut s = Scenario::default_game(|t| {
        t.cycle.day_secs = 0.05;
        t.nights.plans = nights(&[&[1, 1, 1, 1, 1, 1, 1, 1]]);
        t.nights.first_wave_delay_secs = 0.05;
        t.nights.between_waves_secs = 0.05;
        t.guards.per_node = 0;
        t.loot.chests_per_day = 0;
    });
    let center = s.sim.wagon().rect.center();
    let mut compass = Vec::new();
    for _ in 0..8 {
        s.run_until(400, |sim| sim.monsters().count() == 1);
        let pos = s.sim.monsters().next().unwrap().pos;
        let angle = (pos.y - center.y).atan2(pos.x - center.x);
        compass.push((angle / std::f32::consts::FRAC_PI_4).round() as i32);
        s.clear_monsters();
        s.run_ticks(1);
    }

    let eighths: Vec<i32> = compass.iter().map(|step| step.rem_euclid(8)).collect();
    let mut distinct = eighths.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 8, "all eight directions, got {eighths:?}");
    assert!(
        eighths
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).rem_euclid(8) == 3),
        "the next wave comes from 135 degrees away, got {eighths:?}"
    );
}

#[test]
fn the_alive_cap_pauses_the_spawning_until_monsters_are_gone() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.05;
        t.nights.plans = nights(&[&[12]]);
        t.nights.first_wave_delay_secs = 0.05;
        t.nights.spawn_interval_secs = 0.05;
        t.nights.max_alive = 5;
        t.wagon.max_hp = u32::MAX;
    });

    for _ in 0..200 {
        s.run_ticks(1);
        assert!(monster_count(&s) <= 5, "the cap holds");
    }
    assert_eq!(monster_count(&s), 5);
    assert!(
        matches!(
            s.sim.phase(),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 7, .. },
                ..
            }
        ),
        "the rest waits, got {:?}",
        s.sim.phase()
    );

    s.clear_monsters();
    s.run_ticks(100);
    assert_eq!(
        monster_count(&s),
        5,
        "five more came after the field emptied"
    );
}

#[test]
fn every_night_is_larger_than_the_one_before_and_the_last_has_hundreds() {
    let t = Tuning::default();
    let totals: Vec<u32> = t
        .nights
        .plans
        .iter()
        .map(|night| night.waves.iter().map(|wave| wave.count).sum())
        .collect();

    assert_eq!(totals.len(), 3);
    assert!(
        totals.windows(2).all(|pair| pair[0] < pair[1]),
        "{totals:?}"
    );
    assert!(totals[2] >= 300, "{totals:?}");
}
