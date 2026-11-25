//! 상자를 여는 일을 확인합니다: F를 누르고 있는 시간, 여는 동안 하지 못하는 일, 진행이 처음으로 돌아가는 경우, 지키는 몬스터와 동료가 있을 때,
//! F의 우선순위, 열린 뒤에 놓이는 무기, 두 명이 같은 상자를 여는 경우, 화면에 알리는 안내와 진행도.
//!
//! 시나리오는 `Scenario::with_chest`입니다. 후보 자리 (2, 2)의 상자가 `contents` 하나만 낼 수 있고, 플레이어는 상자의 남쪽 (2.5, 3.6)에 섭니다.
//! 플레이어의 가장자리에서 상자의 가장자리까지 0.25타일이므로 닿는 거리(1타일) 안입니다.

use super::support::{Scenario, nights, phase};
use crate::expedition::Phase;
use crate::ids::PlayerId;
use crate::input::{MoveDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::loadout::Loadout;
use crate::loot::Prompt;
use crate::math::Vec2;
use crate::player::Interaction;
use crate::world::Tile;

/// 상자의 남쪽에 플레이어를 놓습니다.
fn beside_the_chest(s: &mut Scenario) -> PlayerId {
    s.player_at(2.5, 3.6)
}

/// 상자를 여는 데 필요한 Tick 수입니다.
fn required_ticks(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().loot.open_secs)
}

/// F를 눌렀다가 Tick이 오기 전에 떼는 한 번의 두드림입니다. 이 함수가 한 Tick을 진행하고, 끝나면 아무 키도 누르고 있지 않습니다.
fn tap_f(s: &mut Scenario, id: PlayerId) {
    s.hold(
        id,
        PlayerInput {
            interact_pressed: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(1);
    s.hold(id, PlayerInput::default());
}

fn chests_left(s: &Scenario) -> usize {
    s.sim.chests().count()
}

fn weapons_on_the_ground(s: &Scenario) -> Vec<(Vec2, WeaponKind)> {
    s.sim
        .ground_weapons()
        .map(|weapon| (weapon.pos, weapon.kind))
        .collect()
}

#[test]
fn holding_f_for_the_set_time_opens_the_chest_and_puts_its_weapon_on_the_chest_tile() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    let required = required_ticks(&s);
    assert_eq!(required, 30, "1.5 seconds at 20 ticks a second");
    s.hold_interact(a);

    s.run_ticks(required - 1);
    assert_eq!(chests_left(&s), 1, "one tick short");
    assert!(weapons_on_the_ground(&s).is_empty());

    s.run_ticks(1);
    assert_eq!(chests_left(&s), 0);
    assert_eq!(
        weapons_on_the_ground(&s),
        [(Tile::new(2, 2).center(), WeaponKind::Spear)],
        "the weapon lies in the middle of the tile the chest stood on"
    );
    assert!(s.player(a).interaction().is_none(), "the opening is over");
}

#[test]
fn the_progress_counts_the_ticks_f_was_held_and_is_shown_as_a_fraction() {
    let mut s = Scenario::with_chest(WeaponKind::Bow, 0);
    let a = beside_the_chest(&mut s);
    let chest = s.sim.chests().next().unwrap().id;
    s.hold_interact(a);

    s.run_ticks(15);

    assert_eq!(
        s.player(a).interaction(),
        Some(&Interaction::Opening {
            chest,
            elapsed_ticks: 15
        })
    );
    assert_eq!(s.sim.open_progress(a), Some(0.5));
}

#[test]
fn the_opened_tile_is_free_again() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    s.hold_interact(a);
    s.run_ticks(required_ticks(&s));
    s.hold_move(a, 0.0, -1.0);

    s.run_ticks(30);

    assert!(
        s.player(a).pos.y < 2.9,
        "the player walked onto the tile where the chest stood, at {:?}",
        s.player(a).pos
    );
}

