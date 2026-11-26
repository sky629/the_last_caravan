//! 피해 없이 몬스터에게 효과만 거는 포탑(감속 포탑)을 확인합니다. 효과는 `MonsterEffects`의 질문과 몬스터가 걸은 거리로 읽습니다.
//!
//! 시나리오는 20 x 20 맵이고 마차는 (17, 17)에서 (19, 19)까지입니다. 걷는 몬스터는 (5, 18)에서 마차를 향해 동쪽으로 걷습니다.

use super::support::{Scenario, close};
use crate::debrief::Report;
use crate::effect::MonsterEffects;
use crate::expedition::Outcome;
use crate::ids::MonsterId;
use crate::kind::StructureKind;
use crate::tuning::{Debuff, PulseTuning, TowerAction, Tuning};
use crate::world::Tile;

/// 걷는 몬스터 하나가 한 Tick에 걷는 거리입니다(초당 2.2타일, 20 Hz).
const WALK: f32 = 0.11;

/// 마차로 걸어가고 죽지 않고 플레이어도 포탑도 쫓지 않는 몬스터의 시나리오입니다. 걷는 거리만 봅니다.
fn walking_to_the_wagon() -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.aggro_range = 0.0;
    })
}

/// 몬스터가 제자리에 서서 죽지 않는 시나리오입니다.
fn standing_still() -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.monsters.grunt.aggro_range = 0.0;
    })
}

fn effects_of(s: &Scenario, id: MonsterId) -> &MonsterEffects {
    &s.monster(id).expect("the monster is alive").effects
}

/// 한 Tick을 진행하고, 그 Tick에 몬스터가 걸은 거리를 돌려줍니다.
fn walked_in_a_tick(s: &mut Scenario, id: MonsterId) -> f32 {
    let before = s.monster(id).unwrap().pos;
    s.run_ticks(1);
    s.monster(id).unwrap().pos.distance(before)
}

fn report_at_the_end(s: &mut Scenario) -> Report {
    s.sim.end_expedition(Outcome::Defeat);
    s.sim
        .debrief()
        .expect("the result is shown")
        .report()
        .clone()
}

#[test]
fn the_default_slow_tower_has_the_numbers_of_the_design() {
    let t = Tuning::default();
    let slow = &t.structures.slow_tower;
    assert_eq!((slow.range, slow.build_cost), (4.0, 15));
    assert_eq!(
        slow.action,
        TowerAction::Pulse(PulseTuning {
            pulse_secs: 0.5,
            debuff: Debuff::Slow {
                factor: 0.7,
                secs: 1.0
            },
        })
    );
}

#[test]
fn every_tower_kind_has_the_hp_the_build_time_and_the_repair_of_the_watchtower() {
    let t = Tuning::default();
    for kind in StructureKind::ALL {
        let row = t.structures.get(kind);
        assert_eq!(
            (row.max_hp, row.build_secs, &row.repair),
            (300, 3.0, &t.structures.watchtower.repair),
            "{kind:?}"
        );
    }
}

