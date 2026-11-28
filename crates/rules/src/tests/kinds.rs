//! 몬스터의 종류를 확인합니다: 한 무리 안에서 섞여 나오는 종류의 순서와 수.
//!
//! 순서를 정하는 함수(`Wave::kind_at`)는 종류를 이름표로만 쓰므로, 순서의 시험은 글자 하나로 종류를 나타냅니다.
//! `G`는 기본 몬스터(`Grunt`), `S`는 허수아비(`Scarecrow`), `B`는 보스(`Boss`)이고, 순서를 시험하는 곳에서는 이름표일 뿐입니다.

use super::support::{Scenario, phase};
use crate::expedition::{NightStep, Phase};
use crate::kind::MonsterKind;
use crate::tuning::{NightPlan, Pack, Wave, WaveScaling};

fn wave_of(packs: &[(MonsterKind, u32)]) -> Wave {
    Wave {
        packs: packs
            .iter()
            .map(|&(kind, count)| Pack { kind, count })
            .collect(),
    }
}

/// 종류마다의 수가 `sizes`일 때 무리의 모든 순서를 종류의 첫 글자로 적습니다.
fn letters(wave: &Wave, sizes: &[u32]) -> String {
    (0..sizes.iter().sum())
        .map(|nth| {
            let kind = wave
                .kind_at(sizes, nth)
                .expect("the wave still has monsters");
            format!("{kind:?}").chars().next().expect("a name")
        })
        .collect()
}

fn two_kinds() -> Wave {
    wave_of(&[(MonsterKind::Grunt, 20), (MonsterKind::Scarecrow, 10)])
}

#[test]
fn a_mixed_wave_takes_the_kind_with_the_largest_share_left_and_the_earlier_kind_on_a_tie() {
    let wave = two_kinds();
    assert_eq!(letters(&wave, &[20, 10]), "GSGGSGGSGGSGGSGGSGGSGGSGGSGGSG");
    assert_eq!(letters(&wave, &[5, 3]), "GSGSGGSG");
    assert_eq!(
        letters(&wave, &[1, 1]),
        "GS",
        "a tie goes to the earlier kind"
    );
    assert_eq!(
        letters(&wave, &[3, 5]),
        "GSSGSSGS",
        "the smaller share is not served last"
    );
    let three = wave_of(&[
        (MonsterKind::Grunt, 2),
        (MonsterKind::Scarecrow, 1),
        (MonsterKind::Boss, 1),
    ]);
    assert_eq!(letters(&three, &[2, 1, 1]), "GSBG");
}

#[test]
fn every_kind_of_a_mixed_wave_comes_out_exactly_as_many_times_as_its_size() {
    let wave = two_kinds();
    for sizes in [[20, 10], [5, 3], [1, 1], [1, 6], [25, 15]] {
        let order = letters(&wave, &sizes);
        assert_eq!(
            order.chars().filter(|&kind| kind == 'G').count() as u32,
            sizes[0],
            "{sizes:?}"
        );
        assert_eq!(
            order.chars().filter(|&kind| kind == 'S').count() as u32,
            sizes[1],
            "{sizes:?}"
        );
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
fn night_of(wave: Wave) -> crate::tuning::NightTuning {
    let mut nights = crate::tuning::Tuning::default().nights;
    nights.plans = vec![NightPlan { waves: vec![wave] }];
    nights.scaling = WaveScaling::ByParty { full_party: 4 };
    nights
}

#[test]
fn each_kind_of_a_wave_is_scaled_by_the_party_on_its_own_and_the_wave_is_their_sum() {
    let nights = night_of(two_kinds());
    for (party, grunts, others) in [(1, 5, 3), (2, 10, 5), (3, 15, 8), (4, 20, 10)] {
        let total = nights.wave_size(1, 1, party);
        assert_eq!(total, grunts + others, "party {party}");
        let order: Vec<_> = (0..total)
            .map(|nth| nights.kind_at(1, 1, party, nth).expect("inside the wave"))
            .collect();
        let count = |kind| order.iter().filter(|&&found| found == kind).count() as u32;
        assert_eq!(count(MonsterKind::Grunt), grunts, "party {party}");
        assert_eq!(count(MonsterKind::Scarecrow), others, "party {party}");
        assert_eq!(nights.kind_at(1, 1, party, total), None, "party {party}");
    }

    let rounded = night_of(wave_of(&[
        (MonsterKind::Grunt, 21),
        (MonsterKind::Scarecrow, 3),
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
        (MonsterKind::Scarecrow, 1),
    ]));
    for party in 1..=4 {
        let total = nights.wave_size(1, 1, party);
        let has_the_small_kind =
            (0..total).any(|nth| nights.kind_at(1, 1, party, nth) == Some(MonsterKind::Scarecrow));
        assert!(has_the_small_kind, "party {party}");
    }
}

#[test]
fn a_wave_of_two_packs_spawns_as_many_monsters_as_the_sum_of_their_scaled_sizes() {
    // 기준 인원일 때 21과 3이고 한 명이면 6과 1이므로 7마리입니다. 두 묶음이 같은 종류여도 묶음마다 올림합니다.
    let packs = wave_of(&[(MonsterKind::Grunt, 21), (MonsterKind::Grunt, 3)]);
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.05;
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.nights.plans = vec![NightPlan { waves: vec![packs] }];
        t.nights.first_wave_delay_secs = 0.05;
        t.nights.spawn_interval_secs = 0.05;
    });
    s.player_at(1.0, 1.0);

    s.run_until(400, |sim| {
        matches!(
            phase(sim),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });

    assert_eq!(s.sim.monsters().count(), 7);
}
