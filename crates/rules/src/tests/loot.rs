//! 바닥의 무기를 확인합니다: 줍기와 맞바꾸기, 상자에서 나온 무기가 사라지지도 늘지도 않는 것, 시작 무기가 바닥에 놓이지 않는 것, 쓰러짐과 퇴장과 새 날과 새 원정에서의 무기, F의 우선순위와 안내.
//!
//! 시험하는 것은 `&Sim`이 공개하는 값(바닥의 무기, 가진 무기, 안내)뿐입니다.

use super::support::{Scenario, nights};
use crate::expedition::{Outcome, Phase};
use crate::ids::PlayerId;
use crate::input::{MoveDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::loadout::Loadout;
use crate::loot::Prompt;
use crate::player::Interaction;

/// `(10, 10)`에 플레이어를 놓고, 그 동쪽 0.5타일에 무기 `kind`를 놓습니다.
fn weapon_beside(s: &mut Scenario, kind: WeaponKind) -> PlayerId {
    let a = s.player_at(10.0, 10.0);
    s.weapon_at(10.5, 10.0, kind);
    a
}

fn weapons_on_the_ground(s: &Scenario) -> Vec<WeaponKind> {
    s.sim.ground_weapons().map(|weapon| weapon.kind).collect()
}

/// 플레이어가 가진 무기와 바닥의 무기를 모두 모아서 정렬합니다.
fn all_weapons(s: &Scenario) -> Vec<WeaponKind> {
    let mut all: Vec<WeaponKind> = s
        .sim
        .players()
        .flat_map(|player| player.loadout.weapons())
        .chain(s.sim.ground_weapons().map(|weapon| weapon.kind))
        .collect();
    all.sort();
    all
}

#[test]
fn one_press_of_f_puts_the_weapon_in_the_empty_slot_and_removes_it_from_the_ground() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Spear))
    );
    assert!(weapons_on_the_ground(&s).is_empty());
}

#[test]
fn with_both_slots_full_a_press_swaps_the_held_weapon_with_the_one_on_the_ground() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Greatsword);
    s.arm(a, WeaponKind::Spear, Some(WeaponKind::Bow));
    let before = s.sim.ground_weapons().next().unwrap().clone();

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Greatsword, Some(WeaponKind::Bow))
    );
    let left: Vec<_> = s.sim.ground_weapons().cloned().collect();
    assert_eq!(
        left.len(),
        1,
        "the weapon on the ground is swapped, not replaced"
    );
    assert_eq!(
        (left[0].id, left[0].pos, left[0].kind),
        (before.id, before.pos, WeaponKind::Spear),
        "the spear lies where the greatsword lay"
    );
}

#[test]
fn a_starting_weapon_that_is_swapped_away_vanishes_instead_of_lying_on_the_ground() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    s.arm(a, WeaponKind::Sword, Some(WeaponKind::Bow));

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Spear, Some(WeaponKind::Bow))
    );
    assert!(
        weapons_on_the_ground(&s).is_empty(),
        "the sword every joining player is given does not lie on the ground"
    );
}

#[test]
fn what_vanishes_in_a_swap_is_what_the_start_loadout_gives_not_the_sword_by_name() {
    let mut s = Scenario::with_tuning(|t| {
        t.player.start_loadout = Loadout::new(WeaponKind::Bow, None);
    });
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    s.arm(a, WeaponKind::Sword, Some(WeaponKind::Bow));

    s.press_interact_key(a);

    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Sword]);
}

#[test]
fn joining_taking_two_weapons_and_leaving_over_and_over_adds_no_weapon_to_the_ground() {
    let mut s = Scenario::open_field();
    s.weapon_at(10.5, 10.0, WeaponKind::Greatsword);
    s.weapon_at(10.0, 10.5, WeaponKind::Spear);

    for round in 0..50 {
        let a = s.player_at(10.0, 10.0);
        s.press_interact_key(a);
        s.press_interact_key(a);
        assert!(
            weapons_on_the_ground(&s).is_empty(),
            "the joiner holds both weapons and the sword is gone"
        );
        s.sim.remove_player(a);

        let mut left = weapons_on_the_ground(&s);
        left.sort();
        assert_eq!(
            left,
            [WeaponKind::Greatsword, WeaponKind::Spear],
            "the ground holds the same two weapons after {} rounds",
            round + 1
        );
    }
}

