//! 몬스터에게 걸리는 효과(감속, 출혈, 스턴)를 확인합니다. 효과의 시간은 `MonsterEffects`의 질문(`is_slowed`, `is_bleeding`, `is_stunned`)과
//! 몬스터의 위치와 HP로 읽고, 남은 Tick을 직접 읽지 않습니다.

use super::support::{Scenario, camp_of, close};
use crate::debrief::Report;
use crate::effect::MonsterEffects;
use crate::expedition::Outcome;
use crate::ids::{MonsterId, PlayerId};
use crate::kind::WeaponKind;
use crate::tuning::{Debuff, Tuning};

const SLOW: Debuff = Debuff::Slow {
    factor: 0.5,
    secs: 2.0,
};
const STUN: Debuff = Debuff::Stun { secs: 0.8 };

fn bleed(damage: u32) -> Debuff {
    Debuff::Bleed { damage, secs: 3.0 }
}

/// 몬스터가 제자리에 서서 죽지 않는 시나리오입니다. 효과가 걸리고 풀리는 시간만 봅니다.
fn standing_still() -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.monsters.grunt.aggro_range = 0.0;
    })
}

/// 마차로 걸어가고 죽지 않는 몬스터의 시나리오입니다. 플레이어를 쫓지 않으므로 걷는 거리만 봅니다.
fn walking_to_the_wagon() -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.aggro_range = 0.0;
    })
}

fn effects_of(s: &Scenario, id: MonsterId) -> &MonsterEffects {
    &s.monster(id).expect("the monster is alive").effects
}

/// 몬스터가 한 Tick에 걷는 거리입니다. 길은 타일 사이를 비스듬히 지나므로 가로 거리가 아니라 걸은 거리를 잽니다. 이 함수가 한 Tick을 진행합니다.
fn walked_in_a_tick(s: &mut Scenario, id: MonsterId) -> f32 {
    let before = s.monster(id).unwrap().pos;
    s.run_ticks(1);
    s.monster(id).unwrap().pos.distance(before)
}

/// `is_on`이 참인 Tick이 몇 번 이어지는지 셉니다. 이 함수가 Tick을 진행합니다.
fn ticks_while(s: &mut Scenario, mut is_on: impl FnMut(&Scenario) -> bool) -> u32 {
    let mut ticks = 0;
    while is_on(s) && ticks < 1000 {
        ticks += 1;
        s.run_ticks(1);
    }
    ticks
}

fn hp_of(s: &Scenario, id: MonsterId) -> u32 {
    s.monster(id).expect("the monster is alive").hp
}

/// 원정을 실패로 끝내고 결과를 돌려줍니다.
fn report_at_the_end(s: &mut Scenario) -> Report {
    s.sim.end_expedition(Outcome::Defeat);
    s.sim
        .debrief()
        .expect("the result is shown")
        .report()
        .clone()
}

fn kills_of(report: &Report, id: PlayerId) -> u32 {
    report
        .rows
        .iter()
        .find(|(player, _)| *player == id)
        .map(|(_, record)| record.kills)
        .expect("the player is in the table")
}

#[test]
fn every_weapon_carries_the_effect_of_its_row() {
    let t = Tuning::default();
    let stun = Some(STUN);
    let slow = Some(SLOW);
    for (kind, on_hit) in [
        (WeaponKind::Sword, Some(bleed(5))),
        (WeaponKind::Spear, Some(bleed(6))),
        (WeaponKind::Greatsword, stun),
        (WeaponKind::Rifle, stun),
        (WeaponKind::Bow, slow),
        (WeaponKind::Crossbow, slow),
    ] {
        assert_eq!(t.weapons.get(kind).on_hit, on_hit, "{kind:?}");
    }
}

#[test]
fn the_watchtower_does_not_apply_an_effect() {
    let t = Tuning::default();
    assert_eq!(t.structures.watchtower.on_hit, None);
}

