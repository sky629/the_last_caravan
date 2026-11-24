//! 마차 곁의 회복과 마차의 수리, 그리고 F의 대상 순서를 확인합니다.
//!
//! 시나리오는 20 x 20 맵이고 마차는 (17, 17)에서 (19, 19)까지입니다. 마차의 서쪽 곁 (16.5, 18.5)에 서면 마차의 가장자리까지 0.15타일입니다.

use super::support::Scenario;
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::PlayerId;
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::player::{Interaction, Life, RepairTarget};
use crate::world::Tile;

/// 마차의 서쪽 곁입니다. 마차의 가장자리까지 0.15타일이라 수리할 수 있는 거리 안이고, 회복하는 거리 안입니다.
const BESIDE_THE_WAGON: (f32, f32) = (16.5, 18.5);

/// 마차에서 회복하는 거리 안인 곳입니다. 마차의 가장자리까지 2.85타일입니다.
const NEAR_THE_WAGON: (f32, f32) = (13.8, 18.5);
/// 마차에서 회복하는 거리 밖인 곳입니다. 마차의 가장자리까지 3.25타일입니다.
const FAR_FROM_THE_WAGON: (f32, f32) = (13.4, 18.5);

fn hurt(s: &mut Scenario, id: PlayerId, hp: u32) {
    if let Life::Alive { hp: current, .. } =
        &mut s.sim.world_mut().players.get_mut(&id).unwrap().life
    {
        *current = hp;
    }
}

fn pulse_ticks(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().wagon.recovery.pulse_secs)
}

/// 마차 곁에서 HP가 50인 플레이어입니다.
fn hurt_player_at(s: &mut Scenario, at: (f32, f32)) -> PlayerId {
    let id = s.player_at(at.0, at.1);
    hurt(s, id, 50);
    id
}

// 마차 곁의 회복

#[test]
fn a_living_player_near_the_wagon_gains_hp_on_every_pulse_and_stops_at_the_maximum() {
    let mut s = Scenario::open_field();
    let a = hurt_player_at(&mut s, NEAR_THE_WAGON);
    let pulse = pulse_ticks(&s);

    s.run_ticks(pulse * 10);
    assert_eq!(s.player(a).hp(), 60, "one HP for each pulse");

    s.run_ticks(pulse * 100);
    assert_eq!(s.player(a).hp(), 100, "no more than the maximum");
}

#[test]
fn the_default_rate_is_five_hp_a_second() {
    let mut s = Scenario::open_field();
    let a = hurt_player_at(&mut s, NEAR_THE_WAGON);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.player(a).hp(), 60, "5 HP a second, for 2 seconds");
}

#[test]
fn a_player_outside_the_range_is_not_healed() {
    let mut s = Scenario::open_field();
    let near = hurt_player_at(&mut s, NEAR_THE_WAGON);
    let far = hurt_player_at(&mut s, FAR_FROM_THE_WAGON);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.player(near).hp(), 60);
    assert_eq!(s.player(far).hp(), 50);
}

#[test]
fn a_player_who_walks_into_the_range_starts_to_heal() {
    let mut s = Scenario::open_field();
    let a = hurt_player_at(&mut s, FAR_FROM_THE_WAGON);
    s.hold_move(a, 1.0, 0.0);
    s.run_ticks(3);
    s.hold(a, PlayerInput::default());

    s.run_ticks(s.ticks(1.0));

    assert!(
        s.player(a).hp() >= 55,
        "a full second inside the range gives 5 HP, got {}",
        s.player(a).hp()
    );
}

#[test]
fn a_downed_or_dead_player_is_not_healed() {
    let mut s = Scenario::open_field();
    let downed = s.player_at(NEAR_THE_WAGON.0, NEAR_THE_WAGON.1);
    let dead = s.player_at(NEAR_THE_WAGON.0, NEAR_THE_WAGON.1 - 1.0);
    s.player_at(1.0, 1.0);
    s.knock_down(downed);
    s.kill(dead);

    s.run_ticks(s.ticks(2.0));

    assert!(s.player(downed).is_downed());
    assert_eq!(s.player(downed).hp(), 0);
    assert!(matches!(s.player(dead).life, Life::Dead));
}

#[test]
fn a_player_is_healed_at_night_too() {
    let mut s = Scenario::open_field();
    s.sim.set_phase(Phase::Night {
        day: 1,
        step: NightStep::Waiting {
            next_wave: 1,
            ticks_left: u32::MAX,
        },
    });
    let a = hurt_player_at(&mut s, NEAR_THE_WAGON);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.player(a).hp(), 60);
}

