//! 보스의 차례와 졸개 부르기: 내려찍기, 돌진, 내려찍기, 졸개 부르기를 되풀이하는 것, 행동 사이에 걷는 시간,
//! HP가 절반 이하일 때 줄어드는 걷는 시간, 포효 뒤에 인원에 비례해서 나타나는 졸개, 보스가 쓰러진 뒤에도 남는 졸개.

use super::boss::{act, boss_tuning, hit_boss, put_boss, with_boss};
use super::support::{Scenario, nights, outcome};
use crate::boss::Act;
use crate::expedition::Outcome;
use crate::kind::MonsterKind;
use crate::tuning::{Move, Tuning, WaveScaling};

/// 보스가 마차에서 8타일 안에 서 있어서 쉬는 시간이 지나면 바로 행동을 시작하는 들판입니다. 마차는 죽지 않습니다.
fn performing_field(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    Scenario::with_cycle(|t| {
        with_boss(t);
        t.monsters.boss.speed = 0.0;
        t.monsters.grunt.speed = 0.0;
        t.nights.plans = nights(&[&[1]]);
        t.wagon.max_hp = u32::MAX;
        adjust(t);
    })
}

/// 행동의 종류를 이름으로 줄입니다. 순서를 비교할 때 씁니다.
fn name_of(act: &Act) -> &'static str {
    match act {
        Act::Walking { .. } => "walk",
        Act::Slamming(_) => "slam",
        Act::Winding { .. } | Act::Charging(_) => "charge",
        Act::Roaring { .. } => "summon",
    }
}

