//! 야영지를 확인합니다: 허수아비는 야영지에만 놓이는 것, 준비와 카운트다운, 원정이 시작될 때 모두가 처음 상태로 돌아가는 것,
//! 야영지에서 아무것도 피해를 받지 않고 허수아비가 맞은 기록만 남는 것, 건설의 거절, 원정이 끝난 뒤 야영지로 돌아오는 것.
//!
//! 시나리오는 `Scenario::camp`입니다. 20 x 20 맵에서 허수아비 셋이 (12.5, 5.0)부터 동쪽으로 한 줄로 서고,
//! 플레이어의 등장 지점은 (10, 10)입니다. 진열대는 `racks.rs`가, 결과 화면은 `debrief.rs`가 확인합니다.

use super::support::{Scenario, camp_of, in_camp, outcome, phase};
use crate::build::{BuildRefusal, CannotBuild};
use crate::camp::Struck;
use crate::expedition::{Outcome, Phase};
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, PlayerInput, PlayerInputs};
use crate::kind::{MonsterKind, WeaponKind};
use crate::loadout::Loadout;
use crate::loot::Seed;
use crate::math::Vec2;
use crate::player::Presence;
use crate::sim::Sim;
use crate::stage::Stage;
use crate::tuning::{Start, Tuning};
use crate::world::Tile;

/// 모두가 준비한 뒤 원정이 시작되기까지의 Tick 수입니다.
fn countdown_ticks(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().camp.countdown_secs)
}

/// 번호가 작은 허수아비부터 차례로 돌려줍니다. 한 줄로 선 허수아비의 서쪽 끝이 처음입니다.
fn scarecrows(s: &Scenario) -> Vec<MonsterId> {
    s.sim
        .monsters()
        .filter(|monster| monster.kind == MonsterKind::Scarecrow)
        .map(|monster| monster.id)
        .collect()
}

/// 허수아비 `id`가 마지막으로 맞은 때입니다.
fn struck_of(s: &Scenario, id: MonsterId) -> Option<Struck> {
    camp_of(&s.sim)
        .struck()
        .find(|(struck, _)| *struck == id)
        .map(|(_, struck)| struck)
}

/// `(x, y)`에 서서 동쪽을 향해 공격 키를 누르고 있는 플레이어입니다.
fn attacker_facing_east(s: &mut Scenario, x: f32, y: f32, weapon: WeaponKind) -> PlayerId {
    let id = s.player_at(x, y);
    s.give_weapon(id, weapon);
    s.hold(
        id,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    );
    id
}

#[test]
fn the_scarecrow_is_refused_in_waves_and_as_a_guard() {
    let mut t = Tuning::default();
    t.nights.plans[0].waves[0].packs[0].kind = MonsterKind::Scarecrow;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.guards.kind = MonsterKind::Scarecrow;
    assert!(t.validate().is_err());
}

#[test]
fn a_default_room_starts_in_the_camp_counts_three_seconds_and_shows_the_result_for_a_minute() {
    let tuning = Tuning::default();
    assert_eq!(tuning.camp.start, Start::WhenReady);
    assert_eq!(tuning.camp.countdown_secs, 3.0);
    assert_eq!(tuning.camp.debrief_secs, 60.0);
    assert_eq!(tuning.validate(), Ok(()));

    let sim = Sim::new(tuning, Seed(1));

    assert!(in_camp(&sim));
    assert_eq!(sim.expedition_number(), 0);
}

#[test]
fn the_camp_has_scarecrows_and_racks_and_nothing_of_the_expedition() {
    // 지도에 채집 지점, 상자 자리, 처음부터 서 있는 감시탑이 있어도 야영지에는 놓이지 않습니다.
    let mut s = Scenario::camp(|t| {
        t.map.nodes = vec![Tile::new(3, 3)];
        t.map.chest_sites = vec![Tile::new(5, 3)];
        t.loot.chests_per_day = 1;
        t.guards.per_node = 2;
        t.village.start_towers = vec![Tile::new(15, 17)];
    });

    let kinds: Vec<MonsterKind> = s.sim.monsters().map(|monster| monster.kind).collect();
    assert_eq!(
        kinds,
        vec![MonsterKind::Scarecrow; 3],
        "no monster but the scarecrows"
    );
    assert_eq!(s.sim.nodes().count(), 0);
    assert_eq!(s.sim.chests().count(), 0);
    assert_eq!(s.sim.towers().count(), 0);
    assert_eq!(s.sim.ground_weapons().count(), 0);
    assert_eq!(s.sim.stash(), 0);
    let racks: Vec<WeaponKind> = s.sim.racks().map(|rack| rack.kind).collect();
    assert_eq!(racks, WeaponKind::ALL.to_vec());

    s.sim.begin_expedition();
    assert_eq!(
        s.sim.nodes().count(),
        1,
        "the expedition has its node again"
    );
    assert_eq!(s.sim.chests().count(), 1);
    assert_eq!(s.sim.towers().count(), 1);
}

