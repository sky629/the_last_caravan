//! 몬스터의 종류를 확인합니다: 한 무리 안에서 섞여 나오는 종류의 순서와 수, 돌격병의 행동과 수치, 기본 수치의 밤마다의 구성, 보스의 졸개.
//!
//! 순서를 정하는 함수(`Wave::kind_at`)는 종류를 이름표로만 쓰므로, 순서를 적을 때는 종류의 첫 글자 하나로 나타냅니다.
//! `G`는 기본 몬스터(`Grunt`)이고 `R`은 돌격병(`Runner`)입니다. 세 종류의 순서를 시험하는 곳에서만 `B`(`Boss`)가 이름표로 쓰입니다.

use super::support::{Scenario, close, phase, watchtower_shot};
use crate::expedition::{NightStep, Phase};
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, PlayerInput};
use crate::kind::MonsterKind;
use crate::tuning::{
    Debuff, Finale, NightPlan, NightTuning, Pack, Prey, Tuning, Wave, WaveScaling,
};

fn wave_of(packs: &[(MonsterKind, u32)]) -> Wave {
    Wave {
        packs: packs
            .iter()
            .map(|&(kind, count)| Pack { kind, count })
            .collect(),
    }
}

fn letter(kind: MonsterKind) -> char {
    format!("{kind:?}").chars().next().expect("a name")
}

/// 종류마다의 수가 `sizes`일 때 무리의 모든 순서를 종류의 첫 글자로 적습니다.
fn letters(wave: &Wave, sizes: &[u32]) -> String {
    (0..sizes.iter().sum())
        .map(|nth| {
            letter(
                wave.kind_at(sizes, nth)
                    .expect("the wave still has monsters"),
            )
        })
        .collect()
}

fn grunts_and_runners() -> Wave {
    wave_of(&[(MonsterKind::Grunt, 20), (MonsterKind::Runner, 10)])
}

#[test]
fn a_mixed_wave_takes_the_kind_with_the_largest_share_left_and_the_earlier_kind_on_a_tie() {
    let wave = grunts_and_runners();
    assert_eq!(letters(&wave, &[20, 10]), "GRGGRGGRGGRGGRGGRGGRGGRGGRGGRG");
    assert_eq!(letters(&wave, &[5, 3]), "GRGRGGRG");
    assert_eq!(
        letters(&wave, &[1, 1]),
        "GR",
        "a tie goes to the earlier kind"
    );
    assert_eq!(
        letters(&wave, &[3, 5]),
        "GRRGRRGR",
        "the smaller share is not served last"
    );
    let three = wave_of(&[
        (MonsterKind::Grunt, 2),
        (MonsterKind::Runner, 1),
        (MonsterKind::Boss, 1),
    ]);
    assert_eq!(letters(&three, &[2, 1, 1]), "GRBG");
}

#[test]
fn every_kind_of_a_mixed_wave_comes_out_exactly_as_many_times_as_its_size() {
    let wave = grunts_and_runners();
    for sizes in [[20, 10], [5, 3], [1, 1], [1, 6], [25, 15]] {
        let order = letters(&wave, &sizes);
        for (kind, size) in [('G', sizes[0]), ('R', sizes[1])] {
            let count = order.chars().filter(|&found| found == kind).count() as u32;
            assert_eq!(count, size, "{kind} in {sizes:?}");
        }
    }
}

#[test]
fn a_wave_of_one_kind_comes_out_in_that_kind_until_its_size_and_then_ends() {
    let wave = wave_of(&[(MonsterKind::Grunt, 8)]);

    assert_eq!(letters(&wave, &[8]), "GGGGGGGG");
    assert_eq!(wave.kind_at(&[8], 8), None, "nothing after the last one");
    assert_eq!(wave.kind_at(&[8], 100), None);
}

/// 밤이 하나이고 무리가 `wave` 하나이며 무리가 인원에 비례하는 수치입니다.
fn night_of(wave: Wave) -> NightTuning {
    let mut nights = Tuning::default().nights;
    nights.plans = vec![NightPlan { waves: vec![wave] }];
    nights.scaling = WaveScaling::ByParty { full_party: 4 };
    nights
}