#[test]
fn while_opening_the_player_can_neither_walk_nor_attack() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    let before = s.player(a).pos;
    s.hold(
        a,
        PlayerInput {
            interact: true,
            attack: true,
            move_dir: MoveDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );

    s.run_ticks(10);

    assert_eq!(s.player(a).pos, before, "the player did not move");
    assert!(
        s.player(a).last_attack.is_none(),
        "the player did not attack"
    );
}

#[test]
fn releasing_f_sends_the_progress_back_to_zero() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    let required = required_ticks(&s);
    s.hold_interact(a);
    s.run_ticks(required - 5);
    assert!(
        s.sim
            .open_progress(a)
            .is_some_and(|progress| progress > 0.8)
    );

    s.hold(a, PlayerInput::default());
    s.run_ticks(1);
    assert_eq!(s.sim.open_progress(a), None, "released");

    s.hold_interact(a);
    s.run_ticks(5);
    assert_eq!(chests_left(&s), 1, "five more ticks do not finish it");
    assert_eq!(
        s.sim.open_progress(a),
        Some(5.0 / required as f32),
        "it counts from the start again"
    );
    s.run_ticks(required - 5);
    assert_eq!(chests_left(&s), 0);
}

/// 플레이어 (2.5, 3.6)을 향해 몬스터 한 마리가 천천히 걸어와서 처음으로 때리는 Tick을 재는 시나리오입니다.
/// 몬스터는 (2.5, 6.5)에서 시작하고 초당 0.5타일로 걸으므로 60 Tick이 넘게 걸립니다.
fn slow_attacker() -> Scenario {
    let mut s = Scenario::with_tuning(|t| {
        super::support::chest_tuning(t, WeaponKind::Spear, 0);
        t.monsters.grunt.speed = 0.5;
    });
    s.monster_at(2.5, 6.5);
    s
}

/// 아무것도 누르지 않는 플레이어가 처음으로 맞는 Tick의 번호(1부터)입니다.
fn tick_of_the_first_hit() -> u32 {
    let mut s = slow_attacker();
    let a = beside_the_chest(&mut s);
    let full = s.player(a).hp();
    let mut tick = 0;
    while s.player(a).hp() == full {
        s.run_ticks(1);
        tick += 1;
        assert!(tick < 400, "the monster never hit");
    }
    tick
}

/// 첫 피해가 들어오는 Tick에서 `ticks_before_the_hit`번째 앞의 Tick부터 F를 누르기 시작합니다.
/// 돌려주는 값은 첫 피해가 들어온 Tick까지 진행한 시나리오와 플레이어입니다.
fn open_while_the_first_hit_lands(ticks_before_the_hit: u32) -> (Scenario, PlayerId) {
    let first_hit = tick_of_the_first_hit();
    assert!(
        first_hit > ticks_before_the_hit,
        "the monster is slow enough"
    );
    let mut s = slow_attacker();
    let a = beside_the_chest(&mut s);
    s.run_ticks(first_hit - ticks_before_the_hit);
    s.hold_interact(a);
    s.run_ticks(ticks_before_the_hit);
    (s, a)
}

#[test]
fn a_hit_resets_the_progress_even_when_the_player_survives() {
    let (s, a) = open_while_the_first_hit_lands(10);

    assert!(s.player(a).hp() < 100, "the monster hit");
    assert!(s.player(a).is_alive(), "and the player is still standing");
    assert_eq!(s.player(a).interaction(), None, "the opening was dropped");
    assert_eq!(s.sim.open_progress(a), None);
}

#[test]
fn a_hit_on_the_tick_the_opening_would_finish_cancels_it() {
    let required = {
        let s = Scenario::with_chest(WeaponKind::Spear, 0);
        required_ticks(&s)
    };
    let (s, a) = open_while_the_first_hit_lands(required);

    assert!(
        s.player(a).hp() < 100,
        "the hit landed on the finishing tick"
    );
    assert_eq!(chests_left(&s), 1, "the chest stays closed");
    assert!(
        weapons_on_the_ground(&s).is_empty(),
        "and nothing is dropped"
    );
}

