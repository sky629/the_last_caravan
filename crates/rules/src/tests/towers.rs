//! 감시탑의 사격, 피해와 파괴, 수리를 확인합니다.
//!
//! 시나리오의 감시탑은 (1, 17)에서 (2, 18)까지의 사각형이고 중심은 (1.5, 17.5)입니다.

use super::support::{Scenario, close};
use crate::input::PlayerInput;
use crate::kind::StructureKind;
use crate::structures::{Shot, TowerState};
use crate::tuning::StructurePlacement;
use crate::world::Tile;

fn press_f() -> PlayerInput {
    PlayerInput {
        interact: true,
        ..PlayerInput::default()
    }
}

/// 감시탑 곁에서 F를 누르고 있는 플레이어와, HP가 100인 감시탑입니다.
fn repairer_at_a_damaged_tower() -> (Scenario, crate::ids::PlayerId) {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.5, 17.5);
    s.set_tower_hp(100);
    s.hold(a, press_f());
    (s, a)
}

#[test]
fn a_tower_shoots_the_nearest_monster_in_range() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 1000);
    let near = s.monster_at(4.0, 17.5);
    let far = s.monster_at(6.0, 17.5);

    s.run_ticks(1);

    assert_eq!(s.monster(near).unwrap().hp, 1000 - 15);
    assert_eq!(s.monster(far).unwrap().hp, 1000);
    let TowerState::Standing { last_shot, .. } = &s.tower().state else {
        panic!("the tower stands");
    };
    assert_eq!(
        *last_shot,
        Some(Shot {
            target: near,
            tick: 1
        }),
        "visible in the state"
    );
}

#[test]
fn a_tower_ignores_monsters_beyond_its_range() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.aggro_range = 0.0);
    let monster = s.monster_at(9.0, 17.5);

    s.run_ticks(1);

    assert_eq!(
        s.monster(monster).unwrap().hp,
        60,
        "7.5 tiles is beyond the range of 6"
    );
}

#[test]
fn a_tower_shoots_again_after_its_cooldown() {
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.aggro_range = 0.0;
        t.monsters.grunt.speed = 0.5;
    });
    let monster = s.monster_at(3.0, 17.5);

    s.run_ticks(s.ticks(0.8));
    assert_eq!(
        s.monster(monster).unwrap().hp,
        1000 - 15,
        "one shot in the first 16 ticks"
    );

    s.run_ticks(1);
    assert_eq!(s.monster(monster).unwrap().hp, 1000 - 30, "the second shot");
}

#[test]
fn towers_are_kept_by_id_and_each_one_shoots() {
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.map.structures.push(StructurePlacement {
            kind: StructureKind::Watchtower,
            tile: Tile::new(1, 14),
        });
    });
    let monster = s.monster_at(3.0, 16.0);

    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 2);
    let ids: Vec<_> = s.sim.towers().map(|tower| tower.id).collect();
    assert_ne!(ids[0], ids[1]);
    assert_eq!(
        s.monster(monster).unwrap().hp,
        1000 - 2 * 15,
        "both towers are in range"
    );
}

#[test]
fn a_monster_in_range_goes_for_the_tower_and_hurts_it() {
    let mut s = Scenario::open_field();
    s.monster_at(3.0, 17.5);
    let max_hp = s.sim.tuning().structures.watchtower.max_hp;

    s.run_ticks(s.ticks(1.0));

    assert!(s.tower().hp() < max_hp);
    assert_eq!(
        s.sim.wagon().hp,
        500,
        "the monster chose the tower, not the wagon"
    );
}

#[test]
fn zero_hp_destroys_the_tower() {
    let mut s = Scenario::open_field();
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);

    s.run_until(100, |sim| !sim.towers().next().unwrap().is_standing());

    assert_eq!(s.tower().state, TowerState::Destroyed);
    assert_eq!(s.tower().hp(), 0);
}

#[test]
fn a_destroyed_tower_stops_shooting() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 1000);
    s.set_tower_hp(10);
    let attacker = s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| !sim.towers().next().unwrap().is_standing());
    let hp_when_destroyed = s.monster(attacker).unwrap().hp;

    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.monster(attacker).unwrap().hp, hp_when_destroyed);
}

#[test]
fn monsters_do_not_attack_a_destroyed_tower() {
    let mut s = Scenario::open_field();
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| !sim.towers().next().unwrap().is_standing());
    s.clear_monsters();

    let fresh = s.monster_at(3.0, 17.5);
    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.monster(fresh).unwrap().last_attack_tick, None);
    assert!(
        s.monster(fresh).unwrap().pos.x > 4.0,
        "it walks on toward the wagon"
    );
}

#[test]
fn a_destroyed_tower_still_blocks_movement() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 17.5);
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| !sim.towers().next().unwrap().is_standing());
    s.clear_monsters();

    s.hold_move(a, -1.0, 0.0);
    s.run_ticks(s.ticks(3.0));

    assert!(close(s.player(a).pos.x, 2.0 + s.sim.tuning().player.radius));
}

#[test]
fn holding_f_next_to_a_damaged_tower_repairs_it_in_pulses() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    let pulse = s.ticks(s.sim.tuning().structures.watchtower.repair_pulse_secs);

    s.run_ticks(pulse - 1);
    assert_eq!(s.tower().hp(), 100, "no HP before the first pulse");

    s.run_ticks(1);
    assert_eq!(s.tower().hp(), 110, "10 HP per pulse");
    s.run_ticks(pulse);
    assert_eq!(s.tower().hp(), 120);
    assert!(s.player(a).interaction().is_some());
}

