//! 자리를 비운 플레이어를 확인합니다: 몬스터가 노리지 않고 피해를 받지 않는 것, 출혈이 멈추는 것,
//! 준비와 확인과 실패에서 빠지는 것, 돌아오면 모두 되돌아오는 것.

use super::support::{Scenario, outcome};
use crate::expedition::Outcome;
use crate::math::Vec2;
use crate::player::Presence;
use crate::stage::Stage;
use crate::world::{Hit, Target, Tile};

#[test]
fn a_monster_ignores_an_away_player_and_walks_on_to_the_wagon() {
    let mut s = Scenario::open_field();
    let away = s.player_at(5.0, 5.0);
    s.player_at(1.0, 1.0);
    s.sim.set_presence(away, Presence::Away);
    let monster = s.monster_at(5.6, 5.0);

    s.run_ticks(40);

    assert_eq!(s.player(away).hp(), s.sim.tuning().player.max_hp);
    let walked = s.monster(monster).unwrap().pos;
    assert!(
        walked.x > 5.7,
        "it walks off toward the wagon, at {walked:?}"
    );
}

#[test]
fn a_guard_does_not_chase_an_away_player_inside_its_range() {
    let mut s = Scenario::with_tuning(|t| {
        t.map.nodes = vec![Tile::new(2, 2)];
        t.guards.per_node = 1;
    });
    let away = s.player_at(5.5, 2.5);
    s.sim.set_presence(away, Presence::Away);
    let before: Vec<_> = s.sim.monsters().cloned().collect();

    s.run_ticks(s.ticks(3.0));

    let after: Vec<_> = s.sim.monsters().cloned().collect();
    assert_eq!(after, before, "the guard stayed on its post");
    assert_eq!(s.player(away).hp(), s.sim.tuning().player.max_hp);
}

#[test]
fn a_player_who_comes_back_is_a_target_again() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.sim.set_presence(a, Presence::Away);
    s.monster_at(5.6, 5.0);
    s.run_ticks(40);
    assert_eq!(s.player(a).hp(), s.sim.tuning().player.max_hp);

    s.sim.set_presence(a, Presence::Here);
    s.sim.world_mut().monsters.clear();
    s.monster_at(5.6, 5.0);
    s.run_ticks(40);

    assert!(s.player(a).hp() < s.sim.tuning().player.max_hp);
}

#[test]
fn a_hit_that_reaches_an_away_player_does_nothing() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.sim.set_presence(a, Presence::Away);
    let t = s.sim.tuning().clone();

    s.sim.world_mut().apply_hits(
        &[Hit {
            target: Target::Player(a),
            amount: t.player.max_hp,
        }],
        &t,
    );

    assert_eq!(s.player(a).hp(), t.player.max_hp);
}

#[test]
fn an_away_downed_player_does_not_bleed_but_bleeds_again_when_back() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.player_at(1.0, 1.0);
    s.knock_down(a);
    s.sim.set_presence(a, Presence::Away);
    let bleed = s.ticks(s.sim.tuning().revive.bleed_secs);

    s.run_ticks(bleed + 10);

    assert!(s.player(a).is_downed(), "still downed, not dead");

    s.sim.set_presence(a, Presence::Here);
    s.run_ticks(bleed + 10);
    assert!(!s.player(a).is_downed() && !s.player(a).is_alive(), "dead");
}

#[test]
fn an_away_player_who_is_still_alive_does_not_hold_off_a_defeat() {
    let mut s = Scenario::open_field();
    let here = s.player_at(2.0, 2.0);
    let away = s.player_at(3.0, 2.0);
    s.sim.set_presence(away, Presence::Away);

    s.knock_down(here);
    s.run_ticks(1);

    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));
}

#[test]
fn a_room_with_only_away_players_never_fails() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 2.0);
    s.sim.set_presence(a, Presence::Away);
    s.set_wagon_hp(0);

    s.run_ticks(10);

    assert_eq!(outcome(&s.sim), None);

    s.sim.set_presence(a, Presence::Here);
    s.run_ticks(1);
    assert_eq!(
        outcome(&s.sim),
        Some(Outcome::Defeat),
        "present again, the wagon is gone"
    );
}

#[test]
fn away_players_do_not_hold_up_the_ready_or_the_confirmation_and_still_count_for_the_room() {
    let mut s = Scenario::camp(|_| {});
    let here = s.sim.add_player().unwrap();
    let away = s.sim.add_player().unwrap();
    s.sim.set_presence(away, Presence::Away);

    s.tap_ready(here);
    s.run_until(200, |sim| sim.phase().is_some());

    s.sim.end_expedition(Outcome::Defeat);
    s.tap_confirm(here);
    assert!(matches!(s.sim.stage(), Stage::Camp(_)));
    assert_eq!(
        s.sim.player_count(),
        2,
        "the away seat stays in the room and in its capacity"
    );
}

fn camp_of(sim: &crate::sim::Sim) -> &crate::camp::Camp {
    let Stage::Camp(camp) = sim.stage() else {
        panic!("expected the camp");
    };
    camp
}

#[test]
fn withdrawing_the_ready_of_an_away_player_keeps_the_countdown_for_the_others_and_cancels_it_when_the_player_is_back()
 {
    let mut s = Scenario::camp(|_| {});
    let a = s.sim.add_player().unwrap();
    let b = s.sim.add_player().unwrap();
    s.tap_ready(a);
    s.tap_ready(b);
    assert!(camp_of(&s.sim).countdown().is_some(), "both are ready");

    s.sim.set_presence(b, Presence::Away);
    s.sim.withdraw_ready(b);
    assert!(!camp_of(&s.sim).is_ready(b));
    assert!(
        camp_of(&s.sim).countdown().is_some(),
        "the one who is here is ready, so the countdown goes on"
    );

    s.sim.set_presence(b, Presence::Here);
    assert!(
        camp_of(&s.sim).countdown().is_none(),
        "back and not ready: the countdown is cancelled"
    );
}

#[test]
fn withdrawing_the_ready_of_someone_who_is_not_ready_or_not_in_the_room_changes_nothing() {
    let mut s = Scenario::camp(|_| {});
    let a = s.sim.add_player().unwrap();
    let b = s.sim.add_player().unwrap();
    s.tap_ready(a);
    let before = camp_of(&s.sim).clone();

    s.sim.withdraw_ready(b);
    s.sim.withdraw_ready(crate::ids::PlayerId(99));

    assert_eq!(*camp_of(&s.sim), before);
}

#[test]
fn withdrawing_the_ready_outside_the_camp_does_nothing() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 2.0);

    s.sim.withdraw_ready(a);

    assert!(matches!(s.sim.stage(), Stage::Expedition(_)));
    assert_eq!(s.player(a).presence, Presence::Here);
}

#[test]
fn presence_carries_over_into_a_new_expedition() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 2.0);
    s.sim.set_presence(a, Presence::Away);

    s.restart_expedition();

    assert_eq!(s.player(a).presence, Presence::Away);
    assert_eq!(s.player(a).pos, Vec2::new(10.0, 10.0), "back at the spawn");
}

#[test]
fn naming_a_player_who_is_not_in_the_room_changes_nothing() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 2.0);

    s.sim.set_presence(crate::ids::PlayerId(99), Presence::Away);

    assert_eq!(s.player(a).presence, Presence::Here);
}