/// 보스를 `ticks`만큼 진행하면서 걷기에서 행동으로, 행동에서 걷기로 바뀐 Tick을 모읍니다. 돌려주는 것은 `(행동, 시작한 Tick, 끝난 Tick)`입니다.
fn performances(s: &mut Scenario, ticks: u32) -> Vec<(&'static str, u32, u32)> {
    let mut found: Vec<(&'static str, u32, u32)> = Vec::new();
    let mut current: Option<(&'static str, u32)> = None;
    for tick in 0..ticks {
        s.run_ticks(1);
        let Some(boss) = s.sim.boss() else {
            break;
        };
        let name = name_of(&boss.act);
        match (current, name) {
            (None, "walk") => {}
            (None, name) => current = Some((name, tick)),
            (Some((_, started)), "walk") => {
                let (done, _) = current.take().expect("an act");
                found.push((done, started, tick));
            }
            (Some(_), _) => {}
        }
    }
    found
}

/// 보스가 서 있는 들판에서 보스의 HP를 `hp`로 정합니다.
fn set_boss_hp(s: &mut Scenario, hp: u32) {
    let id = s.sim.boss().expect("a boss").id;
    s.sim
        .world_mut()
        .monsters
        .iter_mut()
        .find(|monster| monster.id == id)
        .expect("the body")
        .hp = hp;
}

#[test]
fn the_boss_repeats_slam_charge_slam_summon_with_three_seconds_of_walking_between() {
    let mut s = performing_field(|_| {});
    // 마차의 사각형 모서리 (17, 17)에서 7.07타일 떨어진 곳이라 대상인 마차가 8타일 안입니다. 돌진은 마차 앞에서 멈춥니다.
    put_boss(&mut s, 12.0, 12.0, 4);

    let list = performances(&mut s, 2000);

    let names: Vec<&str> = list.iter().map(|(name, _, _)| *name).collect();
    assert!(
        list.len() >= 8,
        "two rounds fit in a hundred seconds: {names:?}"
    );
    let round = ["slam", "charge", "slam", "summon"];
    for (index, name) in names.iter().enumerate() {
        assert_eq!(*name, round[index % 4], "act number {index}");
    }
    let rest = s.ticks(3.0);
    for pair in list.windows(2) {
        let (_, _, ended) = pair[0];
        let (_, started, _) = pair[1];
        assert_eq!(
            started - ended,
            rest,
            "three seconds of walking between acts"
        );
    }
    assert_eq!(
        list[0].1 + 1,
        rest,
        "the first act waits the same three seconds"
    );
}

#[test]
fn from_half_of_its_hp_the_boss_walks_two_seconds_between_acts() {
    let mut s = performing_field(|_| {});
    put_boss(&mut s, 12.0, 12.0, 4);
    let max = s.sim.boss().expect("a boss").max_hp;
    set_boss_hp(&mut s, max / 2);

    let list = performances(&mut s, 900);

    let rest = s.ticks(2.0);
    assert!(list.len() >= 4, "{list:?}");
    for pair in list.windows(2) {
        assert_eq!(pair[1].1 - pair[0].2, rest, "two seconds");
    }
}

#[test]
fn just_above_half_of_its_hp_the_boss_still_walks_three_seconds() {
    let mut s = performing_field(|_| {});
    put_boss(&mut s, 12.0, 12.0, 4);
    let max = s.sim.boss().expect("a boss").max_hp;
    set_boss_hp(&mut s, max / 2 + 1);

    let list = performances(&mut s, 900);

    let rest = s.ticks(3.0);
    assert!(list.len() >= 4, "{list:?}");
    for pair in list.windows(2) {
        assert_eq!(pair[1].1 - pair[0].2, rest, "three seconds");
    }
}

#[test]
fn the_walk_after_an_act_follows_the_hp_the_boss_has_when_the_act_ends() {
    let mut s = performing_field(|t| boss_tuning(t).script = vec![Move::Slam]);
    put_boss(&mut s, 12.0, 12.0, 4);
    let max = s.sim.boss().expect("a boss").max_hp;
    s.run_until(s.ticks(3.0) + 5, |sim| {
        matches!(sim.boss().map(|boss| &boss.act), Some(Act::Slamming(_)))
    });

    // 예고하는 동안에 절반 아래로 맞았다면, 예고가 끝난 뒤의 걷는 시간이 줄어듭니다.
    set_boss_hp(&mut s, max / 2);
    s.run_until(s.ticks(2.0), |sim| {
        matches!(sim.boss().map(|boss| &boss.act), Some(Act::Walking { .. }))
    });

    let Act::Walking { ticks_left } = act(&s) else {
        panic!("walking");
    };
    assert_eq!(ticks_left, s.ticks(2.0));
}

/// 보스가 한 번만 졸개를 부르는 들판입니다. 졸개는 서 있고 방에 플레이어가 `party`명 있으며, 동시에 살아 있는 몬스터의 상한은 `cap`입니다.
fn summoning(party: u32, cap: u32) -> Scenario {
    let mut s = performing_field(|t| {
        boss_tuning(t).script = vec![Move::Summon];
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.nights.max_alive = cap;
    });
    for _ in 0..party {
        s.player_at(1.0, 1.0);
    }
    put_boss(&mut s, 12.0, 12.0, party);
    s
}

fn grunts(s: &Scenario) -> usize {
    s.sim
        .monsters()
        .filter(|monster| monster.kind == MonsterKind::Grunt)
        .count()
}

fn run_until_roaring(s: &mut Scenario) {
    s.run_until(500, |sim| {
        matches!(sim.boss().map(|boss| &boss.act), Some(Act::Roaring { .. }))
    });
}

/// 포효가 끝나고 졸개 `minions`마리가 모두 나타날 만큼 진행합니다.
fn run_until_the_minions_are_out(s: &mut Scenario, minions: u32) {
    s.run_ticks(s.ticks(2.0) + minions * s.ticks(0.3) + 5);
}

#[test]
fn the_roar_lasts_two_seconds_and_then_the_minions_come_in_proportion_to_the_party() {
    for (party, minions) in [(1, 3), (2, 6), (3, 9), (4, 12)] {
        let mut s = summoning(party, 300);
        run_until_roaring(&mut s);
        let Act::Roaring { total_ticks, .. } = act(&s) else {
            panic!("roaring");
        };
        assert_eq!(total_ticks, s.ticks(2.0));
        assert_eq!(grunts(&s), 0, "nobody comes during the roar");

        run_until_the_minions_are_out(&mut s, minions);

        assert_eq!(grunts(&s) as u32, minions, "{party} players");
    }
}

#[test]
fn the_minions_come_one_at_a_time_after_the_roar_and_walk_to_the_wagon() {
    let mut s = summoning(4, 300);
    run_until_roaring(&mut s);
    s.run_ticks(s.ticks(2.0));
    let mut counts = Vec::new();
    for _ in 0..4 * s.ticks(0.3) {
        s.run_ticks(1);
        counts.push(grunts(&s));
    }

    assert!(
        counts.windows(2).all(|pair| pair[1] - pair[0] <= 1),
        "{counts:?}"
    );
    assert!(counts[0] <= 1, "{counts:?}");
    assert!(counts.last().is_some_and(|last| *last >= 3), "{counts:?}");
}

#[test]
fn the_minions_arrive_where_the_night_waves_arrive() {
    let mut s = summoning(4, 300);
    run_until_roaring(&mut s);
    run_until_the_minions_are_out(&mut s, 12);

    let wagon = s.sim.wagon().rect;
    let distance = s.sim.tuning().nights.spawn_distance;
    for monster in s.sim.monsters().filter(|m| m.kind == MonsterKind::Grunt) {
        let gap = wagon.distance_to(monster.pos);
        assert!(
            (gap - distance).abs() < 3.0,
            "{gap} tiles from the wagon, like the waves ({distance})"
        );
    }
}

#[test]
fn the_minions_never_push_the_number_of_living_monsters_over_the_cap() {
    let cap = 6;
    let mut s = summoning(4, cap);
    let mut most = 0;
    for _ in 0..400 {
        s.run_ticks(1);
        most = most.max(s.sim.monsters().count() as u32);
        assert!(s.sim.monsters().count() as u32 <= cap);
    }
    assert_eq!(
        most, cap,
        "the cap is reached and held (the boss counts as one)"
    );

    // 한 마리가 줄면 기다리던 졸개가 이어서 나타납니다.
    let victim = s
        .sim
        .monsters()
        .find(|monster| monster.kind == MonsterKind::Grunt)
        .map(|monster| monster.id)
        .expect("a minion");
    s.sim
        .world_mut()
        .monsters
        .retain(|monster| monster.id != victim);
    s.run_ticks(s.ticks(1.0));
    assert_eq!(s.sim.monsters().count() as u32, cap, "refilled");
}

#[test]
fn the_minions_called_before_the_boss_falls_stay_and_the_boss_fall_ends_the_expedition() {
    let mut s = summoning(4, 300);
    run_until_roaring(&mut s);
    run_until_the_minions_are_out(&mut s, 12);
    let minions = grunts(&s);
    assert_eq!(minions, 12);

    let slayer = s.sim.players().next().expect("a player").id;
    hit_boss(&mut s, slayer, 1_000_000);
    s.run_ticks(1);

    assert_eq!(
        outcome(&s.sim),
        Some(Outcome::Victory),
        "the same tick the boss falls"
    );
    assert_eq!(grunts(&s), minions, "the minions are still there");
}
