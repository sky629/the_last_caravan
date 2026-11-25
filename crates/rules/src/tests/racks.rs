//! 야영지의 무기 진열대를 확인합니다: 집으면 진열대는 그대로이고 내준 무기는 사라지는 것, 이미 가진 종류는 집지 않는 것,
//! 가장 가까운 진열대를 고르는 것, 바닥의 무기와 거리가 같을 때의 우선순위, 같은 Tick에 여럿이 집는 것, F의 안내.
//!
//! 시나리오는 `Scenario::camp`입니다. 진열대 여섯이 (4, 15)부터 동쪽으로 2타일 간격으로 서고,
//! 순서는 검 (4, 15), 활 (6, 15), 대검 (8, 15), 창 (10, 15), 석궁 (12, 15), 소총 (14, 15)입니다.
//! 무기를 집을 수 있는 거리(`loot.take_reach`)는 1.75타일이고, 플레이어의 반지름은 0.35타일입니다.

use super::support::Scenario;
use crate::ids::PlayerId;
use crate::input::PlayerInput;
use crate::kind::WeaponKind;
use crate::loadout::Loadout;
use crate::loot::Prompt;
use crate::math::Vec2;

/// 활의 진열대 (6, 15)에서 북쪽으로 0.5타일 떨어진 곳에 선 플레이어입니다. 활의 진열대가 가장 가깝습니다.
fn beside_the_bow_rack(s: &mut Scenario) -> PlayerId {
    s.player_at(6.0, 14.5)
}

#[test]
fn the_camp_has_one_rack_for_each_weapon_in_the_order_of_the_kinds() {
    let s = Scenario::camp(|_| {});

    let racks: Vec<_> = s.sim.racks().map(|rack| rack.kind).collect();

    assert_eq!(racks, WeaponKind::ALL.to_vec());
}

#[test]
fn one_press_of_f_takes_the_weapon_of_the_nearest_rack_and_the_rack_stays() {
    let mut s = Scenario::camp(|_| {});
    let a = beside_the_bow_rack(&mut s);

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Bow))
    );
    assert_eq!(s.sim.racks().count(), 6, "the rack keeps its weapon");
    assert_eq!(s.sim.ground_weapons().count(), 0);
}

#[test]
fn with_both_slots_full_the_held_weapon_is_given_up_and_does_not_land_on_the_ground() {
    let mut s = Scenario::camp(|_| {});
    let a = beside_the_bow_rack(&mut s);
    s.arm(a, WeaponKind::Spear, Some(WeaponKind::Greatsword));

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Bow, Some(WeaponKind::Greatsword))
    );
    assert_eq!(
        s.sim.ground_weapons().count(),
        0,
        "the given-up weapon is not left to pile up in the camp"
    );
    assert_eq!(s.sim.racks().count(), 6);
}

#[test]
fn the_rack_of_a_weapon_the_player_holds_is_skipped_for_the_next_rack_in_reach() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(4.0, 14.5);
    assert_eq!(s.player(a).loadout.held(), WeaponKind::Sword);

    assert_eq!(
        s.sim.prompt(a),
        Some(Prompt::TakeWeapon(WeaponKind::Bow)),
        "the sword rack is the nearest, but the sword is already held"
    );
    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Bow))
    );
}

#[test]
fn a_player_who_holds_the_weapon_of_the_only_rack_in_reach_is_offered_nothing() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(15.0, 15.0);
    s.arm(a, WeaponKind::Rifle, Some(WeaponKind::Sword));
    assert_eq!(s.sim.prompt(a), None);

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Rifle, Some(WeaponKind::Sword))
    );
}

#[test]
fn a_rack_out_of_reach_gives_nothing() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(6.0, 10.0);
    assert_eq!(s.sim.prompt(a), None);

    s.press_interact_key(a);

    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Sword, None));
}

#[test]
fn the_prompt_names_the_weapon_of_the_nearest_rack() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(8.6, 14.5);

    assert_eq!(
        s.sim.prompt(a),
        Some(Prompt::TakeWeapon(WeaponKind::Greatsword)),
        "the greatsword rack at (8, 15) is nearer than the spear rack at (10, 15)"
    );
    s.move_player(a, Vec2::new(9.4, 14.5));
    assert_eq!(s.sim.prompt(a), Some(Prompt::TakeWeapon(WeaponKind::Spear)));
}

#[test]
fn a_weapon_on_the_ground_wins_a_tie_with_a_rack() {
    let mut s = Scenario::camp(|_| {});
    let a = beside_the_bow_rack(&mut s);
    s.weapon_at(6.0, 15.0, WeaponKind::Spear);

    assert_eq!(s.sim.prompt(a), Some(Prompt::TakeWeapon(WeaponKind::Spear)));
    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Spear))
    );
    assert_eq!(s.sim.ground_weapons().count(), 0);
}

#[test]
fn two_players_take_from_the_same_rack_in_the_same_tick() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(6.0, 14.5);
    let b = s.player_at(6.0, 14.3);
    for id in [a, b] {
        s.hold(
            id,
            PlayerInput {
                interact: true,
                interact_pressed: true,
                ..PlayerInput::default()
            },
        );
    }

    s.run_ticks(1);

    for id in [a, b] {
        assert_eq!(
            s.player(id).loadout,
            Loadout::new(WeaponKind::Sword, Some(WeaponKind::Bow))
        );
    }
}
