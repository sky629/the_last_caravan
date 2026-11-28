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
use crate::world::{Hit, Source, Target};

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

/// 종류마다의 수가 `packs`일 때 무리의 모든 순서를 종류의 첫 글자로 적습니다. 인원에 따라 줄이지 않으므로 수는 묶음에 적은 그대로입니다.
fn letters(packs: &[(MonsterKind, u32)]) -> String {
    let wave = wave_of(packs);
    (0..wave.size(WaveScaling::Fixed, 1))
        .map(|nth| {
            letter(
                wave.kind_at(WaveScaling::Fixed, 1, nth)
                    .expect("the wave still has monsters"),
            )
        })
        .collect()
}

fn grunts_and_runners() -> Wave {
    wave_of(&[(MonsterKind::Grunt, 20), (MonsterKind::Runner, 10)])
}

/// 기본 몬스터와 돌격병의 수가 `grunts`와 `runners`인 무리의 순서입니다.
fn grunts_then_runners(grunts: u32, runners: u32) -> String {
    letters(&[(MonsterKind::Grunt, grunts), (MonsterKind::Runner, runners)])
}

#[test]
fn a_mixed_wave_takes_the_kind_with_the_largest_share_left_and_the_earlier_kind_on_a_tie() {
    assert_eq!(
        grunts_then_runners(20, 10),
        "GRGGRGGRGGRGGRGGRGGRGGRGGRGGRG"
    );
    assert_eq!(grunts_then_runners(5, 3), "GRGRGGRG");
    assert_eq!(
        grunts_then_runners(1, 1),
        "GR",
        "a tie goes to the earlier kind"
    );
    assert_eq!(
        grunts_then_runners(3, 5),
        "GRRGRRGR",
        "the smaller share is not served last"
    );
    assert_eq!(
        letters(&[
            (MonsterKind::Grunt, 2),
            (MonsterKind::Runner, 1),
            (MonsterKind::Boss, 1),
        ]),
        "GRBG"
    );
}

#[test]
fn every_kind_of_a_mixed_wave_comes_out_exactly_as_many_times_as_its_size() {
    for (grunts, runners) in [(20, 10), (5, 3), (1, 1), (1, 6), (25, 15)] {
        let order = grunts_then_runners(grunts, runners);
        for (kind, size) in [('G', grunts), ('R', runners)] {
            let count = order.chars().filter(|&found| found == kind).count() as u32;
            assert_eq!(count, size, "{kind} in {grunts} and {runners}");
        }
    }
}

#[test]
fn a_wave_of_one_kind_comes_out_in_that_kind_until_its_size_and_then_ends() {
    let wave = wave_of(&[(MonsterKind::Grunt, 8)]);

    assert_eq!(letters(&[(MonsterKind::Grunt, 8)]), "GGGGGGGG");
    assert_eq!(
        wave.kind_at(WaveScaling::Fixed, 1, 8),
        None,
        "nothing after the last one"
    );
    assert_eq!(wave.kind_at(WaveScaling::Fixed, 1, 100), None);
}

/// 몬스터를 한 마리씩 내보내며 「나온 비율이 가장 작은 종류, 같으면 앞쪽」을 고르는 가장 단순한 계산입니다. `Wave::kind_at`이 같은 순서를 내는지 견주는 기준입니다.
fn replayed_order(sizes: &[u32]) -> Vec<usize> {
    let mut spawned = vec![0u32; sizes.len()];
    let mut order = Vec::new();
    while let Some(index) = (0..sizes.len())
        .filter(|&index| spawned[index] < sizes[index])
        .reduce(|best, index| {
            let (done, size) = (u64::from(spawned[index]), u64::from(sizes[index]));
            let (best_done, best_size) = (u64::from(spawned[best]), u64::from(sizes[best]));
            if done * best_size < best_done * size {
                index
            } else {
                best
            }
        })
    {
        spawned[index] += 1;
        order.push(index);
    }
    order
}

