//! 플레이어가 가진 무기를 확인합니다: 처음 가진 무기, `Q`로 바꾸기, 바꿀 수 없는 때, 쓰러짐과 새 날과 새 원정에서의 무기.
//! 시험하는 것은 `&Sim`이 공개하는 값(`Player::loadout`, `Sim::projectiles`)뿐입니다.

use super::support::{Scenario, phase};
use crate::expedition::Phase;
use crate::ids::MonsterId;
use crate::ids::PlayerId;
use crate::input::{AimDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::loadout::Loadout;
use crate::player::{Interaction, LastAttack};
use crate::tuning::{ArcTuning, Delivery, ProjectileTuning, WeaponTuning};
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

/// 근접 무기 `kind`의 행을 다른 무기의 행과 겹치지 않는 수치로 바꾼 시나리오입니다. 몬스터는 제자리에 서 있고 한 번에 죽지 않습니다.
/// 행이 아니라 다른 무기의 수치를 읽는 코드는 이 시나리오에서 맞히는 몬스터가 달라져서 드러납니다.
fn arc_weapon_with_its_own_numbers(kind: WeaponKind, row: WeaponTuning) -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        match kind {
            WeaponKind::Greatsword => t.weapons.greatsword = row,
            WeaponKind::Spear => t.weapons.spear = row,
            other => panic!("{other:?} is not a swing weapon of these tests"),
        }
    })
}

fn arc_row(damage: u32, cooldown_secs: f32, reach: f32, half_angle_deg: f32) -> WeaponTuning {
    WeaponTuning {
        damage,
        cooldown_secs,
        delivery: Delivery::Arc(ArcTuning {
            reach,
            half_angle_deg,
        }),
        chest_weight: 1,
    }
}

/// 플레이어(10, 10)에서 `degrees` 방향으로 `distance`만큼 떨어진 곳에 몬스터를 놓습니다. 0도는 동쪽(조준 방향)입니다.
fn monster_at_angle(s: &mut Scenario, distance: f32, degrees: f32) -> MonsterId {
    let (sin, cos) = degrees.to_radians().sin_cos();
    s.monster_at(10.0 + cos * distance, 10.0 + sin * distance)
}

fn hp_of(s: &Scenario, id: MonsterId) -> u32 {
    s.monster(id).expect("the monster is alive").hp
}

#[test]
fn the_greatsword_swings_the_arc_and_damage_of_its_own_row() {
    // 칼날 끝에서 몬스터의 가장자리까지의 닿는 거리는 중심 사이의 거리에서 두 반지름(0.35, 0.4)을 뺀 값입니다.
    let mut s =
        arc_weapon_with_its_own_numbers(WeaponKind::Greatsword, arc_row(7, 0.25, 2.0, 120.0));
    let a = s.player_at(10.0, 10.0);
    s.give_weapon(a, WeaponKind::Greatsword);
    let in_reach = monster_at_angle(&mut s, 2.7, 0.0);
    let beyond_reach = monster_at_angle(&mut s, 2.9, 90.0);
    let wide_inside = monster_at_angle(&mut s, 1.0, 100.0);
    let wide_outside = monster_at_angle(&mut s, 1.0, 140.0);
    s.hold(a, attack_east());

    s.run_ticks(1);

    assert_eq!(hp_of(&s, in_reach), 1000 - 7);
    assert_eq!(
        hp_of(&s, wide_inside),
        1000 - 7,
        "100 degrees is inside 120"
    );
    assert_eq!(
        hp_of(&s, beyond_reach),
        1000,
        "2.9 tiles is beyond 2.0 + 0.75"
    );
    assert_eq!(hp_of(&s, wide_outside), 1000, "140 degrees is outside 120");
}

#[test]
fn the_greatsword_waits_for_the_cooldown_of_its_own_row() {
    let mut s =
        arc_weapon_with_its_own_numbers(WeaponKind::Greatsword, arc_row(7, 0.25, 2.0, 120.0));
    let a = s.player_at(10.0, 10.0);
    s.give_weapon(a, WeaponKind::Greatsword);
    let monster = monster_at_angle(&mut s, 1.0, 0.0);
    s.hold(a, attack_east());

    s.run_ticks(11);

    // 쿨다운 0.25초는 5 Tick이므로 1, 6, 11번째 Tick에 휘두릅니다.
    assert_eq!(hp_of(&s, monster), 1000 - 3 * 7);
}