#[test]
fn a_room_that_starts_in_the_camp_returns_to_it_when_the_result_time_is_up() {
    let mut s = Scenario::camp(|_| {});
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
    assert_eq!(scarecrows(&s).len(), 3, "the scarecrows stand again");
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

#[test]
fn a_player_who_leaves_the_camp_leaves_nothing_behind() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(2.0, 2.0);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Spear));

    s.sim.remove_player(a);

    assert_eq!(s.sim.ground_weapons().count(), 0);
    assert_eq!(s.sim.piles().count(), 0);
    assert_eq!(s.sim.player_count(), 0);
}

// 준비와 카운트다운

#[test]
fn a_lone_player_who_gets_ready_starts_the_expedition_after_the_countdown() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let wait = countdown_ticks(&s);

    s.run_ticks(5);
    assert!(!camp_of(&s.sim).is_ready(a));
    assert_eq!(camp_of(&s.sim).countdown(), None, "nobody is ready yet");

    s.tap_ready(a);
    assert!(camp_of(&s.sim).is_ready(a));
    assert_eq!(camp_of(&s.sim).countdown(), Some(wait));

    s.run_ticks(wait - 1);
    assert!(in_camp(&s.sim), "one tick of the countdown is left");
    assert_eq!(camp_of(&s.sim).countdown(), Some(1));

    s.run_ticks(1);
    assert_eq!(s.sim.expedition_number(), 1);
    assert!(matches!(phase(&s.sim), Phase::Day { day: 1, .. }));
}

#[test]
fn a_camp_where_nobody_is_ready_waits_however_long_it_takes() {
    let mut s = Scenario::camp(|_| {});
    s.player_at(10.0, 10.0);
    s.player_at(11.0, 10.0);

    s.run_ticks(5000);

    assert!(in_camp(&s.sim));
    assert_eq!(s.sim.expedition_number(), 0);
    assert_eq!(camp_of(&s.sim).countdown(), None);
}

#[test]
fn a_player_who_joins_a_running_expedition_is_in_it_at_once_and_alive_at_a_start_point() {
    let mut s = Scenario::with_cycle(|_| {});
    let first = s.player_at(2.0, 2.0);
    s.run_ticks(40);
    let before = phase(&s.sim);

    let late = s.sim.add_player().expect("the room has space");

    assert_eq!(s.sim.expedition_number(), 1, "no new expedition starts");
    assert!(!in_camp(&s.sim));
    assert_eq!(
        std::mem::discriminant(&phase(&s.sim)),
        std::mem::discriminant(&before),
        "the day goes on"
    );
    assert!(s.player(late).is_alive());
    assert_eq!(s.player(late).hp(), s.sim.tuning().player.max_hp);
    assert_eq!(s.player(late).pos, s.sim.tuning().map.player_spawns[0]);
    assert_eq!(
        s.player(first).pos,
        Vec2::new(2.0, 2.0),
        "the player who was there is not moved"
    );
}

#[test]
fn a_ready_player_does_not_start_the_expedition_while_another_is_not_ready() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);

    s.tap_ready(a);
    s.run_ticks(countdown_ticks(&s) * 3);

    assert!(in_camp(&s.sim));
    assert!(camp_of(&s.sim).is_ready(a));
    assert!(!camp_of(&s.sim).is_ready(b));
    assert_eq!(camp_of(&s.sim).countdown(), None);
}

#[test]
fn pressing_ready_again_takes_the_ready_back() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);

    s.tap_ready(a);
    assert!(camp_of(&s.sim).is_ready(a));
    s.tap_ready(a);

    assert!(!camp_of(&s.sim).is_ready(a));
    assert_eq!(camp_of(&s.sim).countdown(), None, "the countdown is gone");
    s.run_ticks(countdown_ticks(&s) * 2);
    assert!(in_camp(&s.sim));
}

#[test]
fn the_countdown_starts_over_when_a_ready_player_takes_the_ready_back() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    let wait = countdown_ticks(&s);
    s.tap_ready(a);
    s.tap_ready(b);
    assert_eq!(camp_of(&s.sim).countdown(), Some(wait));
    s.run_ticks(10);
    assert_eq!(camp_of(&s.sim).countdown(), Some(wait - 10));

    s.tap_ready(b);
    assert_eq!(camp_of(&s.sim).countdown(), None);
    s.run_ticks(10);
    s.tap_ready(b);

    assert_eq!(
        camp_of(&s.sim).countdown(),
        Some(wait),
        "the time already counted is not kept"
    );
}

