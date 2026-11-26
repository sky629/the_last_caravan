//! 결과 화면의 기록을 확인합니다. 기록은 원정이 끝날 때 결과(`Report`)로 복사되므로, 테스트는 원정을 끝내고 결과의 표를 읽습니다.

use super::support::{Scenario, in_camp};
use crate::expedition::Outcome;
use crate::ids::PlayerId;
use crate::input::{AimDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::player::{Presence, Record};

/// 원정을 실패로 끝내고 결과를 돌려줍니다.
fn report_at_the_end(s: &mut Scenario) -> crate::debrief::Report {
    s.sim.end_expedition(Outcome::Defeat);
    s.sim
        .debrief()
        .expect("the result is shown")
        .report()
        .clone()
}

fn record_of(report: &crate::debrief::Report, id: PlayerId) -> Record {
    report
        .rows
        .iter()
        .find(|(player, _)| *player == id)
        .map(|(_, record)| *record)
        .expect("the player is in the table")
}

fn swing_east() -> PlayerInput {
    PlayerInput {
        attack: true,
        aim: AimDir::new(1.0, 0.0),
        ..PlayerInput::default()
    }
}

#[test]
fn the_player_who_deals_the_last_blow_gets_the_kill() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 150);
    let first = s.player_at(5.0, 5.0);
    let last = s.player_at(5.0, 4.0);
    let monster = s.monster_at(5.8, 4.5);

    s.hold(first, swing_east());
    s.run_ticks(1);
    s.hold(first, PlayerInput::default());
    s.hold(last, swing_east());
    s.run_ticks(1);

    assert!(s.monster(monster).is_none(), "two blows took it down");
    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, first).kills, 0);
    assert_eq!(record_of(&report, last).kills, 1);
}

#[test]
fn two_blows_in_the_same_tick_count_one_kill_for_the_first_blow_in_the_order_of_the_hits() {
    let mut s = Scenario::open_field();
    let first = s.player_at(5.0, 5.0);
    let second = s.player_at(5.0, 4.0);
    let monster = s.monster_at(5.8, 4.5);
    s.hold(first, swing_east());
    s.hold(second, swing_east());

    s.run_ticks(1);

    assert!(s.monster(monster).is_none());
    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, first).kills, 1);
    assert_eq!(record_of(&report, second).kills, 0);
}

#[test]
fn an_arrow_that_kills_belongs_to_the_archer_who_shot_it() {
    let mut s = Scenario::open_field();
    let archer = s.player_at(3.0, 10.0);
    let bystander = s.player_at(3.0, 2.0);
    s.give_weapon(archer, WeaponKind::Bow);
    s.monster_at(5.0, 10.0);
    s.hold(archer, swing_east());

    s.run_until(60, |sim| sim.monsters().next().is_none());

    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, archer).kills, 1);
    assert_eq!(record_of(&report, bystander).kills, 0);
}

#[test]
fn a_rifle_shot_that_kills_belongs_to_the_shooter() {
    let mut s = Scenario::open_field();
    let shooter = s.player_at(3.0, 10.0);
    let bystander = s.player_at(3.0, 2.0);
    s.give_weapon(shooter, WeaponKind::Rifle);
    s.monster_at(8.0, 10.0);
    s.hold(shooter, swing_east());

    s.run_ticks(1);

    assert!(s.sim.monsters().next().is_none(), "one shot took it down");
    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, shooter).kills, 1);
    assert_eq!(record_of(&report, bystander).kills, 0);
}

#[test]
fn an_arrow_that_kills_after_its_archer_left_is_counted_for_nobody() {
    let mut s = Scenario::open_field();
    let archer = s.player_at(3.0, 10.0);
    let bystander = s.player_at(3.0, 2.0);
    s.give_weapon(archer, WeaponKind::Bow);
    s.monster_at(8.0, 10.0);
    s.hold(archer, swing_east());
    s.run_ticks(1);
    s.hold(archer, PlayerInput::default());
    s.sim.remove_player(archer);

    s.run_until(60, |sim| sim.monsters().next().is_none());

    let report = report_at_the_end(&mut s);
    assert_eq!(report.rows.len(), 1, "only the bystander is left");
    assert_eq!(record_of(&report, bystander), Record::default());
    assert_eq!(report.tower_kills, 0);
}

#[test]
fn a_kill_by_a_tower_is_counted_for_the_tower_and_for_no_player() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 2.0);
    s.tower_at(14, 3);
    s.monster_at(14.5, 5.0);

    s.run_until(200, |sim| sim.monsters().next().is_none());

    let report = report_at_the_end(&mut s);
    assert_eq!(report.tower_kills, 1);
    assert_eq!(record_of(&report, a), Record::default());
}