/// 방에 `party`명이 있을 때 `day`번째 밤의 `wave`번째 무리의 모든 순서입니다.
fn order_of(nights: &NightTuning, day: u32, wave: u32, party: u32) -> Vec<MonsterKind> {
    (0..nights.wave_size(day, wave, party))
        .map(|nth| {
            nights
                .kind_at(day, wave, party, nth)
                .expect("inside the wave")
        })
        .collect()
}

fn count_of(order: &[MonsterKind], kind: MonsterKind) -> u32 {
    order.iter().filter(|&&found| found == kind).count() as u32
}

#[test]
fn each_kind_of_a_wave_is_scaled_by_the_party_on_its_own_and_the_wave_is_their_sum() {
    let nights = night_of(grunts_and_runners());
    for (party, grunts, runners) in [(1, 5, 3), (2, 10, 5), (3, 15, 8), (4, 20, 10)] {
        let order = order_of(&nights, 1, 1, party);
        assert_eq!(order.len() as u32, grunts + runners, "party {party}");
        assert_eq!(
            count_of(&order, MonsterKind::Grunt),
            grunts,
            "party {party}"
        );
        assert_eq!(
            count_of(&order, MonsterKind::Runner),
            runners,
            "party {party}"
        );
        assert_eq!(
            nights.kind_at(1, 1, party, order.len() as u32),
            None,
            "party {party}"
        );
    }

    let rounded = night_of(wave_of(&[
        (MonsterKind::Grunt, 21),
        (MonsterKind::Runner, 3),
    ]));
    assert_eq!(
        rounded.wave_size(1, 1, 1),
        7,
        "each kind rounds up on its own: 6 and 1, not 6 for the 24 together"
    );
}

#[test]
fn every_kind_of_a_wave_comes_out_at_least_once_whatever_the_party() {
    let nights = night_of(wave_of(&[
        (MonsterKind::Grunt, 40),
        (MonsterKind::Runner, 1),
    ]));
    for party in 1..=4 {
        let order = order_of(&nights, 1, 1, party);
        assert!(count_of(&order, MonsterKind::Runner) >= 1, "party {party}");
    }
}

/// 방에 `party`명이 있을 때 `wave` 하나로 이루어진 밤이 실제로 몬스터를 나타낸 순서입니다. 나타난 몬스터를 번호 순서로 읽어 첫 글자로 적습니다.
fn spawned_order(wave: Wave, party: u32) -> String {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.05;
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.nights.plans = vec![NightPlan { waves: vec![wave] }];
        t.nights.first_wave_delay_secs = 0.05;
        t.nights.spawn_interval_secs = 0.05;
        t.wagon.max_hp = u32::MAX;
        t.player.max_hp = u32::MAX;
    });
    for _ in 0..party {
        s.player_at(1.0, 1.0);
    }
    s.run_until(600, |sim| {
        matches!(
            phase(sim),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });
    let mut monsters: Vec<_> = s.sim.monsters().collect();
    monsters.sort_by_key(|monster| monster.id);
    monsters
        .iter()
        .map(|monster| letter(monster.kind))
        .collect()
}

#[test]
fn a_wave_of_grunts_and_runners_spawns_in_the_mixed_order_for_one_two_and_four_players() {
    for (party, expected) in [
        (1, "GRGRGGRG"),
        (2, "GRGGRGGRGGRGGRG"),
        (4, "GRGGRGGRGGRGGRGGRGGRGGRGGRGGRG"),
    ] {
        assert_eq!(
            spawned_order(grunts_and_runners(), party),
            expected,
            "{party} players"
        );
    }
}

#[test]
fn a_wave_of_two_packs_spawns_as_many_monsters_as_the_sum_of_their_scaled_sizes() {
    // 기준 인원일 때 21과 3이고 한 명이면 6과 1이므로 7마리입니다. 두 묶음이 같은 종류여도 묶음마다 올림합니다.
    let packs = wave_of(&[(MonsterKind::Grunt, 21), (MonsterKind::Grunt, 3)]);

    assert_eq!(spawned_order(packs, 1).len(), 7);
}