#[test]
fn holding_f_takes_the_weapon_once_and_does_not_swap_back() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Greatsword);
    s.arm(a, WeaponKind::Spear, Some(WeaponKind::Bow));

    s.press_interact_key(a);
    s.hold_interact(a);
    s.run_ticks(40);

    assert_eq!(s.player(a).weapon(), WeaponKind::Greatsword);
    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
}

#[test]
fn a_tap_of_f_that_was_released_before_the_tick_still_takes_the_weapon() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    s.hold(
        a,
        PlayerInput {
            interact: false,
            interact_pressed: true,
            ..PlayerInput::default()
        },
    );

    s.run_ticks(1);

    assert_eq!(s.player(a).loadout.spare(), Some(WeaponKind::Spear));
}

#[test]
fn f_held_before_walking_up_to_a_weapon_does_not_take_it() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 10.0);
    s.weapon_at(10.5, 10.0, WeaponKind::Spear);
    s.hold(
        a,
        PlayerInput {
            interact: true,
            move_dir: MoveDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );

    s.run_ticks(60);

    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
    assert_eq!(s.player(a).loadout.spare(), None);
}

#[test]
fn a_weapon_out_of_reach_is_not_taken() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    s.weapon_at(13.0, 10.0, WeaponKind::Spear);

    s.press_interact_key(a);

    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
}

#[test]
fn the_nearest_of_two_weapons_in_reach_is_taken() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    s.weapon_at(10.9, 10.0, WeaponKind::Spear);
    s.weapon_at(10.4, 10.0, WeaponKind::Greatsword);

    s.press_interact_key(a);

    assert_eq!(s.player(a).loadout.spare(), Some(WeaponKind::Greatsword));
    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
}

#[test]
fn two_presses_in_one_tick_for_the_same_weapon_give_it_to_the_lower_id_only() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    let b = s.player_at(10.0, 10.5);
    for id in [a, b] {
        s.hold(
            id,
            PlayerInput {
                interact_pressed: true,
                ..PlayerInput::default()
            },
        );
    }

    s.run_ticks(1);

    assert_eq!(s.player(a).loadout.spare(), Some(WeaponKind::Spear));
    assert_eq!(s.player(b).loadout.spare(), None);
    assert!(weapons_on_the_ground(&s).is_empty());
}

#[test]
fn a_press_while_a_downed_ally_is_in_reach_revives_instead_of_taking_the_weapon() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    let b = s.player_at(10.9, 10.0);
    s.knock_down(b);

    s.press_interact_key(a);

    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Reviving { .. })
    ));
}

#[test]
fn a_tap_beside_a_downed_ally_neither_revives_nor_takes_the_weapon() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    let b = s.player_at(10.9, 10.0);
    s.knock_down(b);
    s.hold(
        a,
        PlayerInput {
            interact_pressed: true,
            ..PlayerInput::default()
        },
    );

    s.run_ticks(3);

    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
    assert_eq!(s.player(a).interaction(), None);
}

#[test]
fn a_downed_player_and_a_busy_player_cannot_take_a_weapon() {
    let mut s = Scenario::open_field();
    let downed = weapon_beside(&mut s, WeaponKind::Spear);
    s.knock_down(downed);
    s.press_interact_key(downed);
    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);

    let mut s = Scenario::open_field();
    let reviver = weapon_beside(&mut s, WeaponKind::Spear);
    let b = s.player_at(10.9, 10.0);
    s.knock_down(b);
    s.hold_interact(reviver);
    s.run_ticks(2);
    s.press_interact_key(reviver);
    assert_eq!(
        weapons_on_the_ground(&s),
        [WeaponKind::Spear],
        "the reviver is busy"
    );
}