#[test]
fn a_player_who_joins_during_the_countdown_cancels_it_at_once() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    s.tap_ready(a);
    s.run_ticks(10);
    assert!(camp_of(&s.sim).countdown().is_some());

    let b = s.player_at(11.0, 10.0);

    assert_eq!(
        camp_of(&s.sim).countdown(),
        None,
        "not before the next tick, the moment the player joins"
    );
    assert!(camp_of(&s.sim).is_ready(a), "the others stay ready");
    assert!(
        !camp_of(&s.sim).is_ready(b),
        "the new player has to get ready"
    );
    s.run_ticks(countdown_ticks(&s) * 2);
    assert!(in_camp(&s.sim));
    s.tap_ready(b);
    s.run_ticks(countdown_ticks(&s));
    assert_eq!(s.sim.expedition_number(), 1);
}

#[test]
fn a_ready_player_who_leaves_does_not_cancel_the_countdown() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    let wait = countdown_ticks(&s);
    s.tap_ready(a);
    s.tap_ready(b);
    s.run_ticks(10);

    s.sim.remove_player(a);

    assert_eq!(camp_of(&s.sim).countdown(), Some(wait - 10));
    assert!(
        !camp_of(&s.sim).is_ready(a),
        "the ready of a player who left is forgotten"
    );
    assert_eq!(camp_of(&s.sim).ready().collect::<Vec<_>>(), vec![b]);
    s.run_ticks(wait - 10);
    assert_eq!(s.sim.expedition_number(), 1);
}

#[test]
fn a_ready_player_coming_back_does_not_cancel_but_an_unready_one_does() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    let wait = countdown_ticks(&s);
    s.tap_ready(a);
    s.tap_ready(b);
    s.sim.set_presence(b, Presence::Away);
    s.run_ticks(10);
    assert_eq!(
        camp_of(&s.sim).countdown(),
        Some(wait - 10),
        "away is not a cancel"
    );

    s.sim.set_presence(b, Presence::Here);

    assert_eq!(
        camp_of(&s.sim).countdown(),
        Some(wait - 10),
        "b was ready before and still is"
    );

    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let c = s.player_at(11.0, 10.0);
    s.tap_ready(a);
    s.sim.set_presence(c, Presence::Away);
    s.run_ticks(5);
    assert!(
        camp_of(&s.sim).countdown().is_some(),
        "the one who is here is ready and starts it"
    );

    s.sim.set_presence(c, Presence::Here);

    assert_eq!(
        camp_of(&s.sim).countdown(),
        None,
        "not before the next tick, the moment c is back and not ready"
    );
    assert!(camp_of(&s.sim).is_ready(a));
}

#[test]
fn when_the_player_who_is_not_ready_leaves_the_others_start_the_countdown() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    let wait = countdown_ticks(&s);
    s.tap_ready(a);
    assert_eq!(camp_of(&s.sim).countdown(), None);

    s.sim.remove_player(b);
    s.run_ticks(1);

    assert_eq!(camp_of(&s.sim).countdown(), Some(wait));
}

#[test]
fn an_empty_camp_never_starts_an_expedition() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    s.tap_ready(a);
    s.run_ticks(5);
    assert!(camp_of(&s.sim).countdown().is_some());

    s.sim.remove_player(a);

    assert_eq!(
        camp_of(&s.sim).countdown(),
        None,
        "nobody is left to wait for"
    );
    s.run_ticks(countdown_ticks(&s) * 3);
    assert!(in_camp(&s.sim));
    assert_eq!(s.sim.expedition_number(), 0);
}

#[test]
fn the_ready_key_of_a_player_who_is_not_in_the_room_is_ignored() {
    let mut s = Scenario::camp(|_| {});
    let mut inputs = PlayerInputs::new();
    inputs.insert(
        PlayerId(99),
        PlayerInput {
            toggle_ready: true,
            ..PlayerInput::default()
        },
    );

    s.sim.step(&inputs);

    assert_eq!(camp_of(&s.sim).ready().count(), 0);
}

#[test]
fn the_ready_key_does_nothing_during_the_expedition() {
    let mut s = Scenario::with_cycle(|_| {});
    let a = s.player_at(10.0, 10.0);

    s.tap_ready(a);
    s.run_ticks(countdown_ticks(&s) * 2);

    assert_eq!(s.sim.expedition_number(), 1);
    assert!(matches!(phase(&s.sim), Phase::Day { day: 1, .. }));
}