#[test]
fn the_progress_that_was_lost_to_a_hit_starts_again_from_one_tick() {
    let (mut s, a) = open_while_the_first_hit_lands(10);
    let chest = s.sim.chests().next().unwrap().id;

    s.run_ticks(1);

    assert_eq!(
        s.player(a).interaction(),
        Some(&Interaction::Opening {
            chest,
            elapsed_ticks: 1
        }),
        "F is still held, so a new opening starts at one tick"
    );
}

#[test]
fn living_guards_do_not_stop_the_opening_while_they_do_not_hit_the_opener() {
    let mut s = Scenario::with_tuning(|t| {
        super::support::chest_tuning(t, WeaponKind::Greatsword, 3);
        // 지키는 몬스터는 자리에서 이 거리 안에 들어온 플레이어만 쫓습니다. 0이면 자리에 선 채로 아무도 때리지 않습니다.
        t.guards.chase_range = 0.0;
    });
    let a = beside_the_chest(&mut s);
    assert_eq!(s.sim.monsters().count(), 3, "three guards beside the chest");
    s.hold_interact(a);

    s.run_ticks(required_ticks(&s));

    assert_eq!(chests_left(&s), 0);
    assert_eq!(s.sim.monsters().count(), 3, "all three are still alive");
    assert_eq!(weapons_on_the_ground(&s).len(), 1);
}

#[test]
fn a_downed_ally_beside_the_opener_does_not_stop_the_opening_and_then_f_revives() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    let b = s.player_at(3.4, 3.6);
    s.hold_interact(a);
    s.run_ticks(10);

    s.knock_down(b);
    s.run_ticks(required_ticks(&s) - 10);

    assert_eq!(chests_left(&s), 0, "the opening went on and finished");
    assert!(s.player(a).interaction().is_none());

    s.run_ticks(1);
    assert!(
        matches!(
            s.player(a).interaction(),
            Some(Interaction::Reviving { target, .. }) if *target == b
        ),
        "F is still held, so the revive starts now"
    );
}

#[test]
fn f_picks_a_downed_ally_before_a_chest() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    let b = s.player_at(3.4, 3.6);
    s.knock_down(b);
    s.hold_interact(a);

    s.run_ticks(1);

    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Reviving { target, .. }) if *target == b
    ));
}

#[test]
fn f_repairs_a_damaged_tower_before_it_opens_a_chest() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    s.tower_at(3, 3);
    s.set_tower_hp(10);
    s.hold_interact(a);

    s.run_ticks(1);

    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Repairing { .. })
    ));
}

#[test]
fn f_gathers_from_a_node_before_it_opens_a_chest_and_opens_it_when_the_node_is_empty() {
    let mut s = Scenario::with_tuning(|t| {
        super::support::chest_tuning(t, WeaponKind::Spear, 0);
        t.map.nodes = vec![Tile::new(3, 3)];
    });
    let a = beside_the_chest(&mut s);
    s.hold_interact(a);

    s.run_ticks(1);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Gathering { .. })
    ));

    let node = s.sim.nodes().next().unwrap().id;
    s.set_node_amount(node, 0);
    s.run_ticks(1);
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Opening { .. })
    ));
}

#[test]
fn a_chest_comes_before_a_weapon_on_the_ground() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    s.weapon_at(1.6, 3.6, WeaponKind::Bow);
    assert_eq!(s.sim.prompt(a), Some(Prompt::OpenChest));

    tap_f(&mut s, a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Sword, None),
        "a press beside a chest does not take the weapon"
    );
    assert_eq!(weapons_on_the_ground(&s).len(), 1);
}

#[test]
fn a_tap_of_f_beside_a_chest_does_nothing() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);

    tap_f(&mut s, a);
    s.run_ticks(5);

    assert_eq!(chests_left(&s), 1);
    assert_eq!(s.sim.open_progress(a), None);
    assert!(s.player(a).interaction().is_none());
}

