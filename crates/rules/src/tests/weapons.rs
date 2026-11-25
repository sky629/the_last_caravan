//! 플레이어가 가진 무기를 확인합니다: 처음 가진 무기, `Q`로 바꾸기, 바꿀 수 없는 때, 쓰러짐과 새 날과 새 원정에서의 무기.
//! 시험하는 것은 `&Sim`이 공개하는 값(`Player::loadout`, `Sim::projectiles`)뿐입니다.

use super::support::Scenario;
use crate::expedition::{Outcome, Phase};
use crate::ids::PlayerId;
use crate::input::{AimDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::loadout::Loadout;
use crate::player::{Interaction, LastAttack};
use crate::world::Tile;

fn attack_east() -> PlayerInput {
    PlayerInput {
        aim: AimDir::new(1.0, 0.0),
        attack: true,
        ..PlayerInput::default()
    }
}

/// 몬스터가 한 번에 죽지 않고 제자리에 서 있는 시나리오입니다.
fn quiet_field() -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
    })
}

/// 검을 들고 활을 다른 칸에 가진 플레이어를 `(10, 10)`에 놓습니다.
fn armed_with_sword_and_bow(s: &mut Scenario) -> PlayerId {
    let a = s.player_at(10.0, 10.0);
    s.arm(a, WeaponKind::Sword, Some(WeaponKind::Bow));
    a
}

#[test]
fn a_player_starts_with_the_sword_and_an_empty_second_slot() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);

    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Sword, None));
}

#[test]
fn the_starting_loadout_is_the_one_the_tuning_names() {
    let mut s = Scenario::with_tuning(|t| {
        t.player.start_loadout = Loadout::new(WeaponKind::Bow, Some(WeaponKind::Sword));
    });
    let a = s.player_at(10.0, 10.0);

    assert_eq!(s.player(a).weapon(), WeaponKind::Bow);
    assert_eq!(s.player(a).loadout.spare(), Some(WeaponKind::Sword));
}

#[test]
fn q_changes_nothing_while_only_one_slot_is_filled() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);

    s.tap_switch_key(a);

    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Sword, None));
}

#[test]
fn each_press_of_q_switches_between_the_two_weapons() {
    let mut s = Scenario::open_field();
    let a = armed_with_sword_and_bow(&mut s);

    s.tap_switch_key(a);
    assert_eq!(s.player(a).weapon(), WeaponKind::Bow);
    assert_eq!(s.player(a).loadout.spare(), Some(WeaponKind::Sword));

    s.tap_switch_key(a);
    assert_eq!(s.player(a).weapon(), WeaponKind::Sword);

    s.tap_switch_key(a);
    assert_eq!(s.player(a).weapon(), WeaponKind::Bow);
}

#[test]
fn the_weapon_does_not_change_without_a_press() {
    let mut s = Scenario::open_field();
    let a = armed_with_sword_and_bow(&mut s);
    s.hold(a, attack_east());

    s.run_ticks(40);

    assert_eq!(s.player(a).weapon(), WeaponKind::Sword);
}

#[test]
fn a_switch_only_changes_the_player_who_pressed_it() {
    let mut s = Scenario::open_field();
    let a = armed_with_sword_and_bow(&mut s);
    let b = s.player_at(12.0, 10.0);
    s.arm(b, WeaponKind::Sword, Some(WeaponKind::Bow));

    s.tap_switch_key(a);

    assert_eq!(s.player(a).weapon(), WeaponKind::Bow);
    assert_eq!(s.player(b).weapon(), WeaponKind::Sword);
}

#[test]
fn the_new_weapon_attacks_in_the_tick_of_the_switch() {
    let mut s = quiet_field();
    let a = armed_with_sword_and_bow(&mut s);
    let monster = s.monster_at(10.9, 10.0);
    s.hold(a, attack_east());

    s.tap_switch_key(a);

    let hp = s.monster(monster).expect("the monster is alive").hp;
    assert_eq!(
        hp,
        1000 - s.sim.tuning().weapons.bow.damage,
        "the bow shot, not the sword"
    );
    assert_eq!(s.sim.projectiles().count(), 1);
}

#[test]
fn the_last_attack_remembers_the_weapon_that_attacked_after_a_switch() {
    let mut s = Scenario::open_field();
    let a = armed_with_sword_and_bow(&mut s);
    s.hold(a, attack_east());
    s.run_ticks(1);
    s.hold(a, PlayerInput::default());

    s.tap_switch_key(a);

    assert_eq!(s.player(a).weapon(), WeaponKind::Bow);
    assert_eq!(
        s.player(a).last_attack,
        Some(LastAttack {
            tick: 1,
            weapon: WeaponKind::Sword
        }),
        "the swing of tick 1 was a sword swing"
    );
}

