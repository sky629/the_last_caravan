//! 결과 화면의 기록과 인원에 비례하는 무리를 확인합니다. 기록은 원정이 끝날 때 결과(`Report`)로 복사되므로, 테스트는 원정을 끝내고 결과의 표를 읽습니다.

use super::support::{Scenario, in_camp, nights, phase};
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::PlayerId;
use crate::input::{AimDir, PlayerInput};
use crate::kind::{MonsterKind, WeaponKind};
use crate::player::{Presence, Record};
use crate::sim::Sim;
use crate::tuning::{Tuning, WaveScaling};

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

/// 몬스터의 HP가 `weapon`의 피해량과 같아서, 그 무기로 한 번 맞히면 죽는 시나리오입니다.
fn one_blow_kills(weapon: WeaponKind) -> Scenario {
    Scenario::with_tuning(|t| t.monsters.grunt.max_hp = t.weapons.get(weapon).damage)
}

#[test]
fn the_player_who_deals_the_last_blow_gets_the_kill() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 2 * t.weapons.sword.damage);
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
    let mut s = one_blow_kills(WeaponKind::Sword);
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
    let mut s = one_blow_kills(WeaponKind::Bow);
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
    let mut s = one_blow_kills(WeaponKind::Sword);
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

/// 낮이 거의 없고 무리가 곧바로 나오며 마차와 플레이어가 죽지 않는, 인원에 비례하는 밤의 시나리오입니다. 무리는 네 명이 있을 때의 크기입니다.
fn scaled_night(waves: &[u32]) -> Scenario {
    let waves = waves.to_vec();
    Scenario::with_cycle(move |t| {
        t.cycle.day_secs = 0.1;
        t.nights.plans = nights(&[&waves]);
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.nights.first_wave_delay_secs = 0.1;
        t.nights.between_waves_secs = 0.1;
        t.nights.spawn_interval_secs = 0.05;
        t.wagon.max_hp = u32::MAX;
        t.player.max_hp = u32::MAX;
        // 몬스터가 걷지도 서로 밀어내지도 않으므로 나온 자리에 그대로 서 있습니다.
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
    })
}

fn wave_is_out(wave: u32) -> impl Fn(&Sim) -> bool {
    move |sim| {
        matches!(
            sim.phase(),
            Some(Phase::Night {
                step: NightStep::Fighting { wave: now, to_spawn: 0, .. },
                ..
            }) if now == wave
        )
    }
}

/// 방에 플레이어가 `party`명 있을 때 무리마다 필드에 나온 몬스터의 수입니다. 무리가 모두 나오면 모두 처치된 것으로 하고 다음 무리를 기다립니다.
fn wave_sizes_for(party: usize, waves: &[u32]) -> Vec<usize> {
    let mut s = scaled_night(waves);
    for index in 0..party {
        s.player_at(2.0 + index as f32, 2.0);
    }
    (1..=waves.len() as u32)
        .map(|wave| {
            s.run_until(400, wave_is_out(wave));
            let out = s.sim.monsters().count();
            s.clear_monsters();
            out
        })
        .collect()
}

#[test]
fn a_wave_is_the_table_size_times_the_party_over_four_rounded_up() {
    assert_eq!(wave_sizes_for(1, &[8, 3, 1]), [2, 1, 1]);
    assert_eq!(wave_sizes_for(2, &[8, 3, 1]), [4, 2, 1]);
    assert_eq!(wave_sizes_for(3, &[8, 3, 1]), [6, 3, 1]);
    assert_eq!(wave_sizes_for(4, &[8, 3, 1]), [8, 3, 1]);
}

#[test]
fn the_monsters_of_a_small_wave_stand_where_the_first_ones_of_the_full_wave_stand() {
    let spots = |party: usize| {
        let mut s = scaled_night(&[8]);
        for index in 0..party {
            s.player_at(2.0 + index as f32, 2.0);
        }
        s.run_until(400, wave_is_out(1));
        s.sim
            .monsters()
            .map(|monster| monster.pos)
            .collect::<Vec<_>>()
    };

    let full = spots(4);
    let small = spots(1);

    assert_eq!(full.len(), 8);
    assert_eq!(small.len(), 2);
    assert_eq!(small, full[..2], "the small wave starts at the first spot");
}

#[test]
fn a_wave_for_an_empty_room_is_still_one_monster_of_each_of_its_packs() {
    let t = Tuning::default();
    assert_eq!(t.nights.wave_size(1, 1, 0), 1);
    assert_eq!(
        t.nights.wave_size(3, 3, 0),
        3,
        "the last wave has three packs, one monster for each"
    );
    assert!(
        (0..3).all(|nth| t.nights.kind_at(3, 3, 0, nth) != Some(MonsterKind::Sapper)),
        "the sapper's one is a grunt, because an empty room has no two players"
    );
    assert_eq!(
        t.nights.wave_size(4, 1, 1),
        0,
        "a night that is not in the table has no wave"
    );
}

#[test]
fn a_fixed_wave_ignores_the_party() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.1;
        t.nights.plans = nights(&[&[8]]);
        t.nights.first_wave_delay_secs = 0.1;
        t.nights.spawn_interval_secs = 0.05;
        t.wagon.max_hp = u32::MAX;
        t.player.max_hp = u32::MAX;
    });
    assert_eq!(s.sim.tuning().nights.scaling, WaveScaling::Fixed);
    s.player_at(2.0, 2.0);

    s.run_until(400, wave_is_out(1));

    assert_eq!(s.sim.monsters().count(), 8, "one player, the whole table");
}

#[test]
fn the_party_is_counted_when_the_night_begins_and_includes_players_who_are_away() {
    let mut s = scaled_night(&[8]);
    s.player_at(2.0, 2.0);
    let away = s.player_at(3.0, 2.0);
    s.sim.set_presence(away, Presence::Away);
    s.run_until(100, |sim| matches!(sim.phase(), Some(Phase::Night { .. })));
    assert!(matches!(phase(&s.sim), Phase::Night { party: 2, .. }));

    s.player_at(4.0, 2.0);
    s.run_until(400, wave_is_out(1));

    assert_eq!(
        s.sim.monsters().count(),
        4,
        "two seats at nightfall, 8 x 2 / 4, though a third player came in"
    );
}

#[test]
fn a_player_who_leaves_during_the_night_does_not_make_the_wave_smaller() {
    let mut s = scaled_night(&[8]);
    s.player_at(2.0, 2.0);
    s.player_at(3.0, 2.0);
    let leaver = s.player_at(4.0, 2.0);
    s.run_until(100, |sim| matches!(sim.phase(), Some(Phase::Night { .. })));

    s.sim.remove_player(leaver);
    s.run_until(400, wave_is_out(1));

    assert_eq!(
        s.sim.monsters().count(),
        6,
        "three seats at nightfall: 8 x 3 / 4"
    );
}
