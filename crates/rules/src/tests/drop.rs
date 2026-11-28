//! 든 무기를 버리는 일을 확인합니다: G가 든 무기를 선 자리의 바닥에 놓고 다른 칸의 무기를 드는 것, 버릴 수 없는 때(무기가 하나뿐일 때, 하고 있는 일이 있을 때,
//! 쓰러졌거나 사망했거나 자리를 비웠을 때, 야영지와 결과 화면), 시작 무기가 바닥에 놓이지 않는 것, 바닥의 무기가 늘지 않는 것, 같은 Tick의 F.
//!
//! 시험하는 것은 `&Sim`이 공개하는 값(바닥의 무기, 가진 무기, 안내)뿐입니다.

use super::support::{Scenario, in_camp, outcome};
use crate::expedition::Outcome;
use crate::ids::PlayerId;
use crate::input::PlayerInput;
use crate::kind::WeaponKind;
use crate::loadout::Loadout;
use crate::loot::Prompt;
use crate::math::Vec2;
use crate::player::{Interaction, Life, Presence};
use crate::world::Tile;

/// 바닥에 놓인 무기를 종류와 자리로 모으고 종류 순서로 정렬합니다.
fn ground(s: &Scenario) -> Vec<(WeaponKind, f32, f32)> {
    let mut found: Vec<_> = s
        .sim
        .ground_weapons()
        .map(|weapon| (weapon.kind, weapon.pos.x, weapon.pos.y))
        .collect();
    found.sort_by_key(|(kind, ..)| *kind);
    found
}

/// `(6, 7)`에 서서 창을 들고 활을 다른 칸에 가진 플레이어입니다.
fn holding_spear_and_bow(s: &mut Scenario) -> PlayerId {
    let a = s.player_at(6.0, 7.0);
    s.arm(a, WeaponKind::Spear, Some(WeaponKind::Bow));
    a
}

#[test]
fn g_puts_the_held_weapon_on_the_ground_where_the_player_stands_and_holds_the_other_one() {
    let mut s = Scenario::open_field();
    let a = holding_spear_and_bow(&mut s);

    s.tap_drop_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Bow, None),
        "the other weapon is held now and the slot it left is empty"
    );
    assert_eq!(ground(&s), [(WeaponKind::Spear, 6.0, 7.0)]);
}

#[test]
fn g_changes_nothing_while_only_one_slot_is_filled_so_the_player_is_never_bare_handed() {
    let mut s = Scenario::open_field();
    let a = s.player_at(6.0, 7.0);
    s.give_weapon(a, WeaponKind::Bow);

    s.tap_drop_key(a);
    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Bow, None));
    assert!(ground(&s).is_empty());

    let b = s.player_at(12.0, 7.0);
    s.tap_drop_key(b);
    assert_eq!(s.player(b).loadout, Loadout::new(WeaponKind::Sword, None));
    assert!(ground(&s).is_empty());
}

#[test]
fn a_second_g_after_the_drop_has_nothing_left_to_drop() {
    let mut s = Scenario::open_field();
    let a = holding_spear_and_bow(&mut s);

    s.tap_drop_key(a);
    s.tap_drop_key(a);

    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Bow, None));
    assert_eq!(ground(&s), [(WeaponKind::Spear, 6.0, 7.0)]);
}

#[test]
fn a_dropped_starting_weapon_vanishes_instead_of_lying_on_the_ground() {
    let mut s = Scenario::open_field();
    let a = s.player_at(6.0, 7.0);
    s.arm(a, WeaponKind::Sword, Some(WeaponKind::Bow));

    s.tap_drop_key(a);

    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Bow, None));
    assert!(
        ground(&s).is_empty(),
        "the sword every joining player is given does not lie on the ground"
    );
}

#[test]
fn what_vanishes_in_a_drop_is_what_the_start_loadout_gives_not_the_sword_by_name() {
    let mut s = Scenario::with_tuning(|t| {
        t.player.start_loadout = Loadout::new(WeaponKind::Bow, None);
    });
    let gives_the_bow = s.player_at(6.0, 7.0);
    s.arm(gives_the_bow, WeaponKind::Bow, Some(WeaponKind::Spear));
    let holds_a_sword = s.player_at(12.0, 7.0);
    s.arm(holds_a_sword, WeaponKind::Sword, Some(WeaponKind::Bow));

    s.tap_drop_key(gives_the_bow);
    assert!(ground(&s).is_empty(), "the bow is what this loadout gives");

    s.tap_drop_key(holds_a_sword);
    assert_eq!(ground(&s), [(WeaponKind::Sword, 12.0, 7.0)]);
}