#[test]
fn the_cooldown_carries_over_to_the_other_weapon() {
    let mut s = Scenario::open_field();
    let a = armed_with_sword_and_bow(&mut s);
    s.hold(a, attack_east());
    s.run_ticks(1);
    assert_eq!(
        s.player(a).last_attack.map(|attack| attack.tick),
        Some(1),
        "the sword swung"
    );

    s.tap_switch_key(a);
    s.run_ticks(8);
    assert_eq!(
        s.sim.projectiles().count(),
        0,
        "still cooling down at tick 10"
    );

    s.run_ticks(1);
    assert_eq!(
        s.player(a).last_attack.map(|attack| attack.tick),
        Some(11),
        "the bow shoots at tick 11"
    );
    assert_eq!(s.sim.projectiles().count(), 1);
}

#[test]
fn both_weapons_survive_being_downed_and_revived() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Sword));
    s.knock_down(a);
    s.hold(
        b,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    let before = s.player(a).loadout;

    s.run_until(200, |sim| sim.player(a).is_some_and(|p| p.is_alive()));

    assert_eq!(s.player(a).loadout, before, "revived with both weapons");
}

#[test]
fn both_weapons_survive_a_new_day() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Sword));
    s.sim.set_phase(Phase::Dawn {
        next_day: 2,
        ticks_left: 1,
    });

    s.run_until(5, |sim| matches!(sim.phase(), Phase::Day { day: 2, .. }));

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Bow, Some(WeaponKind::Sword))
    );
}

#[test]
fn a_new_expedition_returns_every_player_to_the_sword_alone() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Sword));
    s.give_weapon(b, WeaponKind::Bow);
    s.sim.set_phase(Phase::Over {
        outcome: Outcome::Victory,
        ticks_left: 1,
    });

    s.run_until(5, |sim| sim.expedition_number() == 2);

    let starting = Loadout::new(WeaponKind::Sword, None);
    assert_eq!(s.player(a).loadout, starting);
    assert_eq!(s.player(b).loadout, starting);
}

/// `q`를 한 번 눌러도 무기가 바뀌지 않아야 하는 상황에서 확인합니다. 두 칸이 모두 차 있으므로 눌렀다면 바뀌었을 상황입니다.
fn assert_cannot_switch(s: &mut Scenario, id: PlayerId) {
    s.arm(id, WeaponKind::Sword, Some(WeaponKind::Bow));

    s.tap_switch_key(id);

    assert_eq!(s.player(id).weapon(), WeaponKind::Sword);
}

#[test]
fn a_player_who_is_reviving_cannot_switch() {
    let mut s = Scenario::open_field();
    let downed = s.player_at(10.0, 10.0);
    let reviver = s.player_at(11.0, 10.0);
    s.knock_down(downed);
    s.hold_interact(reviver);
    s.run_ticks(2);
    assert!(matches!(
        s.player(reviver).interaction(),
        Some(Interaction::Reviving { .. })
    ));

    assert_cannot_switch(&mut s, reviver);
}

#[test]
fn a_player_who_is_repairing_cannot_switch() {
    let mut s = Scenario::open_field();
    let a = s.player_at(16.3, 18.0);
    s.set_wagon_hp(100);
    s.hold_interact(a);
    s.run_ticks(2);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Repairing { .. })
    ));

    assert_cannot_switch(&mut s, a);
}

#[test]
fn a_player_who_is_gathering_cannot_switch() {
    let mut s = Scenario::with_tuning(|t| t.map.nodes = vec![Tile::new(8, 10)]);
    let a = s.player_at(7.3, 10.5);
    s.hold_interact(a);
    s.run_ticks(2);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Gathering { .. })
    ));

    assert_cannot_switch(&mut s, a);
}

#[test]
fn a_player_who_is_building_cannot_switch() {
    let mut s = Scenario::open_field();
    let a = s.player_at(13.5, 15.5);
    s.hold_build(a, 0.0, -1.0);
    s.run_ticks(2);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Building { .. })
    ));

    assert_cannot_switch(&mut s, a);
}

#[test]
fn a_downed_or_dead_player_cannot_switch() {
    let mut s = Scenario::open_field();
    let downed = s.player_at(10.0, 10.0);
    let dead = s.player_at(12.0, 10.0);
    s.knock_down(downed);
    s.kill(dead);

    assert_cannot_switch(&mut s, downed);
    assert_cannot_switch(&mut s, dead);
}

#[test]
fn the_key_pressed_while_busy_is_not_kept_for_later() {
    let mut s = Scenario::open_field();
    let a = s.player_at(13.5, 15.5);
    s.arm(a, WeaponKind::Sword, Some(WeaponKind::Bow));
    s.hold_build(a, 0.0, -1.0);
    s.run_ticks(2);
    s.tap_switch_key(a);

    s.run_until(100, |sim| sim.towers().all(|tower| tower.is_standing()));
    s.hold(a, PlayerInput::default());
    s.run_ticks(5);

    assert!(s.player(a).interaction().is_none(), "the build is finished");
    assert_eq!(
        s.player(a).weapon(),
        WeaponKind::Sword,
        "the press during the build did not apply afterwards"
    );
}