#[test]
fn the_expedition_the_countdown_starts_puts_everyone_back_at_the_start() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    s.arm(a, WeaponKind::Bow, Some(WeaponKind::Spear));
    s.set_carried(a, 5);
    s.move_player(b, Vec2::new(3.0, 3.0));
    s.hold_move(b, 1.0, 0.0);
    s.set_stash(7);
    let start_stash = s.sim.tuning().wagon.start_stash;
    assert_ne!(start_stash, 7);
    let spawns = s.sim.tuning().map.player_spawns.clone();
    let wait = countdown_ticks(&s);
    s.tap_ready(a);
    s.tap_ready(b);

    s.run_ticks(wait);

    assert_eq!(s.sim.expedition_number(), 1);
    let start = s.sim.tuning().player.start_loadout;
    assert_eq!(start, Loadout::new(WeaponKind::Sword, None));
    for id in [a, b] {
        let player = s.player(id);
        assert_eq!(
            player.loadout, start,
            "the weapon of the camp is not carried over"
        );
        assert_eq!(player.hp(), s.sim.tuning().player.max_hp);
        assert_eq!(
            player.carried(),
            0,
            "nothing is carried into the expedition"
        );
        assert!(
            spawns.contains(&player.pos),
            "back at a start point, not one step beyond it: {:?}",
            player.pos
        );
    }
    assert_eq!(scarecrows(&s).len(), 0, "the scarecrows stay in the camp");
    assert_eq!(s.sim.racks().count(), 0, "so do the racks");
    assert_eq!(
        s.sim.stash(),
        start_stash,
        "the stash starts again from the start of an expedition"
    );
}

#[test]
fn the_time_spent_in_the_camp_does_not_change_what_the_expedition_draws() {
    fn chests_after(waiting: u32) -> Vec<(Tile, WeaponKind)> {
        let mut s = Scenario::camp(|t| {
            t.map.chest_sites = (0..8).map(|row| Tile::new(2, 1 + row * 2)).collect();
            t.loot.chests_per_day = 3;
            t.guards.per_chest = 0;
        });
        let a = s.player_at(10.0, 10.0);
        s.run_ticks(waiting);
        s.tap_ready(a);
        s.run_ticks(countdown_ticks(&s));
        s.sim
            .chests()
            .map(|chest| (chest.tile, chest.contents))
            .collect()
    }

    let at_once = chests_after(0);
    assert_eq!(at_once.len(), 3);
    assert_eq!(chests_after(137), at_once);
    assert_eq!(chests_after(1000), at_once);
}

// 야영지에서의 연습

#[test]
fn a_player_walks_and_sprints_in_the_camp() {
    let walked = {
        let mut s = Scenario::camp(|_| {});
        let a = s.player_at(5.0, 5.0);
        s.hold_move(a, 1.0, 0.0);
        s.run_ticks(10);
        s.player(a).pos.x
    };
    let sprinted = {
        let mut s = Scenario::camp(|_| {});
        let a = s.player_at(5.0, 5.0);
        s.hold_sprint(a, 1.0, 0.0);
        s.run_ticks(10);
        s.player(a).pos.x
    };

    let t = Tuning::default();
    let per_tick = t.player.speed * t.dt();
    assert!((walked - (5.0 + 10.0 * per_tick)).abs() < 1e-3, "{walked}");
    assert!(
        (sprinted - (5.0 + 10.0 * per_tick * t.sprint.speed_multiplier)).abs() < 1e-3,
        "{sprinted}"
    );
}

#[test]
fn a_sword_swing_is_recorded_on_the_scarecrow_and_does_not_hurt_it() {
    let mut s = Scenario::camp(|_| {});
    let [first, second, third] = scarecrows(&s)[..] else {
        panic!("the camp has three scarecrows");
    };
    attacker_facing_east(&mut s, 11.0, 5.0, WeaponKind::Sword);
    let damage = s.sim.tuning().weapons.sword.damage;
    let hp = s.sim.tuning().monsters.scarecrow.max_hp;

    s.run_ticks(1);

    let tick = s.sim.tick();
    assert_eq!(
        struck_of(&s, first),
        Some(Struck {
            tick,
            amount: damage
        })
    );
    assert_eq!(struck_of(&s, second), None, "it is out of reach");
    assert_eq!(struck_of(&s, third), None);
    s.run_ticks(60);
    assert_eq!(s.monster(first).expect("it still stands").hp, hp);
}