#[test]
fn a_dropped_weapon_is_taken_again_with_f_by_the_one_who_dropped_it_or_by_anyone_else() {
    let mut s = Scenario::open_field();
    let a = holding_spear_and_bow(&mut s);
    let b = s.player_at(6.5, 7.0);
    s.tap_drop_key(a);
    assert_eq!(
        s.sim.prompt(a),
        Some(Prompt::TakeWeapon(WeaponKind::Spear)),
        "F would take it back"
    );

    s.press_interact_key(b);

    assert_eq!(
        s.player(b).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Spear)),
        "a teammate takes it like any other weapon on the ground"
    );
    assert!(ground(&s).is_empty());

    let mut s = Scenario::open_field();
    let a = holding_spear_and_bow(&mut s);
    s.tap_drop_key(a);

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Bow, Some(WeaponKind::Spear)),
        "the empty slot takes it without a swap"
    );
    assert!(ground(&s).is_empty());
}

/// `(10, 10)`에 `held`를 가진 플레이어가 서고, 그 동쪽 0.5타일에 대검이 놓여 있습니다. 한 Tick 동안 G와 F를 함께 누릅니다.
fn press_g_and_f_together(held: Loadout) -> (Scenario, PlayerId) {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    s.arm(a, held.held(), held.spare());
    s.weapon_at(10.5, 10.0, WeaponKind::Greatsword);
    s.hold(
        a,
        PlayerInput {
            drop_weapon: true,
            interact: true,
            interact_pressed: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(1);
    (s, a)
}

#[test]
fn g_and_f_in_the_same_tick_only_drop_and_do_not_take_the_weapon_back_or_another() {
    let (s, a) = press_g_and_f_together(Loadout::new(WeaponKind::Spear, Some(WeaponKind::Bow)));

    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Bow, None));
    assert_eq!(
        ground(&s),
        [
            (WeaponKind::Greatsword, 10.5, 10.0),
            (WeaponKind::Spear, 10.0, 10.0)
        ],
        "the spear lies where the player stood and the greatsword was not touched"
    );
}

#[test]
fn f_takes_the_weapon_as_usual_when_g_had_nothing_to_drop() {
    let (s, a) = press_g_and_f_together(Loadout::new(WeaponKind::Sword, None));

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Greatsword))
    );
    assert!(ground(&s).is_empty());
}

#[test]
fn g_only_changes_the_player_who_pressed_it() {
    let mut s = Scenario::open_field();
    let a = holding_spear_and_bow(&mut s);
    let b = s.player_at(12.0, 7.0);
    s.arm(b, WeaponKind::Greatsword, Some(WeaponKind::Rifle));

    s.tap_drop_key(a);

    assert_eq!(s.player(a).weapon(), WeaponKind::Bow);
    assert_eq!(
        s.player(b).loadout,
        Loadout::new(WeaponKind::Greatsword, Some(WeaponKind::Rifle))
    );
    assert_eq!(ground(&s), [(WeaponKind::Spear, 6.0, 7.0)]);
}

#[test]
fn two_players_who_drop_in_the_same_tick_each_leave_their_own_weapon_where_they_stood() {
    let mut s = Scenario::open_field();
    let a = holding_spear_and_bow(&mut s);
    let b = s.player_at(12.0, 7.0);
    s.arm(b, WeaponKind::Greatsword, Some(WeaponKind::Rifle));
    for id in [a, b] {
        s.hold(
            id,
            PlayerInput {
                drop_weapon: true,
                ..PlayerInput::default()
            },
        );
    }

    s.run_ticks(1);

    let ids: Vec<_> = s.sim.ground_weapons().map(|weapon| weapon.id).collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "each is a weapon of its own");
    assert_eq!(
        ground(&s),
        [
            (WeaponKind::Greatsword, 12.0, 7.0),
            (WeaponKind::Spear, 6.0, 7.0)
        ]
    );
}

/// 창과 활을 가진 플레이어에게 G를 한 번 눌러도 아무것도 바뀌지 않아야 하는 상황에서 확인합니다. 눌렀다면 창이 바닥에 놓였을 상황입니다.
fn assert_cannot_drop(s: &mut Scenario, id: PlayerId) {
    s.arm(id, WeaponKind::Spear, Some(WeaponKind::Bow));

    s.tap_drop_key(id);

    assert_eq!(
        s.player(id).loadout,
        Loadout::new(WeaponKind::Spear, Some(WeaponKind::Bow))
    );
    assert!(ground(s).is_empty());
}

#[test]
fn a_player_who_is_reviving_cannot_drop() {
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

    assert_cannot_drop(&mut s, reviver);
}

#[test]
fn a_player_who_is_repairing_cannot_drop() {
    let mut s = Scenario::open_field();
    let a = s.player_at(16.3, 18.0);
    s.set_wagon_hp(100);
    s.hold_interact(a);
    s.run_ticks(2);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Repairing { .. })
    ));

    assert_cannot_drop(&mut s, a);
}