#[test]
fn a_slowed_monster_walks_at_half_speed_for_exactly_the_time_of_the_slow() {
    let mut s = walking_to_the_wagon();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(5.0, 18.0);
    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.11),
        "2.2 tiles a second at 20 Hz"
    );

    s.afflict(monster, SLOW, owner);

    for tick in 1..=s.ticks(2.0) {
        let walked = walked_in_a_tick(&mut s, monster);
        assert!(close(walked, 0.055), "tick {tick} of the slow: {walked}");
    }
    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.11),
        "the old speed is back"
    );
    assert!(!effects_of(&s, monster).is_slowed());
}

#[test]
fn a_second_slow_restarts_the_time_and_does_not_slow_more() {
    let mut s = walking_to_the_wagon();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(5.0, 18.0);
    s.afflict(monster, SLOW, owner);
    s.run_ticks(s.ticks(1.0));

    s.afflict(monster, SLOW, owner);

    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.055),
        "half, not a quarter"
    );
    let slowed_for = ticks_while(&mut s, |s| effects_of(s, monster).is_slowed());
    assert_eq!(
        slowed_for + 1,
        s.ticks(2.0),
        "two seconds from the second slow"
    );
}

#[test]
fn a_slowed_monster_attacks_as_often_as_one_that_is_not() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 1000);
    let target = s.player_at(5.0, 5.0);
    let monster = s.monster_at(5.5, 5.0);
    s.afflict(
        monster,
        Debuff::Slow {
            factor: 0.5,
            secs: 10.0,
        },
        target,
    );
    let attacked = |s: &Scenario| s.monster(monster).unwrap().last_attack_tick;

    s.run_until(40, |sim| {
        sim.monsters().next().unwrap().last_attack_tick.is_some()
    });
    let first = attacked(&s).unwrap();
    s.run_until(60, |sim| {
        sim.monsters().next().unwrap().last_attack_tick != Some(first)
    });
    let second = attacked(&s).unwrap();

    assert!(effects_of(&s, monster).is_slowed());
    assert_eq!(second - first, u64::from(s.ticks(1.0)));
}

#[test]
fn a_stunned_monster_neither_walks_nor_attacks_but_its_cooldown_runs() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 1000);
    let player = s.player_at(5.0, 5.0);
    let monster = s.monster_at(5.5, 5.0);
    s.run_until(40, |sim| {
        sim.monsters().next().unwrap().last_attack_tick.is_some()
    });
    let first_attack = s.monster(monster).unwrap().last_attack_tick.unwrap();
    let hp = s.player(player).hp();

    s.afflict(monster, STUN, player);
    let stunned_for = ticks_while(&mut s, |s| effects_of(s, monster).is_stunned());

    assert_eq!(stunned_for, s.ticks(0.8));
    assert_eq!(s.player(player).hp(), hp, "no attack while stunned");
    // 쿨다운이 스턴 동안에도 흘렀으므로, 다음 공격은 스턴이 풀린 때에서 쿨다운을 새로 세는 것이 아니라 첫 공격에서 쿨다운이 지난 Tick에 나갑니다.
    s.run_until(40, |sim| {
        sim.monsters().next().unwrap().last_attack_tick != Some(first_attack)
    });
    let second_attack = s.monster(monster).unwrap().last_attack_tick.unwrap();
    assert_eq!(second_attack - first_attack, u64::from(s.ticks(1.0)));
}

#[test]
fn a_stunned_monster_does_not_walk_toward_the_wagon() {
    let mut s = walking_to_the_wagon();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(5.0, 18.0);
    let from = s.monster(monster).unwrap().pos;

    s.afflict(monster, STUN, owner);
    s.run_ticks(s.ticks(0.8));

    assert_eq!(s.monster(monster).unwrap().pos, from);
    s.run_ticks(1);
    assert!(
        s.monster(monster).unwrap().pos != from,
        "it walks again the tick after the stun"
    );
}

