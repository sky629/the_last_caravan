//! 몬스터에게 걸리는 효과(감속, 출혈, 스턴)를 확인합니다. 효과의 시간은 `MonsterEffects`의 질문(`is_slowed`, `is_bleeding`, `is_stunned`)과
//! 몬스터의 위치와 HP로 읽고, 남은 Tick을 직접 읽지 않습니다.

use super::support::{Scenario, camp_of, close, watchtower_shot};
use crate::debrief::Report;
use crate::effect::MonsterEffects;
use crate::expedition::Outcome;
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, PlayerInput};
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

fn attack_east() -> PlayerInput {
    PlayerInput {
        aim: AimDir::new(1.0, 0.0),
        attack: true,
        ..PlayerInput::default()
    }
}

/// 한 번 공격하고 키를 뗍니다. 이 함수가 한 Tick을 진행합니다. 쿨다운이 0이어야 공격이 나갑니다.
fn strike_once(s: &mut Scenario, id: PlayerId) {
    s.hold(id, attack_east());
    s.run_ticks(1);
    s.hold(id, PlayerInput::default());
}

fn armed(s: &mut Scenario, x: f32, y: f32, weapon: WeaponKind) -> PlayerId {
    let id = s.player_at(x, y);
    s.give_weapon(id, weapon);
    id
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
    let mut t = Tuning::default();
    assert_eq!(watchtower_shot(&mut t).on_hit, None);
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
fn a_kill_by_bleeding_belongs_to_the_player_whose_bleed_is_on_the_monster() {
    // HP 12는 5와 6의 합(11)보다 많고 6의 두 배(12)와 같으므로, 더 센 두 번째 사람의 피해량(6)으로 두 번 맞아야 죽습니다.
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

/// 무기 하나로 몬스터를 한 번 맞히고, 걸린 효과를 (감속, 출혈, 스턴)의 순서로 돌려줍니다. 탄환은 몬스터에 닿을 때까지 진행합니다.
fn effects_after_one_hit_with(weapon: WeaponKind) -> (bool, bool, bool) {
    let mut s = standing_still();
    let attacker = armed(&mut s, 5.0, 10.0, weapon);
    let monster = s.monster_at(6.5, 10.0);
    strike_once(&mut s, attacker);
    s.run_until(30, |sim| sim.projectiles().all(|p| !p.is_flying()));
    let effects = effects_of(&s, monster);
    (
        effects.is_slowed(),
        effects.is_bleeding(),
        effects.is_stunned(),
    )
}

#[test]
fn each_weapon_puts_its_own_effect_on_the_monster_it_hits_and_no_other() {
    for (weapon, expected) in [
        (WeaponKind::Sword, (false, true, false)),
        (WeaponKind::Spear, (false, true, false)),
        (WeaponKind::Greatsword, (false, false, true)),
        (WeaponKind::Rifle, (false, false, true)),
        (WeaponKind::Bow, (true, false, false)),
        (WeaponKind::Crossbow, (true, false, false)),
    ] {
        assert_eq!(effects_after_one_hit_with(weapon), expected, "{weapon:?}");
    }
}

#[test]
fn a_monster_hit_by_a_crossbow_bolt_walks_at_half_speed_for_two_seconds() {
    let mut s = walking_to_the_wagon();
    let archer = armed(&mut s, 1.0, 18.0, WeaponKind::Crossbow);
    let monster = s.monster_at(5.0, 18.0);
    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.11),
        "2.2 tiles a second at 20 Hz"
    );

    strike_once(&mut s, archer);
    s.run_until(30, |sim| sim.monsters().any(|m| m.effects.is_slowed()));
    let slowed_for = ticks_while(&mut s, |s| effects_of(s, monster).is_slowed());

    assert_eq!(slowed_for, s.ticks(2.0));
    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.11),
        "the old speed is back"
    );
}

#[test]
fn a_crossbow_bolt_slows_every_monster_it_passes_through() {
    let mut s = standing_still();
    let archer = armed(&mut s, 1.0, 10.0, WeaponKind::Crossbow);
    let line: Vec<MonsterId> = (0..4).map(|i| s.monster_at(4.0 + i as f32, 10.0)).collect();
    let fifth = s.monster_at(8.0, 10.0);

    strike_once(&mut s, archer);
    s.run_until(30, |sim| sim.projectiles().all(|p| !p.is_flying()));

    for id in line {
        assert!(effects_of(&s, id).is_slowed(), "{id:?}");
    }
    assert!(!effects_of(&s, fifth).is_slowed(), "the bolt ended at four");
}