#[test]
fn a_player_is_healed_while_a_monster_is_hitting_him() {
    // 몬스터의 쿨다운이 시험보다 길어서 한 번만 때립니다.
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.attack_damage = 1;
        t.monsters.grunt.attack_cooldown_secs = 100.0;
    });
    let a = hurt_player_at(&mut s, NEAR_THE_WAGON);
    s.monster_at(NEAR_THE_WAGON.0 - 0.6, NEAR_THE_WAGON.1);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.player(a).hp(), 50 + 10 - 1);
}

#[test]
fn the_pulse_follows_the_clock_of_the_room_so_everyone_heals_on_the_same_tick() {
    let mut s = Scenario::open_field();
    let early = hurt_player_at(&mut s, NEAR_THE_WAGON);
    let pulse = pulse_ticks(&s);
    s.run_until(pulse, |sim| sim.player(early).is_some_and(|p| p.hp() > 50));
    s.run_ticks(pulse / 2);
    let late = hurt_player_at(&mut s, (NEAR_THE_WAGON.0, NEAR_THE_WAGON.1 - 1.0));

    s.run_until(pulse, |sim| sim.player(early).is_some_and(|p| p.hp() > 51));

    assert_eq!(s.player(early).hp(), 52);
    assert_eq!(
        s.player(late).hp(),
        51,
        "a player who joined in the middle of a pulse heals on the next pulse of the room"
    );
}

// 마차의 수리

/// 마차의 서쪽 곁에서 F를 누르고 있는 플레이어와 HP가 300인 마차입니다.
fn repairer_at_a_damaged_wagon() -> (Scenario, PlayerId) {
    let mut s = Scenario::open_field();
    let a = s.player_at(BESIDE_THE_WAGON.0, BESIDE_THE_WAGON.1);
    s.set_wagon_hp(300);
    s.hold_interact(a);
    (s, a)
}

fn repair_pulse(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().wagon.repair.pulse_secs)
}

#[test]
fn holding_f_beside_a_damaged_wagon_repairs_it_in_pulses_and_costs_nothing() {
    let (mut s, a) = repairer_at_a_damaged_wagon();
    let pulse = repair_pulse(&s);
    let stash = s.sim.stash();

    s.run_ticks(pulse - 1);
    assert_eq!(s.sim.wagon().hp, 300, "no HP before the first pulse");
    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Repairing {
            target: RepairTarget::Wagon,
            ..
        })
    ));

    s.run_ticks(1);
    assert_eq!(s.sim.wagon().hp, 310, "10 HP for each pulse");
    s.run_ticks(pulse);
    assert_eq!(s.sim.wagon().hp, 320);
    assert_eq!(s.sim.stash(), stash, "a repair spends no resource");
}

#[test]
fn a_wagon_repair_ends_at_the_maximum_and_never_goes_above_it() {
    let (mut s, a) = repairer_at_a_damaged_wagon();
    s.set_wagon_hp(495);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.sim.wagon().hp, 500);
    assert!(
        s.player(a).interaction().is_none(),
        "nothing left to repair"
    );
}

#[test]
fn a_full_wagon_is_not_repaired() {
    let mut s = Scenario::open_field();
    let a = s.player_at(BESIDE_THE_WAGON.0, BESIDE_THE_WAGON.1);
    s.hold_interact(a);

    s.run_ticks(5);

    assert!(s.player(a).interaction().is_none());
}

