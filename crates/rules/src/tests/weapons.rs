//! 플레이어가 드는 무기를 확인합니다: 처음 드는 무기, 쓰러짐과 새 날과 새 원정에서의 무기.
//! 시험하는 것은 `&Sim`이 공개하는 값(`Player::weapon`, `Sim::projectiles`)뿐입니다.

use super::support::Scenario;
use crate::expedition::{Outcome, Phase};
use crate::input::PlayerInput;
use crate::kind::WeaponKind;

#[test]
fn a_player_starts_with_the_sword() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);

    assert_eq!(s.player(a).weapon, WeaponKind::Sword);
}

#[test]
fn the_weapon_survives_being_downed_and_revived() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    s.give_weapon(a, WeaponKind::Bow);
    s.knock_down(a);
    s.hold(
        b,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(s.player(a).weapon, WeaponKind::Bow, "downed with the bow");

    s.run_until(200, |sim| sim.player(a).is_some_and(|p| p.is_alive()));

    assert_eq!(s.player(a).weapon, WeaponKind::Bow, "revived with the bow");
}

#[test]
fn the_weapon_survives_a_new_day() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    s.give_weapon(a, WeaponKind::Bow);
    s.sim.set_phase(Phase::Dawn {
        next_day: 2,
        ticks_left: 1,
    });

    s.run_until(5, |sim| matches!(sim.phase(), Phase::Day { day: 2, .. }));

    assert_eq!(s.player(a).weapon, WeaponKind::Bow);
}

#[test]
fn a_new_expedition_returns_every_player_to_the_sword() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    s.give_weapon(a, WeaponKind::Bow);
    s.give_weapon(b, WeaponKind::Bow);
    s.sim.set_phase(Phase::Over {
        outcome: Outcome::Victory,
        ticks_left: 1,
    });

    s.run_until(5, |sim| sim.expedition_number() == 2);

    assert_eq!(s.player(a).weapon, WeaponKind::Sword);
    assert_eq!(s.player(b).weapon, WeaponKind::Sword);
}