#[test]
fn the_spear_swings_the_arc_and_damage_of_its_own_row() {
    let mut s = arc_weapon_with_its_own_numbers(WeaponKind::Spear, arc_row(13, 0.35, 3.2, 25.0));
    let a = s.player_at(10.0, 10.0);
    s.give_weapon(a, WeaponKind::Spear);
    let in_reach = monster_at_angle(&mut s, 3.9, 0.0);
    let beyond_reach = monster_at_angle(&mut s, 4.1, 90.0);
    let narrow_inside = monster_at_angle(&mut s, 2.0, 20.0);
    let narrow_outside = monster_at_angle(&mut s, 2.0, 35.0);
    s.hold(a, attack_east());

    s.run_ticks(1);

    assert_eq!(hp_of(&s, in_reach), 1000 - 13);
    assert_eq!(
        hp_of(&s, narrow_inside),
        1000 - 13,
        "20 degrees is inside 25"
    );
    assert_eq!(
        hp_of(&s, beyond_reach),
        1000,
        "4.1 tiles is beyond 3.2 + 0.75"
    );
    assert_eq!(hp_of(&s, narrow_outside), 1000, "35 degrees is outside 25");
}

#[test]
fn the_spear_waits_for_the_cooldown_of_its_own_row() {
    let mut s = arc_weapon_with_its_own_numbers(WeaponKind::Spear, arc_row(13, 0.35, 3.2, 25.0));
    let a = s.player_at(10.0, 10.0);
    s.give_weapon(a, WeaponKind::Spear);
    let monster = monster_at_angle(&mut s, 1.0, 0.0);
    s.hold(a, attack_east());

    s.run_ticks(15);

    // 쿨다운 0.35초는 7 Tick이므로 1, 8, 15번째 Tick에 찌릅니다.
    assert_eq!(hp_of(&s, monster), 1000 - 3 * 13);
}

/// 석궁의 행을 활의 행과 겹치지 않는 수치로 바꾼 시나리오입니다. 몬스터는 제자리에 서 있고 한 번에 죽지 않습니다.
/// 행이 아니라 활의 수치를 읽는 코드는 이 시나리오에서 맞히는 몬스터와 날아가는 거리가 달라져서 드러납니다.
fn crossbow_with_its_own_numbers() -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        t.weapons.crossbow = WeaponTuning {
            damage: 7,
            cooldown_secs: 0.25,
            delivery: Delivery::Projectile(ProjectileTuning {
                speed: 30.0,
                range: 6.0,
                radius: 0.2,
                pierce: 2,
            }),
            chest_weight: 1,
        };
    })
}

#[test]
fn the_crossbow_shoots_the_damage_width_and_count_of_its_own_row() {
    let mut s = crossbow_with_its_own_numbers();
    let a = s.player_at(3.0, 10.0);
    s.give_weapon(a, WeaponKind::Crossbow);
    let first = s.monster_at(5.0, 10.0);
    // 활의 탄환(반지름 0.15)이 닿는 0.55보다 멀고 석궁의 탄환(반지름 0.2)이 닿는 0.6보다 가까운 곳입니다.
    let just_inside_the_width = s.monster_at(6.0, 10.58);
    let beyond_the_count = s.monster_at(7.0, 10.0);
    s.hold(a, attack_east());

    s.run_ticks(1);
    s.hold(
        a,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    s.run_ticks(4);

    assert_eq!(hp_of(&s, first), 1000 - 7);
    assert_eq!(
        hp_of(&s, just_inside_the_width),
        1000 - 7,
        "0.58 aside is inside 0.4 + 0.2"
    );
    assert_eq!(hp_of(&s, beyond_the_count), 1000, "two hits and it ended");
}

#[test]
fn the_crossbow_bolt_flies_the_speed_and_range_of_its_own_row() {
    let mut s = crossbow_with_its_own_numbers();
    let a = s.player_at(3.0, 10.0);
    s.give_weapon(a, WeaponKind::Crossbow);
    s.hold(a, attack_east());

    s.run_ticks(1);
    s.hold(
        a,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let bolt = s.sim.projectiles().next().expect("the bolt flies");
    // 초당 30타일은 Tick(20 Hz)마다 1.5타일입니다.
    assert!(
        (bolt.pos.x - 4.5).abs() < 1e-3,
        "one tick of speed 30, at {:?}",
        bolt.pos
    );

    s.run_until(10, |sim| sim.projectiles().any(|bolt| !bolt.is_flying()));
    let bolt = s.sim.projectiles().next().expect("the bolt ended");
    assert!(
        (bolt.pos.x - 9.0).abs() < 1e-3,
        "range 6 from x = 3, ended at {:?}",
        bolt.pos
    );
}

#[test]
fn the_crossbow_waits_for_the_cooldown_of_its_own_row() {
    let mut s = crossbow_with_its_own_numbers();
    let a = s.player_at(3.0, 10.0);
    s.give_weapon(a, WeaponKind::Crossbow);
    s.hold(a, attack_east());

    s.run_ticks(11);

    // 쿨다운 0.25초는 5 Tick이므로 1, 6, 11번째 Tick에 쏩니다.
    let newest = s.sim.projectiles().map(|bolt| bolt.id.0).max();
    assert_eq!(newest, Some(3));
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
            weapon: WeaponKind::Sword,
            trace_end: None
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

    s.run_until(5, |sim| matches!(phase(sim), Phase::Day { day: 2, .. }));

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
    s.restart_expedition();

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