#[test]
fn stunned_monsters_that_overlap_are_still_pushed_apart() {
    let mut s = walking_to_the_wagon();
    let owner = s.player_at(2.0, 2.0);
    let near = s.monster_at(5.0, 10.0);
    let far = s.monster_at(5.3, 10.0);
    s.afflict(near, STUN, owner);
    s.afflict(far, STUN, owner);

    s.run_ticks(3);

    assert!(s.monster(near).unwrap().pos.x < 5.0, "pushed back");
    let pushed = s.monster(far).unwrap().pos.x - 5.3;
    assert!(pushed > 0.0, "pushed forward");
    assert!(pushed < 3.0 * 0.11, "pushed, not walking: {pushed}");
}

#[test]
fn a_stun_received_while_stunned_does_not_change_the_time_left() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    s.afflict(monster, STUN, owner);
    s.run_ticks(s.ticks(0.5));

    s.afflict(monster, STUN, owner);

    assert!(effects_of(&s, monster).is_stunned());
    let rest = ticks_while(&mut s, |s| effects_of(s, monster).is_stunned());
    assert_eq!(rest + s.ticks(0.5), s.ticks(0.8), "it still ends at 0.8 s");
}

#[test]
fn a_monster_whose_stun_ended_cannot_be_stunned_again_for_the_time_of_the_immunity() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    s.afflict(monster, STUN, owner);
    ticks_while(&mut s, |s| effects_of(s, monster).is_stunned());
    let immunity = s.ticks(2.0);

    s.run_ticks(immunity - 1);
    s.afflict(monster, STUN, owner);
    assert!(!effects_of(&s, monster).is_stunned(), "still immune");

    s.run_ticks(1);
    s.afflict(monster, STUN, owner);
    assert!(effects_of(&s, monster).is_stunned(), "free again");
}

#[test]
fn an_immune_monster_can_still_be_slowed_and_bleed() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    s.afflict(monster, STUN, owner);
    ticks_while(&mut s, |s| effects_of(s, monster).is_stunned());

    s.afflict(monster, bleed(5), owner);
    s.afflict(monster, SLOW, owner);

    assert!(effects_of(&s, monster).is_bleeding());
    assert!(effects_of(&s, monster).is_slowed());
    assert!(!effects_of(&s, monster).is_stunned());
}

#[test]
fn different_effects_are_on_a_monster_at_the_same_time() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);

    s.afflict(monster, SLOW, owner);
    s.afflict(monster, bleed(5), owner);
    s.afflict(monster, STUN, owner);

    let effects = effects_of(&s, monster);
    assert!(effects.is_slowed() && effects.is_bleeding() && effects.is_stunned());
}

#[test]
fn bleeding_deals_damage_once_a_second_starting_one_second_after_it_is_applied() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    let full = hp_of(&s, monster);

    s.afflict(monster, bleed(5), owner);
    s.run_ticks(s.ticks(1.0) - 1);
    assert_eq!(hp_of(&s, monster), full, "not yet");
    s.run_ticks(1);
    assert_eq!(hp_of(&s, monster), full - 5, "the first damage");
    s.run_ticks(s.ticks(2.0));
    assert_eq!(hp_of(&s, monster), full - 15, "three times in all");
    assert!(!effects_of(&s, monster).is_bleeding());
    s.run_ticks(s.ticks(2.0));
    assert_eq!(hp_of(&s, monster), full - 15, "nothing after it ended");
}