/// 마차가 있는 오른쪽 위로 향하는 길입니다. 돌격병과 기본 몬스터는 이 길의 시작점에 놓입니다.
const START: (f32, f32) = (5.0, 18.0);

/// `START`에 돌격병 하나가 서 있고 멀리 플레이어가 한 명 있는 들판입니다. 돌격병은 죽지 않게 HP를 크게 합니다.
fn runner_field(adjust: impl FnOnce(&mut Tuning)) -> (Scenario, MonsterId, PlayerId) {
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.runner.max_hp = 1000;
        adjust(t);
    });
    let owner = s.player_at(2.0, 2.0);
    let runner = s.monster_of(MonsterKind::Runner, START.0, START.1);
    (s, runner, owner)
}

/// 몬스터가 한 Tick에 걷는 거리입니다. 이 함수가 한 Tick을 진행합니다.
fn walked_in_a_tick(s: &mut Scenario, id: MonsterId) -> f32 {
    let before = s.monster(id).unwrap().pos;
    s.run_ticks(1);
    s.monster(id).unwrap().pos.distance(before)
}

#[test]
fn a_runner_walks_past_a_player_and_a_tower_beside_its_way_and_hits_the_wagon() {
    // 걷는 길에 선 플레이어와 감시탑을 지나 마차를 때립니다. 감시탑은 사격하지만 피해를 주지 않습니다.
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.runner.max_hp = 1000;
        watchtower_shot(t).damage = 0;
    });
    let player = s.player_at(11.0, 17.0);
    s.tower_at(11, 19);
    s.monster_of(MonsterKind::Runner, START.0, START.1);

    s.run_until(200, |sim| sim.wagon().hp < 500);

    let t = s.sim.tuning().clone();
    assert_eq!(s.sim.wagon().hp, 500 - t.monsters.runner.attack_damage);
    assert_eq!(s.player(player).hp(), t.player.max_hp);
    assert_eq!(s.tower().hp(), t.structures.watchtower.max_hp);
}

#[test]
fn a_runner_that_a_player_hits_does_not_turn_on_the_player_and_goes_on_to_the_wagon() {
    let (mut s, runner, _) = runner_field(|_| {});
    let swordsman = s.player_at(11.0, 18.0);
    s.hold(
        swordsman,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    );
    s.run_until(100, |sim| {
        sim.monsters()
            .next()
            .is_some_and(|monster| monster.hp < 1000)
    });
    s.hold(swordsman, PlayerInput::default());
    let hurt = s.monster(runner).unwrap().hp;
    assert!(hurt < 1000, "the sword landed");

    s.run_until(200, |sim| sim.wagon().hp < 500);

    assert_eq!(
        s.player(swordsman).hp(),
        s.sim.tuning().player.max_hp,
        "the runner never hit the player who hit it"
    );
    assert!(
        s.monster(runner).unwrap().pos.x > 11.0,
        "it went past the player to the wagon"
    );
}

#[test]
fn a_runner_is_faster_than_a_grunt_and_slower_than_a_walking_player_and_has_less_hp() {
    let t = Tuning::default();
    assert!(t.monsters.runner.speed > t.monsters.grunt.speed);
    assert!(t.monsters.runner.speed < t.player.speed);
    assert!(t.monsters.runner.max_hp < t.monsters.grunt.max_hp);

    let ticks_to_the_wagon = |kind| {
        let mut s = Scenario::with_tuning(|_| {});
        s.monster_of(kind, START.0, START.1);
        s.run_until(400, |sim| sim.wagon().hp < 500)
    };
    let (runner, grunt) = (
        ticks_to_the_wagon(MonsterKind::Runner),
        ticks_to_the_wagon(MonsterKind::Grunt),
    );
    assert!(
        runner < grunt,
        "the runner needed {runner} ticks and the grunt {grunt}"
    );
}