#[test]
fn the_order_of_a_wave_is_the_one_a_monster_by_monster_replay_gives_for_every_small_set_of_sizes() {
    // 종류가 하나에서 셋이고 종류마다 1마리에서 8마리인 모든 무리를 견줍니다. 순서는 종류를 이름표로만 쓰므로 종류는 앞에서부터 세 가지를 씁니다.
    let kinds = [
        MonsterKind::Grunt,
        MonsterKind::Runner,
        MonsterKind::Scarecrow,
    ];
    let mut checked = 0;
    for kind_count in 1..=kinds.len() {
        let mut sizes = vec![1u32; kind_count];
        loop {
            let packs: Vec<(MonsterKind, u32)> = kinds.iter().copied().zip(sizes.clone()).collect();
            let wave = wave_of(&packs);
            let expected = replayed_order(&sizes);
            let total = sizes.iter().sum::<u32>();
            for nth in 0..total {
                assert_eq!(
                    wave.kind_at(WaveScaling::Fixed, 1, nth),
                    Some(kinds[expected[nth as usize]]),
                    "sizes {sizes:?}, monster {nth}"
                );
            }
            assert_eq!(wave.kind_at(WaveScaling::Fixed, 1, total), None);
            checked += 1;

            // 다음 크기의 조합입니다. 자릿수가 8을 넘으면 올림합니다.
            let Some(digit) = sizes.iter().position(|&size| size < 8) else {
                break;
            };
            sizes[digit] += 1;
            sizes[..digit].fill(1);
        }
    }
    assert_eq!(checked, 8 + 64 + 512);
}

#[test]
fn a_wave_of_billions_gives_the_kind_of_any_place_without_counting_up_to_it() {
    // 두 종류가 20억 마리씩이면 번갈아 나오므로 짝수 번째는 기본 몬스터, 홀수 번째는 돌격병입니다. 한 마리씩 세어서는 끝나지 않는 크기입니다.
    let wave = wave_of(&[
        (MonsterKind::Grunt, 2_000_000_000),
        (MonsterKind::Runner, 2_000_000_000),
    ]);
    let at = |nth| wave.kind_at(WaveScaling::Fixed, 1, nth);

    assert_eq!(at(3_999_999_998), Some(MonsterKind::Grunt));
    assert_eq!(at(3_999_999_999), Some(MonsterKind::Runner));
    assert_eq!(at(4_000_000_000), None);
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
    // `START`는 마차의 가장자리에서 12타일 떨어진 자리이고, README가 이 두 값을 그대로 적습니다.
    assert_eq!(
        (
            ticks_to_the_wagon(MonsterKind::Runner),
            ticks_to_the_wagon(MonsterKind::Grunt)
        ),
        (72, 103),
        "ticks a runner and a grunt need to hit the wagon from the same start"
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

/// 몬스터 하나가 피해 `amount` 하나를 `by`가 준 것으로 받았을 때 잃은 HP입니다. 몬스터는 죽지 않게 HP를 크게 합니다.
fn hp_lost_to_one_hit(kind: MonsterKind, by: fn(PlayerId) -> Source, amount: u32) -> u32 {
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.runner.max_hp = 1000;
    });
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_of(kind, START.0, START.1);
    let t = s.sim.tuning().clone();

    s.sim.world_mut().apply_hits(
        &[Hit {
            target: Target::Monster(monster),
            amount,
            by: by(owner),
            debuff: None,
        }],
        &t,
    );

    1000 - s.monster(monster).expect("alive").hp
}

#[test]
fn a_tower_hit_takes_a_quarter_of_the_damage_off_a_runner_rounded_up_and_all_of_it_off_a_grunt() {
    let by_tower: fn(PlayerId) -> Source = |_| Source::Tower;

    assert_eq!(hp_lost_to_one_hit(MonsterKind::Runner, by_tower, 100), 25);
    assert_eq!(
        hp_lost_to_one_hit(MonsterKind::Runner, by_tower, 15),
        4,
        "a quarter of the watchtower's 15 is 3.75, rounded up to 4"
    );
    assert_eq!(hp_lost_to_one_hit(MonsterKind::Runner, by_tower, 1), 1);
    assert_eq!(hp_lost_to_one_hit(MonsterKind::Grunt, by_tower, 100), 100);
    assert_eq!(hp_lost_to_one_hit(MonsterKind::Grunt, by_tower, 15), 15);
}

#[test]
fn a_player_hit_takes_all_of_the_damage_off_a_runner() {
    assert_eq!(
        hp_lost_to_one_hit(MonsterKind::Runner, Source::Player, 100),
        100
    );
    assert_eq!(
        hp_lost_to_one_hit(MonsterKind::Runner, Source::Player, 15),
        15
    );
}

#[test]
fn a_watchtower_shot_takes_four_hp_off_a_runner_and_fifteen_off_a_grunt() {
    let hp_lost_to_one_shot = |kind| {
        let mut s = Scenario::with_tuning(|t| {
            t.monsters.grunt.max_hp = 1000;
            t.monsters.runner.max_hp = 1000;
        });
        s.tower_at(1, 17);
        let monster = s.monster_of(kind, 4.0, 17.5);

        s.run_ticks(1);

        1000 - s.monster(monster).expect("alive").hp
    };

    assert_eq!(hp_lost_to_one_shot(MonsterKind::Runner), 4);
    assert_eq!(hp_lost_to_one_shot(MonsterKind::Grunt), 15);
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