#[test]
fn passing_weapons_around_neither_makes_nor_loses_any() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    let b = s.player_at(10.0, 10.6);
    s.arm(b, WeaponKind::Sword, Some(WeaponKind::Bow));
    let before = all_weapons(&s);

    for _ in 0..6 {
        s.press_interact_key(a);
        s.press_interact_key(b);
        s.tap_switch_key(a);
        assert_eq!(all_weapons(&s), before);
    }
}

/// 바닥에 놓인 무기를 종류와 자리로 모으고 종류 순서로 정렬합니다.
fn ground_with_places(s: &Scenario) -> Vec<(WeaponKind, f32, f32)> {
    let mut found: Vec<_> = s
        .sim
        .ground_weapons()
        .map(|weapon| (weapon.kind, weapon.pos.x, weapon.pos.y))
        .collect();
    found.sort_by_key(|(kind, ..)| *kind);
    found
}

#[test]
fn a_player_who_leaves_drops_the_weapons_from_chests_where_they_stood() {
    let mut s = Scenario::open_field();
    let a = s.player_at(6.0, 7.0);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Spear));

    s.sim.remove_player(a);

    assert_eq!(
        ground_with_places(&s),
        [(WeaponKind::Bow, 6.0, 7.0), (WeaponKind::Spear, 6.0, 7.0)]
    );
}

#[test]
fn a_player_who_leaves_keeps_the_sword_the_start_loadout_gives_in_either_slot() {
    let mut s = Scenario::open_field();
    let held = s.player_at(6.0, 7.0);
    s.arm(held, WeaponKind::Sword, Some(WeaponKind::Spear));
    let spare = s.player_at(12.0, 7.0);
    s.arm(spare, WeaponKind::Greatsword, Some(WeaponKind::Sword));

    s.sim.remove_player(held);
    s.sim.remove_player(spare);

    assert_eq!(
        ground_with_places(&s),
        [
            (WeaponKind::Greatsword, 12.0, 7.0),
            (WeaponKind::Spear, 6.0, 7.0)
        ],
        "only the weapons the start loadout does not give are left behind"
    );
}

#[test]
fn what_stays_with_a_leaver_is_what_the_start_loadout_gives_not_the_sword_by_name() {
    let mut s = Scenario::with_tuning(|t| {
        t.player.start_loadout = Loadout::new(WeaponKind::Bow, Some(WeaponKind::Greatsword));
    });
    let a = s.player_at(6.0, 7.0);
    s.arm(a, WeaponKind::Sword, Some(WeaponKind::Bow));
    let b = s.player_at(12.0, 7.0);
    s.arm(b, WeaponKind::Greatsword, Some(WeaponKind::Bow));

    s.sim.remove_player(a);
    s.sim.remove_player(b);

    assert_eq!(
        ground_with_places(&s),
        [(WeaponKind::Sword, 6.0, 7.0)],
        "the sword is not given by this start loadout, the bow and the greatsword are"
    );
}

#[test]
fn a_player_who_leaves_with_only_the_starting_sword_leaves_no_weapon() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);

    s.sim.remove_player(a);

    assert!(weapons_on_the_ground(&s).is_empty());
}

#[test]
fn joining_and_leaving_over_and_over_with_the_start_loadout_never_puts_a_weapon_on_the_ground() {
    let mut s = Scenario::open_field();
    for round in 0..50 {
        let crowd: Vec<PlayerId> = (0..4)
            .map(|_| s.sim.add_player().expect("the room has space"))
            .collect();
        s.run_ticks(3);
        for id in crowd {
            s.sim.remove_player(id);
        }
        assert!(
            weapons_on_the_ground(&s).is_empty(),
            "a weapon lies on the ground after {} joins and leaves",
            round + 1
        );
    }
}