#[test]
fn a_runner_adds_nothing_to_the_largest_body_or_the_longest_reach_the_rules_read() {
    let monsters = Tuning::default().monsters;
    let (runner, grunt) = (&monsters.runner, &monsters.grunt);

    assert!(runner.radius <= grunt.radius);
    assert!(runner.radius + runner.attack_reach <= grunt.radius + grunt.attack_reach);
    assert_eq!(monsters.largest_radius(), grunt.radius);
    assert_eq!(
        monsters.longest_engage_distance(),
        grunt.radius + grunt.attack_reach
    );
    assert_eq!(runner.prey, Prey::Wagon);
    assert_eq!(grunt.prey, Prey::Nearby);
}

#[test]
fn a_slowed_runner_walks_at_the_slowed_speed_for_the_time_of_the_slow() {
    let (mut s, runner, owner) = runner_field(|_| {});
    let full = walked_in_a_tick(&mut s, runner);
    assert!(
        close(full, 3.2 * 0.05),
        "3.2 tiles a second at 20 Hz: {full}"
    );

    s.afflict(
        runner,
        Debuff::Slow {
            factor: 0.5,
            secs: 2.0,
        },
        owner,
    );

    for tick in 1..=s.ticks(2.0) {
        let walked = walked_in_a_tick(&mut s, runner);
        assert!(
            close(walked, full * 0.5),
            "tick {tick} of the slow: {walked}"
        );
    }
    assert!(close(walked_in_a_tick(&mut s, runner), full));
}

#[test]
fn a_stunned_runner_stands_still_for_the_time_of_the_stun() {
    let (mut s, runner, owner) = runner_field(|_| {});
    let from = s.monster(runner).unwrap().pos;

    s.afflict(runner, Debuff::Stun { secs: 0.8 }, owner);
    s.run_ticks(s.ticks(0.8));

    assert_eq!(s.monster(runner).unwrap().pos, from);
    s.run_ticks(1);
    assert_ne!(s.monster(runner).unwrap().pos, from, "it walks again");
}

#[test]
fn a_bleeding_runner_loses_hp_once_a_second() {
    let (mut s, runner, owner) = runner_field(|t| {
        t.monsters.runner.speed = 0.0;
        t.monsters.runner.separation_speed = 0.0;
    });
    let full = s.monster(runner).unwrap().hp;

    s.afflict(
        runner,
        Debuff::Bleed {
            damage: 5,
            secs: 3.0,
        },
        owner,
    );
    s.run_ticks(s.ticks(1.0) - 1);
    assert_eq!(s.monster(runner).unwrap().hp, full, "not yet");
    s.run_ticks(1);
    assert_eq!(s.monster(runner).unwrap().hp, full - 5);
}

#[test]
fn the_first_night_has_only_grunts_and_the_second_and_third_mix_in_runners_with_the_same_totals() {
    let nights = Tuning::default().nights;
    let composition = |day, wave| {
        let order = order_of(&nights, day, wave, 4);
        (
            count_of(&order, MonsterKind::Grunt),
            count_of(&order, MonsterKind::Runner),
        )
    };

    assert_eq!([composition(1, 1), composition(1, 2)], [(8, 0), (12, 0)]);
    assert_eq!(
        [composition(2, 1), composition(2, 2), composition(2, 3)],
        [(20, 0), (20, 10), (25, 15)]
    );
    assert_eq!(
        [composition(3, 1), composition(3, 2), composition(3, 3)],
        [(40, 20), (70, 20), (110, 40)]
    );
    let total = |day| -> u32 {
        (1..=nights.wave_count(day))
            .map(|wave| nights.wave_size(day, wave, 4))
            .sum()
    };
    assert_eq!([total(1), total(2), total(3)], [20, 90, 300]);
}

#[test]
fn the_boss_calls_runners_in_proportion_to_the_party() {
    let t = Tuning::default();
    let Finale::Boss(boss) = &t.nights.finale else {
        panic!("the default tuning has a boss");
    };
    assert_eq!(boss.summon.kind, MonsterKind::Runner);
    assert_eq!(
        [1, 2, 3, 4].map(|party| t.nights.scaled(boss.summon.count, party)),
        [3, 6, 9, 12]
    );
}