#[test]
fn a_repair_ends_at_full_hp_and_never_goes_above_it() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    s.set_tower_hp(295);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.tower().hp(), 300);
    assert!(
        s.player(a).interaction().is_none(),
        "nothing left to repair"
    );
}

#[test]
fn releasing_f_stops_the_repair_and_keeps_the_hp_restored_so_far() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    s.run_ticks(25);
    assert_eq!(s.tower().hp(), 120);

    s.hold(a, PlayerInput::default());
    s.run_ticks(60);

    assert_eq!(
        s.tower().hp(),
        120,
        "the restored HP stays and nothing more arrives"
    );
}

#[test]
fn a_repairer_can_neither_move_nor_attack() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    let monster = s.monster_at(3.4, 17.5);
    s.hold(
        a,
        PlayerInput {
            interact: true,
            attack: true,
            move_dir: crate::input::MoveDir::new(0.0, -1.0),
            aim: crate::input::AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let start = s.player(a).pos;

    s.run_ticks(20);

    assert_eq!(s.player(a).pos, start);
    assert_eq!(s.player(a).last_attack_tick, None);
    assert!(s.monster(monster).is_some_and(|m| m.hp > 0));
}

#[test]
fn the_repair_stops_when_the_repairer_goes_down() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.attack_damage = 100);
    let a = s.player_at(2.5, 17.5);
    s.player_at(15.0, 3.0);
    s.set_tower_hp(100);
    s.hold(a, press_f());
    s.run_ticks(5);
    assert!(s.player(a).interaction().is_some());

    s.monster_at(3.4, 17.5);
    s.run_ticks(2);
    let hp = s.tower().hp();
    s.run_ticks(60);

    assert!(s.player(a).is_downed());
    assert!(s.player(a).interaction().is_none());
    assert!(
        s.tower().hp() <= hp,
        "no more HP is restored once the repairer is down"
    );
}

#[test]
fn several_repairers_add_up() {
    let (mut s, _) = repairer_at_a_damaged_tower();
    let b = s.player_at(2.5, 17.0);
    s.hold(b, press_f());
    let pulse = s.ticks(s.sim.tuning().structures.watchtower.repair_pulse_secs);

    s.run_ticks(pulse);

    assert_eq!(s.tower().hp(), 100 + 2 * 10);
}

#[test]
fn a_repair_needs_no_resource_and_a_far_player_cannot_repair() {
    let mut s = Scenario::open_field();
    let near = s.player_at(2.5, 17.5);
    let far = s.player_at(6.0, 17.5);
    s.set_tower_hp(100);
    s.hold(far, press_f());

    s.run_ticks(40);
    assert_eq!(s.tower().hp(), 100, "out of the repair range");
    assert!(s.player(far).interaction().is_none());

    s.hold(near, press_f());
    s.run_ticks(10);
    assert_eq!(
        s.tower().hp(),
        110,
        "a player in range repairs with no cost"
    );
}

#[test]
fn a_destroyed_tower_cannot_be_repaired() {
    let mut s = Scenario::open_field();
    let a = s.player_at(1.5, 18.4);
    s.set_tower_hp(10);
    s.monster_at(1.5, 16.6);
    s.run_until(100, |sim| !sim.towers().next().unwrap().is_standing());
    s.clear_monsters();

    s.hold(a, press_f());
    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.tower().state, TowerState::Destroyed);
    assert!(s.player(a).interaction().is_none());
}

#[test]
fn a_downed_ally_in_range_comes_before_a_damaged_tower() {
    let mut s = Scenario::open_field();
    let reviver = s.player_at(2.5, 17.5);
    let downed = s.player_at(3.5, 17.5);
    s.player_at(15.0, 3.0);
    s.set_tower_hp(100);
    s.knock_down(downed);
    s.hold(reviver, press_f());
    let revive_ticks = s.ticks(s.sim.tuning().revive.revive_secs);

    s.run_ticks(revive_ticks / 2);
    assert!(s.sim.revive_progress(downed).is_some(), "revive first");
    assert_eq!(s.tower().hp(), 100, "no repair while reviving");

    s.run_ticks(revive_ticks);
    assert!(s.player(downed).is_alive());
    assert!(s.tower().hp() > 100, "then the repair begins");
}

#[test]
fn a_repairer_switches_to_reviving_when_an_ally_goes_down_beside_them() {
    let mut s = Scenario::open_field();
    let reviver = s.player_at(2.5, 17.5);
    let ally = s.player_at(3.5, 17.5);
    s.player_at(15.0, 3.0);
    s.set_tower_hp(100);
    s.hold(reviver, press_f());
    s.run_ticks(5);
    assert!(s.sim.revive_progress(ally).is_none());

    s.knock_down(ally);
    s.run_ticks(2);

    assert!(s.sim.revive_progress(ally).is_some());
}

#[test]
fn a_new_expedition_rebuilds_a_destroyed_tower() {
    let mut s = Scenario::open_field();
    s.player_at(15.0, 3.0);
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| !sim.towers().next().unwrap().is_standing());
    s.clear_monsters();
    s.set_wagon_hp(0);

    s.run_until(400, |sim| sim.expedition_number() == 2);

    assert_eq!(s.tower().hp(), s.sim.tuning().structures.watchtower.max_hp);
    assert_eq!(s.sim.towers().count(), 1);
}