#[test]
fn a_chest_out_of_reach_is_not_opened() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    // 플레이어의 가장자리에서 상자의 가장자리까지 1.45타일입니다.
    let a = s.player_at(2.5, 4.8);
    s.hold_interact(a);

    s.run_ticks(3);

    assert!(s.player(a).interaction().is_none());
    assert_eq!(s.sim.prompt(a), None);
}

#[test]
fn a_held_f_does_not_take_the_weapon_that_was_just_put_down_and_a_new_press_does() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    s.hold_interact(a);

    s.run_ticks(required_ticks(&s) + 20);

    assert_eq!(weapons_on_the_ground(&s).len(), 1, "still on the ground");
    assert_eq!(s.player(a).loadout, Loadout::new(WeaponKind::Sword, None));

    s.press_interact_key(a);

    assert_eq!(
        s.player(a).loadout,
        Loadout::new(WeaponKind::Sword, Some(WeaponKind::Spear))
    );
    assert!(weapons_on_the_ground(&s).is_empty());
}

#[test]
fn two_players_who_finish_on_the_same_tick_get_one_weapon() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    let b = s.player_at(1.6, 3.6);
    s.hold_interact(a);
    s.hold_interact(b);

    s.run_ticks(required_ticks(&s));

    assert_eq!(chests_left(&s), 0);
    assert_eq!(weapons_on_the_ground(&s).len(), 1, "one chest, one weapon");
    assert!(s.player(a).interaction().is_none());
    assert!(s.player(b).interaction().is_none());
}

#[test]
fn a_second_opener_whose_chest_was_opened_first_ends_without_a_weapon() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = beside_the_chest(&mut s);
    let b = s.player_at(1.6, 3.6);
    s.hold_interact(a);
    s.run_ticks(2);
    s.hold_interact(b);

    s.run_ticks(required_ticks(&s) - 2);
    assert_eq!(chests_left(&s), 0, "the first player opened it");
    assert!(
        s.player(b).interaction().is_some(),
        "the second is still counting"
    );

    s.run_ticks(1);
    assert!(
        s.player(b).interaction().is_none(),
        "the chest is gone, so the second opening ends"
    );
    assert_eq!(weapons_on_the_ground(&s).len(), 1);
}

#[test]
fn nightfall_ends_an_opening_that_is_not_finished_and_leaves_no_weapon() {
    let mut s = Scenario::with_cycle(|t| {
        super::support::chest_tuning(t, WeaponKind::Spear, 0);
        t.cycle.day_secs = 1.0;
        t.nights.plans = nights(&[&[1]]);
    });
    let a = beside_the_chest(&mut s);
    s.hold_interact(a);

    s.run_until(100, |sim| matches!(phase(sim), Phase::Night { .. }));
    s.run_ticks(1);

    assert_eq!(chests_left(&s), 0, "the unopened chest vanished");
    assert!(weapons_on_the_ground(&s).is_empty(), "and it gave nothing");
    assert!(
        !matches!(s.player(a).interaction(), Some(Interaction::Opening { .. })),
        "the opening cannot go on without a chest"
    );
}

#[test]
fn the_prompt_says_open_chest_in_reach_and_is_silent_while_opening_or_out_of_reach() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let near = beside_the_chest(&mut s);
    let far = s.player_at(10.0, 10.0);
    assert_eq!(s.sim.prompt(near), Some(Prompt::OpenChest));
    assert_eq!(s.sim.prompt(far), None);

    s.hold_interact(near);
    s.run_ticks(3);

    assert_eq!(s.sim.prompt(near), None, "the progress bar shows instead");

    s.run_ticks(required_ticks(&s));
    s.hold(near, PlayerInput::default());
    s.run_ticks(1);
    assert_eq!(
        s.sim.prompt(near),
        Some(Prompt::TakeWeapon(WeaponKind::Spear)),
        "the chest is open, so F now takes the weapon"
    );
}