#[test]
fn a_wagon_repairer_can_neither_move_nor_attack() {
    let (mut s, a) = repairer_at_a_damaged_wagon();
    let monster = s.monster_at(15.5, 18.5);
    s.hold(
        a,
        PlayerInput {
            interact: true,
            attack: true,
            move_dir: MoveDir::new(0.0, -1.0),
            aim: AimDir::new(-1.0, 0.0),
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
fn releasing_f_keeps_the_hp_restored_so_far() {
    let (mut s, a) = repairer_at_a_damaged_wagon();
    s.run_ticks(25);
    assert_eq!(s.sim.wagon().hp, 320);

    s.hold(a, PlayerInput::default());
    s.run_ticks(60);

    assert_eq!(s.sim.wagon().hp, 320);
}

#[test]
fn several_wagon_repairers_add_up() {
    let (mut s, _) = repairer_at_a_damaged_wagon();
    let b = s.player_at(16.5, 17.5);
    s.hold_interact(b);

    s.run_ticks(repair_pulse(&s));

    assert_eq!(s.sim.wagon().hp, 300 + 2 * 10);
}

#[test]
fn a_player_too_far_from_the_wagon_cannot_repair_it() {
    let mut s = Scenario::open_field();
    let a = s.player_at(14.5, 18.5);
    s.set_wagon_hp(300);
    s.hold_interact(a);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.sim.wagon().hp, 300);
    assert!(s.player(a).interaction().is_none());
}

#[test]
fn a_wagon_with_no_hp_left_is_not_a_repair_target_and_the_expedition_fails() {
    let (mut s, a) = repairer_at_a_damaged_wagon();
    s.set_wagon_hp(0);

    s.run_ticks(1);

    assert!(s.player(a).interaction().is_none());
    assert!(matches!(
        s.sim.phase(),
        Phase::Over {
            outcome: Outcome::Defeat,
            ..
        }
    ));
}

// F의 대상 순서

/// 부활, 감시탑 수리, 마차 수리, 채집이 모두 닿는 곳에서 F를 누르고 있는 플레이어입니다.
/// 채집 지점은 마을의 범위 밖이어야 하므로, 채집 거리를 크게 늘려서 마차 곁에서도 닿게 합니다.
fn everything_in_reach() -> (Scenario, PlayerId, PlayerId) {
    let mut s = Scenario::with_tuning(|t| {
        t.map.nodes = vec![Tile::new(2, 2)];
        t.resources.gather_reach = 40.0;
    });
    s.tower_at(15, 18);
    s.set_tower_hp(290);
    s.set_wagon_hp(490);
    let a = s.player_at(BESIDE_THE_WAGON.0, BESIDE_THE_WAGON.1);
    let downed = s.player_at(16.5, 19.5);
    s.knock_down(downed);
    s.hold_interact(a);
    (s, a, downed)
}

fn interaction_of(s: &Scenario, id: PlayerId) -> Option<Interaction> {
    s.player(id).interaction().cloned()
}

#[test]
fn f_picks_a_downed_ally_then_a_damaged_tower_then_the_wagon_then_a_node() {
    let (mut s, a, downed) = everything_in_reach();

    s.run_ticks(1);
    assert!(matches!(
        interaction_of(&s, a),
        Some(Interaction::Reviving { .. })
    ));

    s.run_until(s.ticks(10.0), |sim| {
        sim.player(downed).is_some_and(|p| p.is_alive())
    });
    s.run_ticks(1);
    assert!(
        matches!(
            interaction_of(&s, a),
            Some(Interaction::Repairing {
                target: RepairTarget::Tower(_),
                ..
            })
        ),
        "after the revive: the tower, got {:?}",
        interaction_of(&s, a)
    );

    s.run_until(s.ticks(5.0), |sim| sim.towers().all(|t| t.hp() == 300));
    s.run_ticks(1);
    assert!(
        matches!(
            interaction_of(&s, a),
            Some(Interaction::Repairing {
                target: RepairTarget::Wagon,
                ..
            })
        ),
        "after the tower: the wagon, got {:?}",
        interaction_of(&s, a)
    );

    s.run_until(s.ticks(5.0), |sim| sim.wagon().hp == 500);
    s.run_ticks(1);
    assert!(
        matches!(interaction_of(&s, a), Some(Interaction::Gathering { .. })),
        "after the wagon: the node, got {:?}",
        interaction_of(&s, a)
    );
}

#[test]
fn a_build_in_progress_goes_on_while_f_is_held_with_a_damaged_wagon_beside() {
    let mut s = Scenario::open_field();
    s.set_wagon_hp(300);
    let a = s.player_at(BESIDE_THE_WAGON.0, BESIDE_THE_WAGON.1);
    s.hold_build(a, -1.0, 0.0);
    s.run_ticks(5);
    assert!(matches!(
        interaction_of(&s, a),
        Some(Interaction::Building { .. })
    ));
    s.hold(
        a,
        PlayerInput {
            build: true,
            interact: true,
            aim: AimDir::new(-1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let required = s.ticks(s.sim.tuning().structures.watchtower.build_secs);

    s.run_ticks(required / 2);
    assert!(matches!(
        interaction_of(&s, a),
        Some(Interaction::Building { .. })
    ));
    assert_eq!(s.sim.wagon().hp, 300, "the build comes before the repair");

    s.run_ticks(required);
    assert!(
        s.sim
            .towers()
            .next()
            .is_some_and(|tower| tower.is_standing()),
        "the tower was finished"
    );
}