#[test]
fn a_monster_in_the_range_of_a_slow_tower_walks_at_70_percent_and_is_back_to_full_speed_within_a_second_of_leaving()
 {
    let mut s = walking_to_the_wagon();
    let tower = Tile::new(7, 16);
    s.tower_of(StructureKind::SlowTower, tower.x, tower.y);
    let range = s.sim.tuning().structures.slow_tower.range;
    let monster = s.monster_at(1.0, 18.0);

    // 한 Tick의 기록입니다. 그 Tick이 시작할 때 몬스터가 범위 안에 있었는지, 감속이 걸려 있었는지, 걸은 거리입니다.
    let mut ticks = Vec::new();
    for _ in 0..160 {
        let inside = tower.center().distance(s.monster(monster).unwrap().pos) <= range;
        let slowed = effects_of(&s, monster).is_slowed();
        let walked = walked_in_a_tick(&mut s, monster);
        ticks.push((inside, slowed, walked));
    }

    for (tick, (_, slowed, walked)) in ticks.iter().enumerate() {
        let expected = if *slowed { WALK * 0.7 } else { WALK };
        assert!(close(*walked, expected), "tick {tick}: {walked}");
    }
    let entered = ticks.iter().position(|tick| tick.0).expect("it enters");
    let left = ticks.iter().rposition(|tick| tick.0).expect("it was in") + 1;
    assert!(left < ticks.len(), "it walks out of the range");
    assert!(
        !ticks[entered].1,
        "the first tick in the range still walks at full speed, because an effect works from the next tick"
    );
    for (tick, (_, slowed, _)) in ticks.iter().enumerate().take(left).skip(entered + 1) {
        assert!(
            slowed,
            "tick {tick} is in the range and slowed, without a gap"
        );
    }
    let slowed_after_leaving = ticks[left..].iter().take_while(|tick| tick.1).count();
    assert!(
        slowed_after_leaving <= s.ticks(1.0) as usize,
        "{slowed_after_leaving} ticks slowed after leaving"
    );
    assert!(!ticks.last().unwrap().1, "back to full speed");
}

#[test]
fn a_monster_that_stands_in_the_range_of_a_slow_tower_stays_slowed_without_a_gap() {
    let mut s = standing_still();
    s.tower_of(StructureKind::SlowTower, 8, 8);
    let monster = s.monster_at(9.5, 10.5);

    s.run_ticks(1);
    for tick in 0..s.ticks(10.0) {
        assert!(effects_of(&s, monster).is_slowed(), "tick {tick}");
        s.run_ticks(1);
    }
}

#[test]
fn a_slow_tower_slows_every_monster_in_its_range_and_none_outside() {
    let mut s = standing_still();
    s.tower_of(StructureKind::SlowTower, 8, 8);
    let inside: Vec<MonsterId> = (0..5).map(|i| s.monster_at(6.5 + i as f32, 9.5)).collect();
    let outside = s.monster_at(14.0, 9.5);

    s.run_ticks(1);

    for id in inside {
        assert!(effects_of(&s, id).is_slowed(), "{id:?}");
    }
    assert!(
        !effects_of(&s, outside).is_slowed(),
        "5.5 tiles away is outside the range of 4"
    );
}

#[test]
fn a_crossbow_slow_of_50_percent_is_not_weakened_by_the_slow_tower_and_the_tower_takes_over_when_it_ends()
 {
    let mut s = walking_to_the_wagon();
    let archer = s.player_at(2.0, 2.0);
    s.tower_of(StructureKind::SlowTower, 9, 16);
    let monster = s.monster_at(6.0, 18.0);
    let crossbow = s
        .sim
        .tuning()
        .weapons
        .crossbow
        .on_hit
        .expect("the crossbow slows");

    s.afflict(monster, crossbow, archer);
    s.run_ticks(1);
    for tick in 1..s.ticks(2.0) {
        let walked = walked_in_a_tick(&mut s, monster);
        assert!(
            close(walked, WALK * 0.5),
            "tick {tick} of the crossbow slow: {walked}"
        );
    }
    // 석궁의 감속이 끝난 뒤 다음 펄스(0.5초 안)부터 포탑의 감속이 걸립니다.
    s.run_ticks(s.ticks(0.5) + 1);
    assert!(close(walked_in_a_tick(&mut s, monster), WALK * 0.7));
}

#[test]
fn a_slow_tower_takes_no_hp_and_neither_damage_nor_kills_show_in_the_result() {
    let mut s = standing_still();
    s.tower_of(StructureKind::SlowTower, 8, 8);
    let monsters: Vec<MonsterId> = (0..6).map(|i| s.monster_at(6.5 + i as f32, 10.0)).collect();

    s.run_ticks(s.ticks(10.0));

    for id in &monsters {
        assert_eq!(s.monster(*id).unwrap().hp, 1000, "{id:?}");
        assert!(
            effects_of(&s, *id).is_slowed(),
            "the tower did work on {id:?}"
        );
    }
    assert_eq!(report_at_the_end(&mut s).tower_kills, 0);
}