#[test]
fn a_bolt_that_was_shot_slows_even_after_the_archer_changed_weapons() {
    let mut s = standing_still();
    let archer = armed(&mut s, 1.0, 10.0, WeaponKind::Crossbow);
    let monster = s.monster_at(9.0, 10.0);

    strike_once(&mut s, archer);
    s.give_weapon(archer, WeaponKind::Greatsword);
    s.run_until(30, |sim| sim.projectiles().all(|p| !p.is_flying()));

    assert!(effects_of(&s, monster).is_slowed());
    assert!(!effects_of(&s, monster).is_stunned());
}

#[test]
fn two_smiths_who_take_turns_cannot_keep_a_monster_stunned_past_the_first_stun() {
    let mut s = standing_still();
    let first = armed(&mut s, 5.0, 10.0, WeaponKind::Greatsword);
    let second = armed(&mut s, 5.0, 11.0, WeaponKind::Greatsword);
    let monster = s.monster_at(6.0, 10.5);

    strike_once(&mut s, first);
    s.run_ticks(s.ticks(0.5));
    strike_once(&mut s, second);
    assert!(
        effects_of(&s, monster).is_stunned(),
        "the second blow changed nothing"
    );
    // 첫 스턴은 0.8초에 끝납니다. 두 번째 공격이 나간 Tick은 이미 0.5초가 지난 뒤입니다.
    let rest = ticks_while(&mut s, |s| effects_of(s, monster).is_stunned());
    assert_eq!(rest + s.ticks(0.5) + 1, s.ticks(0.8));

    // 스턴이 끝난 뒤 2초 동안은 스턴이 다시 걸리지 않습니다.
    s.run_ticks(s.ticks(1.0));
    strike_once(&mut s, first);
    assert!(!effects_of(&s, monster).is_stunned(), "immune");
    s.run_ticks(s.ticks(1.0));
    s.run_ticks(2);
    strike_once(&mut s, second);
    assert!(effects_of(&s, monster).is_stunned(), "free again");
}

#[test]
fn a_sword_blow_makes_a_monster_bleed_five_a_second_for_three_seconds() {
    let mut s = standing_still();
    let swordsman = armed(&mut s, 5.0, 10.0, WeaponKind::Sword);
    let monster = s.monster_at(5.8, 10.0);
    let full = hp_of(&s, monster);
    let blow = s.sim.tuning().weapons.sword.damage;

    strike_once(&mut s, swordsman);
    assert_eq!(hp_of(&s, monster), full - blow);
    s.run_ticks(s.ticks(1.0) - 1);
    assert_eq!(hp_of(&s, monster), full - blow, "not yet");
    s.run_ticks(1);
    assert_eq!(hp_of(&s, monster), full - blow - 5);
    s.run_ticks(s.ticks(2.0));
    assert_eq!(hp_of(&s, monster), full - blow - 15, "three times in all");
    assert!(!effects_of(&s, monster).is_bleeding());
}

#[test]
fn a_kill_by_bleeding_belongs_to_the_player_whose_stronger_bleed_replaced_the_other() {
    // 검의 피해, 창의 피해, 창의 출혈(6) 두 번이 합쳐서 HP와 같습니다. 창의 출혈이 검의 출혈(5)보다 세므로 칸을 이어받고, 두 번째 사람의 출혈이 두 번째 피해를 냅니다.
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = t.weapons.sword.damage + t.weapons.spear.damage + 2 * 6;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.monsters.grunt.aggro_range = 0.0;
    });
    let first = armed(&mut s, 5.0, 10.0, WeaponKind::Sword);
    let second = armed(&mut s, 3.0, 10.5, WeaponKind::Spear);
    s.monster_at(5.8, 10.5);

    strike_once(&mut s, first);
    s.run_ticks(s.ticks(0.5));
    strike_once(&mut s, second);
    s.run_until(200, |sim| sim.monsters().next().is_none());

    let report = report_at_the_end(&mut s);
    assert_eq!(kills_of(&report, first), 0);
    assert_eq!(kills_of(&report, second), 1);
}

