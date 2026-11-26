//! 피해 없이 몬스터에게 효과만 거는 포탑(감속 포탑, 스턴 포탑)을 확인합니다. 효과는 `MonsterEffects`의 질문과 몬스터가 걸은 거리로 읽고,
//! 포탑의 사격은 `TowerState::Standing`의 `last_shot`으로 읽습니다.
//!
//! 시나리오는 20 x 20 맵이고 마차는 (17, 17)에서 (19, 19)까지입니다. 걷는 몬스터는 (5, 18)에서 마차를 향해 동쪽으로 걷습니다.

use super::support::{Scenario, close};
use crate::debrief::Report;
use crate::effect::MonsterEffects;
use crate::expedition::Outcome;
use crate::ids::{MonsterId, TowerId};
use crate::kind::StructureKind;
use crate::structures::{Shot, TowerState};
use crate::tuning::{AfflictTuning, Debuff, PulseTuning, TowerAction, Tuning};
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

/// 포탑의 마지막 사격입니다. 스턴 포탑이 효과를 건 몬스터가 여기에 남습니다.
fn last_shot(s: &Scenario, id: TowerId) -> Option<Shot> {
    match s
        .sim
        .towers()
        .find(|tower| tower.id == id)
        .map(|tower| &tower.state)
    {
        Some(TowerState::Standing { last_shot, .. }) => *last_shot,
        _ => None,
    }
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
fn the_default_stun_tower_has_the_numbers_of_the_design() {
    let t = Tuning::default();
    let stun = &t.structures.stun_tower;
    assert_eq!((stun.range, stun.build_cost), (5.0, 20));
    assert_eq!(
        stun.action,
        TowerAction::Afflict(AfflictTuning {
            cooldown_secs: 2.0,
            debuff: Debuff::Stun { secs: 0.8 },
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
fn a_stun_tower_stuns_the_nearest_monster_that_can_be_stunned_and_skips_a_stunned_one() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let tower = s.tower_of(StructureKind::StunTower, 1, 10);
    let near = s.monster_at(3.5, 10.5);
    let far = s.monster_at(5.0, 10.5);
    s.afflict(near, Debuff::Stun { secs: 0.8 }, owner);

    s.run_ticks(1);

    assert!(effects_of(&s, far).is_stunned(), "the farther one");
    assert_eq!(last_shot(&s, tower).map(|shot| shot.target), Some(far));
}

#[test]
fn a_stun_tower_skips_a_monster_that_is_immune_to_stun() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let near = s.monster_at(3.5, 10.5);
    let far = s.monster_at(5.0, 10.5);
    // 한 Tick짜리 스턴이 끝나면 near는 면역입니다. 포탑은 그 뒤에 섭니다.
    s.afflict(near, Debuff::Stun { secs: 0.05 }, owner);
    s.run_ticks(1);
    assert!(!effects_of(&s, near).is_stunned(), "its stun is over");
    let tower = s.tower_of(StructureKind::StunTower, 1, 10);

    s.run_ticks(1);

    assert!(
        effects_of(&s, far).is_stunned(),
        "the one that can be stunned"
    );
    assert_eq!(last_shot(&s, tower).map(|shot| shot.target), Some(far));
}

#[test]
fn a_stun_tower_with_nothing_to_stun_does_not_shoot_and_does_not_spend_its_cooldown() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let tower = s.tower_of(StructureKind::StunTower, 1, 10);
    let monster = s.monster_at(3.5, 10.5);
    s.afflict(monster, Debuff::Stun { secs: 0.8 }, owner);

    // 스턴 0.8초와 면역 2초 동안에는 걸 수 없습니다. 면역이 끝난 다음 Tick에 곧바로 겁니다.
    s.run_ticks(s.ticks(0.8) + s.ticks(2.0));
    assert_eq!(last_shot(&s, tower), None, "nothing to stun until now");
    assert!(!effects_of(&s, monster).is_stunned());

    s.run_ticks(1);
    assert!(
        effects_of(&s, monster).is_stunned(),
        "the cooldown was not spent while there was nothing to stun"
    );
}

#[test]
fn two_stun_towers_stun_two_monsters_in_the_same_tick() {
    let mut s = standing_still();
    s.tower_of(StructureKind::StunTower, 1, 10);
    s.tower_of(StructureKind::StunTower, 1, 12);
    // 두 포탑 모두에게 첫째가 더 가깝습니다.
    let first = s.monster_at(3.5, 11.5);
    let second = s.monster_at(4.5, 11.5);

    s.run_ticks(1);

    assert!(effects_of(&s, first).is_stunned());
    assert!(
        effects_of(&s, second).is_stunned(),
        "the second tower did not spend its stun on the monster the first one took"
    );
}

#[test]
fn a_stun_tower_stuns_one_monster_every_two_seconds() {
    let mut s = standing_still();
    s.tower_of(StructureKind::StunTower, 1, 10);
    let monsters: Vec<MonsterId> = (0..4)
        .map(|i| s.monster_at(3.5 + i as f32 * 0.9, 10.5))
        .collect();
    let stunned = |s: &Scenario| {
        monsters
            .iter()
            .filter(|id| effects_of(s, **id).is_stunned())
            .count()
    };

    s.run_ticks(1);
    assert_eq!(stunned(&s), 1);
    s.run_ticks(s.ticks(2.0) - 1);
    assert_eq!(
        stunned(&s),
        0,
        "the first stun is over and the cooldown is not"
    );
    s.run_ticks(1);
    assert_eq!(stunned(&s), 1, "the next one");
}

#[test]
fn debuff_towers_take_no_hp_and_neither_damage_nor_kills_show_in_the_result() {
    let mut s = standing_still();
    s.tower_of(StructureKind::SlowTower, 8, 8);
    s.tower_of(StructureKind::StunTower, 8, 11);
    let monsters: Vec<MonsterId> = (0..6).map(|i| s.monster_at(6.5 + i as f32, 10.0)).collect();

    let mut ever_stunned = false;
    for _ in 0..s.ticks(10.0) {
        s.run_ticks(1);
        ever_stunned |= monsters.iter().any(|id| effects_of(&s, *id).is_stunned());
    }

    assert!(ever_stunned, "the stun tower did work");
    for id in &monsters {
        assert_eq!(s.monster(*id).unwrap().hp, 1000, "{id:?}");
        assert!(
            effects_of(&s, *id).is_slowed(),
            "the slow tower did work on {id:?}"
        );
    }
    assert_eq!(report_at_the_end(&mut s).tower_kills, 0);
}