#[test]
fn a_new_bleed_restarts_the_time_but_not_the_clock_of_its_damage() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    let full = hp_of(&s, monster);
    s.afflict(monster, bleed(5), owner);
    s.run_ticks(s.ticks(0.5));

    s.afflict(monster, bleed(5), owner);
    s.run_ticks(s.ticks(0.5) - 1);
    assert_eq!(hp_of(&s, monster), full, "one second after the first bleed");
    s.run_ticks(1);
    assert_eq!(
        hp_of(&s, monster),
        full - 5,
        "the damage comes one second after the first bleed, not the second"
    );

    // 피해는 첫 출혈의 시점(1초, 2초, 3초)에 나고, 시간은 두 번째 출혈에서 3초이므로 첫 출혈에서 3.5초 동안 이어집니다.
    s.run_ticks(s.ticks(2.0));
    assert_eq!(
        hp_of(&s, monster),
        full - 15,
        "the third damage, 3 s after the first"
    );
    s.run_ticks(s.ticks(0.5) - 1);
    assert!(
        effects_of(&s, monster).is_bleeding(),
        "the second blow restarted the time"
    );
    s.run_ticks(1);
    assert!(
        !effects_of(&s, monster).is_bleeding(),
        "3.5 s after the first"
    );
    s.run_ticks(s.ticks(2.0));
    assert_eq!(hp_of(&s, monster), full - 15, "three times in all");
}

#[test]
fn a_new_bleed_takes_over_the_damage_of_the_old_one() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    let full = hp_of(&s, monster);
    s.afflict(monster, bleed(5), owner);
    s.run_ticks(s.ticks(0.5));
    s.afflict(monster, bleed(6), owner);

    s.run_ticks(s.ticks(0.5));

    assert_eq!(hp_of(&s, monster), full - 6);
}

#[test]
fn a_kill_by_bleeding_belongs_to_the_player_who_applied_the_bleed_last() {
    // HP 12는 5와 6의 합(11)보다 많고 6의 두 배(12)와 같으므로, 두 번째 사람의 피해량(6)으로 두 번 맞아야 죽습니다.
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 12;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.monsters.grunt.aggro_range = 0.0;
    });
    let first = s.player_at(2.0, 2.0);
    let second = s.player_at(3.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    s.afflict(monster, bleed(5), first);
    s.run_ticks(s.ticks(0.5));
    s.afflict(monster, bleed(6), second);

    s.run_until(200, |sim| sim.monsters().next().is_none());

    let report = report_at_the_end(&mut s);
    assert_eq!(kills_of(&report, first), 0);
    assert_eq!(kills_of(&report, second), 1);
}

#[test]
fn a_kill_by_bleeding_after_the_player_left_is_counted_for_nobody() {
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 5;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.monsters.grunt.aggro_range = 0.0;
    });
    let leaver = s.player_at(2.0, 2.0);
    let bystander = s.player_at(3.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    s.afflict(monster, bleed(5), leaver);
    s.sim.remove_player(leaver);

    s.run_until(100, |sim| sim.monsters().next().is_none());

    let report = report_at_the_end(&mut s);
    assert_eq!(report.rows.len(), 1, "only the bystander is left");
    assert_eq!(kills_of(&report, bystander), 0);
}

#[test]
fn the_camp_scarecrow_shows_effects_but_loses_no_hp_and_reports_no_bleed_numbers() {
    let mut s = Scenario::camp(|_| {});
    let owner = s.player_at(2.0, 2.0);
    let scarecrow = s.sim.monsters().next().unwrap().id;
    let full = hp_of(&s, scarecrow);

    s.afflict(scarecrow, bleed(5), owner);
    s.afflict(scarecrow, SLOW, owner);
    s.run_ticks(s.ticks(1.0) + 1);

    assert!(effects_of(&s, scarecrow).is_bleeding());
    assert!(effects_of(&s, scarecrow).is_slowed());
    s.run_ticks(s.ticks(2.5));

    assert!(
        !effects_of(&s, scarecrow).is_bleeding(),
        "the bleed ran out"
    );
    assert!(!effects_of(&s, scarecrow).is_slowed(), "the slow ran out");
    assert_eq!(hp_of(&s, scarecrow), full, "no damage in the camp");
    assert_eq!(
        camp_of(&s.sim).struck().next(),
        None,
        "no number for the bleed"
    );
}