#[test]
fn a_blow_that_kills_one_monster_still_puts_its_effect_on_the_survivor_beside_it() {
    let mut s = standing_still();
    let swordsman = armed(&mut s, 5.0, 10.0, WeaponKind::Sword);
    let doomed = s.monster_at(5.8, 10.0);
    let survivor = s.monster_at(5.9, 10.3);
    s.sim.world_mut().monsters[0].hp = s.sim.tuning().weapons.sword.damage;

    strike_once(&mut s, swordsman);

    assert!(s.monster(doomed).is_none(), "the blow killed it");
    assert!(effects_of(&s, survivor).is_bleeding());
}

#[test]
fn a_blow_and_a_bleed_that_could_each_kill_in_one_tick_give_the_kill_to_the_blow() {
    let mut s = standing_still();
    let swordsman = armed(&mut s, 5.0, 10.0, WeaponKind::Sword);
    let bleeder = s.player_at(2.0, 2.0);
    let monster = s.monster_at(5.8, 10.0);
    // 출혈의 피해 5만으로도, 검의 한 번만으로도 죽는 HP입니다. 직접 맞은 피해가 출혈의 피해보다 먼저 적용됩니다.
    s.sim.world_mut().monsters[0].hp = 5;
    s.afflict(monster, bleed(5), bleeder);
    s.run_ticks(s.ticks(1.0) - 1);
    assert!(s.monster(monster).is_some(), "the bleed has not hurt yet");

    strike_once(&mut s, swordsman);

    assert!(s.monster(monster).is_none());
    let report = report_at_the_end(&mut s);
    assert_eq!(kills_of(&report, swordsman), 1);
    assert_eq!(kills_of(&report, bleeder), 0);
}

#[test]
fn a_sword_blow_on_a_camp_scarecrow_shows_the_bleed_and_costs_nothing() {
    let mut s = Scenario::camp(|_| {});
    let swordsman = armed(&mut s, 11.5, 5.0, WeaponKind::Sword);
    let scarecrow = s.sim.monsters().next().unwrap().id;
    let full = hp_of(&s, scarecrow);

    strike_once(&mut s, swordsman);
    let struck = camp_of(&s.sim).struck().next().expect("the blow is noted");
    assert!(effects_of(&s, scarecrow).is_bleeding());

    s.run_ticks(s.ticks(3.5));

    assert!(
        !effects_of(&s, scarecrow).is_bleeding(),
        "the bleed ran out"
    );
    assert_eq!(hp_of(&s, scarecrow), full, "no damage in the camp");
    assert_eq!(
        camp_of(&s.sim).struck().next(),
        Some(struck),
        "no number for the bleed"
    );
}

#[test]
fn a_watchtower_applies_the_effect_of_its_row_to_the_monster_it_shoots() {
    let mut s = standing_still_with_a_tower(Some(STUN));

    s.run_until(100, |sim| sim.monsters().any(|m| m.effects.is_stunned()));
}

#[test]
fn a_watchtower_of_the_default_tuning_shoots_without_an_effect() {
    let mut s = standing_still_with_a_tower(None);
    let monster = s.sim.monsters().next().unwrap().id;
    let full = hp_of(&s, monster);

    s.run_until(100, |sim| {
        sim.monsters().next().is_some_and(|m| m.hp < full)
    });

    let effects = effects_of(&s, monster);
    assert!(!effects.is_slowed() && !effects.is_bleeding() && !effects.is_stunned());
}

/// 감시탑 하나가 제자리에 선 몬스터 하나를 쏘는 시나리오입니다. 감시탑의 행의 효과는 `on_hit`입니다.
fn standing_still_with_a_tower(on_hit: Option<Debuff>) -> Scenario {
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.monsters.grunt.aggro_range = 0.0;
        watchtower_shot(t).on_hit = on_hit;
    });
    s.tower_at(14, 3);
    s.monster_at(14.5, 5.0);
    s
}

fn slow(factor: f32, secs: f32) -> Debuff {
    Debuff::Slow { factor, secs }
}

/// 제자리에 서고 HP가 `hp`인 몬스터의 시나리오입니다. 출혈의 피해로 죽는지 봅니다.
fn standing_still_with_hp(hp: u32) -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = hp;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.monsters.grunt.aggro_range = 0.0;
    })
}

