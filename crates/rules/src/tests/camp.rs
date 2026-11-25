//! 야영지와 결과 화면을 확인합니다: 허수아비는 야영지에만 놓이고, 원정이 끝나면 결과 화면을 거쳐 야영지나 다음 원정으로 넘어갑니다.

use super::support::{Scenario, outcome};
use crate::expedition::{Outcome, Phase};
use crate::input::PlayerInputs;
use crate::kind::{MonsterKind, WeaponKind};
use crate::loot::Seed;
use crate::sim::Sim;
use crate::stage::Stage;
use crate::tuning::{Start, Tuning};

fn in_camp(sim: &Sim) -> bool {
    matches!(sim.stage(), Stage::Camp(_))
}

#[test]
fn the_scarecrow_is_refused_in_waves_and_as_a_guard() {
    let mut t = Tuning::default();
    t.nights.plans[0].waves[0].kind = MonsterKind::Scarecrow;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.guards.kind = MonsterKind::Scarecrow;
    assert!(t.validate().is_err());
}

#[test]
fn a_room_that_starts_in_the_camp_returns_to_it_when_the_result_time_is_up() {
    let mut s = Scenario::with_cycle(|t| t.camp.start = Start::WhenReady);
    assert!(in_camp(&s.sim));
    assert_eq!(s.sim.expedition_number(), 0);
    assert_eq!(s.sim.phase(), None);

    s.sim.begin_expedition();
    let a = s.player_at(2.0, 2.0);
    s.set_wagon_hp(0);
    s.run_ticks(1);
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));

    s.run_ticks(s.ticks(s.sim.tuning().camp.debrief_secs) - 1);
    assert_eq!(
        outcome(&s.sim),
        Some(Outcome::Defeat),
        "still on the result"
    );

    s.run_ticks(1);
    assert!(
        in_camp(&s.sim),
        "the time is up and the room is back in the camp"
    );
    s.run_ticks(5000);
    assert!(in_camp(&s.sim), "and it waits there");
    assert_eq!(s.sim.expedition_number(), 1);
    assert_eq!(s.player(a).hp(), s.sim.tuning().player.max_hp);
    assert_eq!(s.player(a).pos, s.sim.tuning().map.player_spawns[0]);
}

#[test]
fn an_immediate_start_skips_the_camp_and_follows_each_result_with_a_new_expedition() {
    let mut tuning = Tuning::default();
    tuning.camp.start = Start::AtOnce;
    tuning.camp.debrief_secs = tuning.dt();
    let mut sim = Sim::new(tuning, Seed(1));
    assert_eq!(sim.expedition_number(), 1);
    assert!(matches!(sim.phase(), Some(Phase::Day { day: 1, .. })));

    let a = sim.add_player().unwrap();
    sim.world_mut().wagon.hp = 0;
    let idle = PlayerInputs::new();
    sim.step(&idle);
    assert!(matches!(sim.stage(), Stage::Debrief(_)));
    sim.step(&idle);

    assert_eq!(sim.expedition_number(), 2);
    assert!(matches!(sim.phase(), Some(Phase::Day { day: 1, .. })));
    assert!(sim.player(a).is_some());
}

#[test]
fn a_player_who_leaves_during_the_result_leaves_nothing_behind() {
    let mut s = Scenario::with_cycle(|_| {});
    let a = s.player_at(2.0, 2.0);
    s.set_carried(a, 5);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Spear));
    s.set_wagon_hp(0);
    s.run_ticks(1);
    assert!(outcome(&s.sim).is_some());

    s.sim.remove_player(a);

    assert_eq!(s.sim.piles().count(), 0);
    assert_eq!(s.sim.ground_weapons().count(), 0);
}