/// 상자 (2, 2)를 열 수 있는 가장 먼 자리 여덟 곳입니다. 네 변의 한가운데와 네 모서리의 바깥쪽 대각선 방향이고,
/// 어디서든 플레이어의 가장자리에서 상자의 가장자리까지 열 수 있는 거리보다 0.001타일만 안쪽입니다.
fn farthest_spots_to_open_from(s: &Scenario) -> Vec<(&'static str, Vec2)> {
    let t = s.sim.tuning();
    let out = t.player.radius + t.loot.open_reach - 0.001;
    let diagonal = out / std::f32::consts::SQRT_2;
    let chest = Tile::new(2, 2);
    vec![
        ("south", Vec2::new(2.5, 3.0 + out)),
        ("north", Vec2::new(2.5, 2.0 - out)),
        ("east", Vec2::new(3.0 + out, 2.5)),
        ("west", Vec2::new(2.0 - out, 2.5)),
        (
            "south-east corner",
            Vec2::new(3.0 + diagonal, 3.0 + diagonal),
        ),
        (
            "south-west corner",
            Vec2::new(2.0 - diagonal, 3.0 + diagonal),
        ),
        (
            "north-east corner",
            Vec2::new(3.0 + diagonal, 2.0 - diagonal),
        ),
        (
            "north-west corner",
            Vec2::new(2.0 - diagonal, 2.0 - diagonal),
        ),
    ]
    .into_iter()
    .inspect(|(name, spot)| {
        assert!(
            chest.rect().distance_to(*spot) - t.player.radius <= t.loot.open_reach,
            "{name} is within the reach to open"
        );
    })
    .collect()
}

#[test]
fn whoever_opens_a_chest_from_the_farthest_spot_takes_its_weapon_with_one_press_without_stepping() {
    let spots = farthest_spots_to_open_from(&Scenario::with_chest(WeaponKind::Spear, 0));
    for (name, spot) in spots {
        let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
        let a = s.player_at(spot.x, spot.y);
        assert_eq!(s.sim.prompt(a), Some(Prompt::OpenChest), "{name}");
        s.hold_interact(a);
        s.run_ticks(required_ticks(&s));
        assert_eq!(chests_left(&s), 0, "{name}: the chest opened");

        s.hold(a, PlayerInput::default());
        s.run_ticks(1);
        assert_eq!(
            s.sim.prompt(a),
            Some(Prompt::TakeWeapon(WeaponKind::Spear)),
            "{name}: the prompt names the weapon from where the opener stands"
        );
        s.press_interact_key(a);

        assert_eq!(
            s.player(a).loadout,
            Loadout::new(WeaponKind::Sword, Some(WeaponKind::Spear)),
            "{name}: one press takes the weapon"
        );
        assert!(weapons_on_the_ground(&s).is_empty(), "{name}");
        assert_eq!(s.player(a).pos, spot, "{name}: the opener did not step");
    }
}

#[test]
fn every_spot_that_opens_a_chest_is_a_spot_that_takes_a_weapon_lying_on_its_tile() {
    let mut chest_side = Scenario::with_chest(WeaponKind::Spear, 0);
    let opener = chest_side.player_at(10.0, 10.0);
    let mut weapon_side = Scenario::open_field();
    let taker = weapon_side.player_at(10.0, 10.0);
    weapon_side.weapon_at(2.5, 2.5, WeaponKind::Spear);

    let mut spots_that_open = 0;
    for ix in 0..=250 {
        for iy in 0..=250 {
            let spot = Vec2::new(0.35 + ix as f32 * 0.02, 0.35 + iy as f32 * 0.02);
            chest_side.move_player(opener, spot);
            weapon_side.move_player(taker, spot);
            if chest_side.sim.prompt(opener) == Some(Prompt::OpenChest) {
                spots_that_open += 1;
                assert_eq!(
                    weapon_side.sim.prompt(taker),
                    Some(Prompt::TakeWeapon(WeaponKind::Spear)),
                    "the chest opens from {spot:?}, but the weapon on its tile is out of reach"
                );
            }
        }
    }

    assert!(spots_that_open > 1000, "{spots_that_open}");
}