#[test]
fn a_weaker_slow_is_ignored_while_a_stronger_one_lasts_and_is_not_remembered() {
    let mut s = walking_to_the_wagon();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(5.0, 18.0);
    s.afflict(monster, slow(0.5, 2.0), owner);
    s.run_ticks(s.ticks(1.0));

    // 남은 시간은 1초이고 새 감속은 3초입니다. 시간이 더 길어도 약하므로 버려집니다.
    s.afflict(monster, slow(0.7, 3.0), owner);

    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.055),
        "still half speed"
    );
    let rest = ticks_while(&mut s, |s| effects_of(s, monster).is_slowed());
    assert_eq!(
        rest + 1 + s.ticks(1.0),
        s.ticks(2.0),
        "it ends with the first slow"
    );
    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.11),
        "the weaker slow was not kept for after the stronger one"
    );
}

#[test]
fn a_stronger_slow_replaces_a_weaker_one_even_when_it_is_shorter() {
    let mut s = walking_to_the_wagon();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(5.0, 18.0);
    s.afflict(monster, slow(0.7, 3.0), owner);

    s.afflict(monster, slow(0.5, 1.0), owner);

    assert!(
        close(walked_in_a_tick(&mut s, monster), 0.055),
        "half speed"
    );
    let rest = ticks_while(&mut s, |s| effects_of(s, monster).is_slowed());
    assert_eq!(rest + 1, s.ticks(1.0), "the time of the stronger slow");
}

#[test]
fn an_equal_slow_with_less_time_left_is_ignored() {
    let mut s = walking_to_the_wagon();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(5.0, 18.0);
    s.afflict(monster, slow(0.5, 3.0), owner);

    s.afflict(monster, SLOW, owner);

    let slowed_for = ticks_while(&mut s, |s| effects_of(s, monster).is_slowed());
    assert_eq!(slowed_for, s.ticks(3.0), "the longer one stays");
}

#[test]
fn a_spear_bleed_is_not_replaced_by_a_sword_bleed_and_the_kill_stays_with_the_spear_player() {
    let t = Tuning::default();
    let spear = t.weapons.spear.on_hit.expect("the spear makes a bleed");
    let sword = t.weapons.sword.on_hit.expect("the sword makes a bleed");
    // HP 18은 창의 출혈(6) 세 번입니다. 검의 출혈(5)로 바뀌었다면 세 번으로 죽지 않습니다.
    let mut s = standing_still_with_hp(18);
    let spearman = s.player_at(2.0, 2.0);
    let swordsman = s.player_at(3.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    s.afflict(monster, spear, spearman);
    s.run_ticks(s.ticks(0.5));

    s.afflict(monster, sword, swordsman);

    s.run_ticks(s.ticks(0.5));
    assert_eq!(hp_of(&s, monster), 12, "six, not five");
    s.run_ticks(s.ticks(2.0) - 1);
    assert_eq!(hp_of(&s, monster), 6);
    s.run_ticks(1);
    assert!(
        s.monster(monster).is_none(),
        "three pulses of six, three seconds after the spear"
    );
    let report = report_at_the_end(&mut s);
    assert_eq!(kills_of(&report, spearman), 1);
    assert_eq!(kills_of(&report, swordsman), 0);
}

#[test]
fn of_two_bleeds_of_the_same_damage_and_the_same_time_left_the_newer_one_stays() {
    let mut s = standing_still_with_hp(5);
    let first = s.player_at(2.0, 2.0);
    let second = s.player_at(3.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);

    s.afflict(monster, bleed(5), first);
    s.afflict(monster, bleed(5), second);
    s.run_until(100, |sim| sim.monsters().next().is_none());

    let report = report_at_the_end(&mut s);
    assert_eq!(kills_of(&report, first), 0);
    assert_eq!(kills_of(&report, second), 1);
}

#[test]
fn a_stronger_bleed_continues_the_clock_of_the_damage() {
    let mut s = standing_still();
    let owner = s.player_at(2.0, 2.0);
    let monster = s.monster_at(6.0, 10.0);
    let full = hp_of(&s, monster);
    s.afflict(monster, bleed(5), owner);
    s.run_ticks(s.ticks(0.5));

    s.afflict(monster, bleed(6), owner);

    s.run_ticks(s.ticks(0.5) - 1);
    assert_eq!(
        hp_of(&s, monster),
        full,
        "one second after the first bleed, not the second"
    );
    s.run_ticks(1);
    assert_eq!(hp_of(&s, monster), full - 6);
}