#[test]
fn an_arrow_that_reaches_a_scarecrow_is_recorded_on_the_first_one_only() {
    let mut s = Scenario::camp(|_| {});
    let [first, second, _] = scarecrows(&s)[..] else {
        panic!("the camp has three scarecrows");
    };
    attacker_facing_east(&mut s, 10.0, 5.0, WeaponKind::Bow);
    let damage = s.sim.tuning().weapons.bow.damage;

    s.run_ticks(10);

    let struck = struck_of(&s, first).expect("the arrow reached it");
    assert_eq!(struck.amount, damage);
    assert_eq!(struck_of(&s, second), None, "an arrow hits one monster");
}

#[test]
fn a_crossbow_bolt_goes_through_the_three_scarecrows_in_a_row() {
    let mut s = Scenario::camp(|_| {});
    attacker_facing_east(&mut s, 10.0, 5.0, WeaponKind::Crossbow);
    let damage = s.sim.tuning().weapons.crossbow.damage;

    s.run_ticks(10);

    let amounts: Vec<u32> = camp_of(&s.sim)
        .struck()
        .map(|(_, struck)| struck.amount)
        .collect();
    assert_eq!(amounts, vec![damage; 3]);
}

#[test]
fn two_players_who_hit_the_same_scarecrow_in_one_tick_are_added_up() {
    let mut s = Scenario::camp(|_| {});
    let first = scarecrows(&s)[0];
    attacker_facing_east(&mut s, 11.0, 5.0, WeaponKind::Sword);
    attacker_facing_east(&mut s, 11.0, 5.2, WeaponKind::Sword);
    let damage = s.sim.tuning().weapons.sword.damage;

    s.run_ticks(1);

    assert_eq!(
        struck_of(&s, first),
        Some(Struck {
            tick: s.sim.tick(),
            amount: 2 * damage
        })
    );
}

#[test]
fn a_newer_strike_replaces_the_older_one_instead_of_adding_to_it() {
    let mut s = Scenario::camp(|_| {});
    let first = scarecrows(&s)[0];
    attacker_facing_east(&mut s, 11.0, 5.0, WeaponKind::Sword);
    let damage = s.sim.tuning().weapons.sword.damage;
    s.run_ticks(1);
    let older = struck_of(&s, first).expect("the first swing landed");

    s.run_until(100, |sim| {
        camp_of(sim)
            .struck()
            .any(|(_, struck)| struck.tick != older.tick)
    });

    let newer = struck_of(&s, first).expect("the second swing landed");
    assert!(newer.tick > older.tick);
    assert_eq!(newer.amount, damage);
}

#[test]
fn a_monster_and_a_tower_in_the_camp_do_nothing() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(10.0, 10.0);
    let biter = s.monster_at(10.6, 10.0);
    let walker = s.monster_at(2.0, 2.0);
    s.tower_at(7, 10);
    let place = s.monster(walker).expect("the grunt is there").pos;
    let hp = s.monster(biter).expect("the grunt is there").hp;

    s.run_ticks(200);

    assert_eq!(
        s.player(a).hp(),
        s.sim.tuning().player.max_hp,
        "it does not bite"
    );
    let biter = s.monster(biter).expect("the grunt is still there");
    assert_eq!(biter.last_attack_tick, None, "it does not even try to bite");
    assert_eq!(biter.hp, hp, "the tower does not shoot it");
    let walker = s.monster(walker).expect("the grunt is still there");
    assert_eq!(walker.pos, place, "it does not walk");
    assert_eq!(camp_of(&s.sim).struck().count(), 0, "nothing hit anything");
}

#[test]
fn building_is_refused_in_the_camp_even_with_enough_in_the_stash() {
    let mut s = Scenario::camp(|_| {});
    s.set_stash(1000);
    let a = s.player_at(14.5, 14.5);
    s.hold_build(a, 1.0, 0.0);

    s.run_ticks(5);

    assert_eq!(
        s.player(a).build_refusal().copied(),
        Some(BuildRefusal {
            tile: Tile::new(15, 14),
            why: CannotBuild::InCamp
        })
    );
    assert_eq!(s.sim.towers().count(), 0);
    assert_eq!(s.sim.stash(), 1000);
    assert!(s.player(a).interaction().is_none());
}

#[test]
fn a_build_toward_the_edge_of_the_map_is_refused_in_the_camp_on_the_tile_where_the_player_stands() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(19.5, 0.5);
    s.hold_build(a, 1.0, 0.0);

    s.run_ticks(1);

    assert_eq!(
        s.player(a).build_refusal().copied(),
        Some(BuildRefusal {
            tile: Tile::new(19, 0),
            why: CannotBuild::InCamp
        })
    );
}