#[test]
fn a_player_who_is_beaten_down_counts_one_down_and_dying_of_it_adds_none() {
    let mut s = Scenario::open_field();
    let beaten = s.player_at(5.0, 5.0);
    let bystander = s.player_at(2.0, 19.0);
    s.monster_at(5.5, 5.0);

    s.run_until(400, |sim| {
        sim.player(beaten).is_some_and(|player| player.is_downed())
    });
    let bleed_ticks = s.ticks(s.sim.tuning().revive.bleed_secs);
    s.run_ticks(bleed_ticks + 1);
    assert!(
        !s.player(beaten).is_downed() && !s.player(beaten).is_alive(),
        "it bled out"
    );

    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, beaten).downs, 1);
    assert_eq!(record_of(&report, bystander).downs, 0);
}

fn hold_f() -> PlayerInput {
    PlayerInput {
        interact: true,
        ..PlayerInput::default()
    }
}

/// 쓰러진 플레이어와 그 곁의 플레이어입니다. 부활에 걸리는 Tick 수도 돌려줍니다.
fn downed_and_reviver() -> (Scenario, PlayerId, PlayerId, u32) {
    let mut s = Scenario::open_field();
    let downed = s.player_at(10.0, 10.0);
    let reviver = s.player_at(11.0, 10.0);
    s.knock_down(downed);
    let needed = s.ticks(s.sim.tuning().revive.revive_secs);
    (s, downed, reviver, needed)
}

#[test]
fn a_revive_that_is_let_go_one_tick_short_counts_for_nobody() {
    let (mut s, downed, reviver, needed) = downed_and_reviver();

    s.hold(reviver, hold_f());
    s.run_ticks(needed - 1);
    s.hold(reviver, PlayerInput::default());
    s.run_ticks(1);

    assert!(s.player(downed).is_downed(), "the revive was cancelled");
    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, reviver).revives, 0);
}

#[test]
fn a_finished_revive_counts_for_the_reviver_and_not_for_the_one_revived() {
    let (mut s, downed, reviver, needed) = downed_and_reviver();

    s.hold(reviver, hold_f());
    s.run_ticks(needed);

    assert!(s.player(downed).is_alive(), "the revive finished");
    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, reviver).revives, 1);
    assert_eq!(record_of(&report, downed).revives, 0);
}

#[test]
fn what_a_player_puts_in_the_wagon_is_counted_for_that_player() {
    let mut s = Scenario::open_field();
    let carrier = s.player_at(16.5, 18.0);
    let other = s.player_at(2.0, 2.0);
    s.set_carried(carrier, 7);
    s.set_carried(other, 3);

    s.run_ticks(1);

    let report = report_at_the_end(&mut s);
    assert_eq!(record_of(&report, carrier).deposited, 7);
    assert_eq!(
        record_of(&report, other).deposited,
        0,
        "what is still in the hands is not put in"
    );
}

#[test]
fn records_start_at_zero_in_a_new_expedition_and_for_a_player_who_joins_late() {
    let mut s = Scenario::open_field();
    let early = s.player_at(5.0, 5.0);
    s.monster_at(5.8, 5.0);
    s.hold(early, swing_east());
    s.run_ticks(1);
    assert_eq!(
        record_of(&report_at_the_end(&mut s), early).kills,
        1,
        "the first expedition counted the kill"
    );

    s.sim.begin_expedition();
    let late = s.sim.add_player().expect("the room has space");
    let report = report_at_the_end(&mut s);

    assert_eq!(record_of(&report, early), Record::default());
    assert_eq!(record_of(&report, late), Record::default());
}

#[test]
fn the_table_lists_players_who_are_away_but_not_those_who_left() {
    let mut s = Scenario::open_field();
    let here = s.player_at(2.0, 2.0);
    let away = s.player_at(3.0, 2.0);
    let gone = s.player_at(4.0, 2.0);
    s.sim.set_presence(away, Presence::Away);
    s.sim.remove_player(gone);

    let report = report_at_the_end(&mut s);

    let ids: Vec<PlayerId> = report.rows.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, [here, away]);
}

#[test]
fn the_report_counts_the_ticks_of_the_expedition_and_not_the_time_in_the_camp() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(2.0, 2.0);
    s.tap_ready(a);
    s.run_until(200, |sim| !in_camp(sim));
    assert!(
        s.sim.tick() > 60,
        "the countdown in the camp took a while: {}",
        s.sim.tick()
    );

    s.run_ticks(30);
    let report = report_at_the_end(&mut s);

    assert_eq!(report.elapsed_ticks, 30);
}