#[test]
fn a_teammate_can_take_the_weapon_a_leaver_dropped() {
    let mut s = Scenario::open_field();
    let leaver = s.player_at(10.0, 10.0);
    s.arm(leaver, WeaponKind::Sword, Some(WeaponKind::Spear));
    let stays = s.player_at(10.4, 10.0);
    s.sim.remove_player(leaver);

    s.press_interact_key(stays);

    assert_eq!(
        s.player(stays).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Spear))
    );
    assert!(weapons_on_the_ground(&s).is_empty());
}

#[test]
fn a_downed_player_keeps_both_weapons_and_the_ground_keeps_its_own() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Greatsword));
    s.knock_down(a);

    s.run_ticks(5);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Bow, Some(WeaponKind::Greatsword))
    );
    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
}

#[test]
fn a_weapon_on_the_ground_stays_through_the_night_and_the_next_day() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 1.0;
        t.nights.plans = nights(&[&[1], &[1]]);
        t.nights.first_wave_delay_secs = 0.05;
        t.wagon.max_hp = u32::MAX;
    });
    s.weapon_at(5.0, 5.0, WeaponKind::Spear);

    s.run_until(100, |sim| matches!(sim.phase(), Phase::Night { .. }));
    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
    s.run_until(2000, |sim| {
        sim.monsters().count() == 1 && matches!(sim.phase(), Phase::Night { .. })
    });
    s.clear_monsters();
    s.run_until(400, |sim| matches!(sim.phase(), Phase::Day { day: 2, .. }));

    assert_eq!(weapons_on_the_ground(&s), [WeaponKind::Spear]);
}

#[test]
fn a_new_expedition_clears_the_weapons_on_the_ground() {
    let mut s = Scenario::open_field();
    s.weapon_at(5.0, 5.0, WeaponKind::Spear);
    s.sim.set_phase(Phase::Over {
        outcome: Outcome::Defeat,
        ticks_left: 1,
    });

    s.run_until(5, |sim| sim.expedition_number() == 2);

    assert!(weapons_on_the_ground(&s).is_empty());
}

#[test]
fn the_prompt_names_the_weapon_f_would_take_and_is_silent_without_one_in_reach() {
    let mut s = Scenario::open_field();
    let near = s.player_at(12.0, 12.0);
    let far = s.player_at(3.0, 3.0);
    assert_eq!(s.sim.prompt(near), None);

    s.weapon_at(12.4, 12.0, WeaponKind::Spear);

    assert_eq!(
        s.sim.prompt(near),
        Some(Prompt::TakeWeapon(WeaponKind::Spear))
    );
    assert_eq!(s.sim.prompt(far), None);
}

#[test]
fn the_prompt_is_silent_when_a_revive_comes_first_or_the_player_is_busy_or_downed() {
    let mut s = Scenario::open_field();
    let a = weapon_beside(&mut s, WeaponKind::Spear);
    let b = s.player_at(10.9, 10.0);
    s.knock_down(b);
    assert_eq!(s.sim.prompt(a), None, "a revive is in reach");

    s.hold_interact(a);
    s.run_ticks(2);
    assert_eq!(s.sim.prompt(a), None, "already reviving");
    assert_eq!(s.sim.prompt(b), None, "downed");
}

#[test]
fn the_prompt_ignores_a_downed_ally_whom_another_player_is_already_reviving() {
    let mut s = Scenario::open_field();
    let reviver = s.player_at(10.0, 10.0);
    let helper = s.player_at(10.0, 10.8);
    let downed = s.player_at(10.9, 10.4);
    s.weapon_at(10.0, 10.5, WeaponKind::Spear);
    s.knock_down(downed);
    s.hold_interact(reviver);
    s.run_ticks(2);
    assert!(matches!(
        s.player(reviver).interaction(),
        Some(Interaction::Reviving { .. })
    ));

    assert_eq!(
        s.sim.prompt(helper),
        Some(Prompt::TakeWeapon(WeaponKind::Spear)),
        "the revive is taken, so a press would take the weapon"
    );
    s.press_interact_key(helper);
    assert_eq!(s.player(helper).loadout.spare(), Some(WeaponKind::Spear));
}