#[test]
fn a_player_who_is_gathering_cannot_drop() {
    let mut s = Scenario::with_tuning(|t| t.map.nodes = vec![Tile::new(8, 10)]);
    let a = s.player_at(7.3, 10.5);
    s.hold_interact(a);
    s.run_ticks(2);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Gathering { .. })
    ));

    assert_cannot_drop(&mut s, a);
}

#[test]
fn a_player_who_is_building_cannot_drop() {
    let mut s = Scenario::open_field();
    let a = s.player_at(13.5, 15.5);
    s.hold_build(a, 0.0, -1.0);
    s.run_ticks(2);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Building { .. })
    ));

    assert_cannot_drop(&mut s, a);
}

#[test]
fn a_player_who_is_opening_a_chest_cannot_drop() {
    let mut s = Scenario::with_chest(WeaponKind::Rifle, 0);
    let a = s.player_at(2.5, 3.6);
    s.hold_interact(a);
    s.run_ticks(2);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Opening { .. })
    ));

    assert_cannot_drop(&mut s, a);
}

#[test]
fn a_player_who_is_being_pushed_cannot_drop() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    if let Life::Alive { interaction, .. } =
        &mut s.sim.world_mut().players.get_mut(&a).unwrap().life
    {
        *interaction = Some(Interaction::Knocked {
            step: Vec2::new(0.1, 0.0),
            ticks_left: 5,
        });
    }

    assert_cannot_drop(&mut s, a);
}

#[test]
fn a_downed_or_dead_or_away_player_cannot_drop() {
    let mut s = Scenario::open_field();
    let downed = s.player_at(10.0, 10.0);
    let dead = s.player_at(12.0, 10.0);
    let away = s.player_at(14.0, 10.0);
    s.knock_down(downed);
    s.kill(dead);
    s.sim.set_presence(away, Presence::Away);

    assert_cannot_drop(&mut s, downed);
    assert_cannot_drop(&mut s, dead);
    assert_cannot_drop(&mut s, away);
}

#[test]
fn g_does_nothing_in_the_camp_where_no_weapon_may_pile_up() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(6.0, 10.0);
    assert!(in_camp(&s.sim));

    assert_cannot_drop(&mut s, a);
}

#[test]
fn g_does_nothing_on_the_result_screen() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(2.0, 2.0);
    s.sim.begin_expedition();
    s.set_wagon_hp(0);
    s.run_ticks(1);
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));

    assert_cannot_drop(&mut s, a);
}

/// 상자에서 나온 무기의 수입니다. 기본 수치에서 하루에 상자가 3개이고 원정이 사흘이므로, 바닥의 무기는 9개를 넘을 수 없습니다.
/// 스냅샷의 크기 상한(`snapshot_size.rs`의 `MOST_WEAPONS_ON_THE_GROUND`)이 이 수를 전제로 합니다.
const CHEST_WEAPONS: usize = 9;

/// 바닥의 무기와 플레이어가 든 무기 가운데 시작 무기가 아닌 것을 모두 센 수입니다. 검이 바닥에 놓이면 바닥의 무기로 세어져서 이 수가 늘어납니다.
fn weapons_in_play(s: &Scenario) -> usize {
    let start = s.sim.tuning().player.start_loadout;
    let held = s
        .sim
        .players()
        .flat_map(|player| player.loadout.weapons())
        .filter(|kind| !start.holds(*kind))
        .count();
    held + s.sim.ground_weapons().count()
}

#[test]
fn dropping_taking_and_leaving_over_and_over_never_puts_more_on_the_ground_than_the_chests_gave() {
    let mut s = Scenario::open_field();
    let kinds = [
        WeaponKind::Bow,
        WeaponKind::Greatsword,
        WeaponKind::Spear,
        WeaponKind::Crossbow,
        WeaponKind::Rifle,
    ];
    for index in 0..CHEST_WEAPONS {
        s.weapon_at(10.0 + 0.05 * index as f32, 10.0, kinds[index % kinds.len()]);
    }
    assert_eq!(weapons_in_play(&s), CHEST_WEAPONS);

    for round in 0..50 {
        let a = s.player_at(10.0, 10.0);
        for step in 0..3 {
            s.press_interact_key(a);
            s.tap_drop_key(a);
            assert_eq!(
                weapons_in_play(&s),
                CHEST_WEAPONS,
                "neither made nor lost in round {round}, step {step}"
            );
            assert!(
                s.sim.ground_weapons().count() <= CHEST_WEAPONS,
                "the snapshot holds at most {CHEST_WEAPONS} weapons on the ground"
            );
            assert!(
                s.sim
                    .ground_weapons()
                    .all(|weapon| weapon.kind != WeaponKind::Sword),
                "the starting sword is never left on the ground"
            );
        }
        s.sim.remove_player(a);
        assert_eq!(weapons_in_play(&s), CHEST_WEAPONS, "after leaving");
    }
}
