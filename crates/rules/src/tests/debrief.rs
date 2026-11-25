//! 결과 화면을 확인합니다: 모두가 확인하면 끝나는 것, 시간이 다 되면 끝나는 것, 확인한 사람의 목록,
//! 결과 화면에 들어오고 나가는 사람.
//!
//! 시나리오는 `Scenario::camp`에서 원정을 시작하고 마차를 부숴서 곧바로 결과 화면을 여는 것입니다.

use super::support::{Scenario, camp_of, in_camp, outcome};
use crate::expedition::Outcome;
use crate::ids::PlayerId;

/// 플레이어 `count`명이 있는 방에서 원정이 곧바로 실패해서 결과 화면이 열린 상태입니다.
fn result_for(count: usize) -> (Scenario, Vec<PlayerId>) {
    let mut s = Scenario::camp(|_| {});
    let ids: Vec<PlayerId> = (0..count)
        .map(|index| s.player_at(2.0 + index as f32, 2.0))
        .collect();
    s.sim.begin_expedition();
    s.set_wagon_hp(0);
    s.run_ticks(1);
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));
    (s, ids)
}

fn confirmed(s: &Scenario) -> Vec<PlayerId> {
    s.sim
        .debrief()
        .expect("the result is shown")
        .confirmed()
        .collect()
}

fn result_ticks(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().camp.debrief_secs)
}

#[test]
fn the_result_stays_while_somebody_has_not_confirmed() {
    let (mut s, ids) = result_for(2);

    s.tap_confirm(ids[0]);
    s.run_ticks(10);

    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));
    assert_eq!(confirmed(&s), vec![ids[0]]);
}

#[test]
fn when_everyone_has_confirmed_the_room_returns_to_the_camp_at_once() {
    let (mut s, ids) = result_for(2);

    s.tap_confirm(ids[0]);
    s.tap_confirm(ids[1]);

    assert!(in_camp(&s.sim));
    assert!(s.ticks(0.5) < result_ticks(&s), "most of the time was left");
}

#[test]
fn the_result_ends_by_time_when_somebody_never_confirms() {
    let (mut s, ids) = result_for(2);
    s.tap_confirm(ids[0]);

    s.run_ticks(result_ticks(&s) - 2);
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));

    s.run_ticks(1);
    assert!(in_camp(&s.sim));
}

#[test]
fn confirming_twice_does_not_take_the_confirmation_back() {
    let (mut s, ids) = result_for(2);

    s.tap_confirm(ids[0]);
    s.tap_confirm(ids[0]);

    assert_eq!(confirmed(&s), vec![ids[0]]);
}

#[test]
fn the_confirmation_of_a_player_who_leaves_is_forgotten() {
    let (mut s, ids) = result_for(2);
    s.tap_confirm(ids[0]);

    s.sim.remove_player(ids[0]);

    assert_eq!(confirmed(&s), Vec::<PlayerId>::new());
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));
    s.tap_confirm(ids[1]);
    assert!(
        in_camp(&s.sim),
        "the player who is left was the last to confirm"
    );
}

#[test]
fn a_player_who_joins_during_the_result_has_to_confirm_too() {
    let (mut s, ids) = result_for(2);
    s.tap_confirm(ids[0]);
    let late = s.player_at(5.0, 5.0);

    s.tap_confirm(ids[1]);
    assert_eq!(
        outcome(&s.sim),
        Some(Outcome::Defeat),
        "the new player has not confirmed"
    );

    s.tap_confirm(late);
    assert!(in_camp(&s.sim));
}

#[test]
fn a_room_with_nobody_in_it_waits_out_the_result_time() {
    let (mut s, ids) = result_for(1);
    s.sim.remove_player(ids[0]);

    s.run_ticks(result_ticks(&s) - 1);
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));

    s.run_ticks(1);
    assert!(in_camp(&s.sim));
}

#[test]
fn the_confirm_key_of_a_player_who_is_not_in_the_room_is_ignored() {
    let (mut s, _) = result_for(1);

    s.tap_confirm(PlayerId(99));

    assert_eq!(confirmed(&s), Vec::<PlayerId>::new());
}

#[test]
fn nobody_is_ready_when_the_room_returns_to_the_camp() {
    let (mut s, ids) = result_for(2);
    s.tap_confirm(ids[0]);
    s.tap_confirm(ids[1]);
    assert!(in_camp(&s.sim));

    let camp = camp_of(&s.sim);

    assert_eq!(camp.ready().count(), 0);
    assert_eq!(camp.countdown(), None);
}
