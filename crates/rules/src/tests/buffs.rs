//! 플레이어의 버프(공격 속도, 이동 속도)를 확인합니다. 이 게임에는 버프를 거는 곳이 없으므로 테스트가 `Scenario::grant`로 줍니다.
//! 버프는 플레이어의 공격이 나가는 Tick과 이동한 거리, 그리고 버프가 걸려 있는지의 질문(`PlayerBuffs::has_haste`, `has_swift`)으로 읽습니다.

use super::support::{Scenario, close, in_camp, nights, phase};
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::PlayerId;
use crate::input::{AimDir, PlayerInput};
use crate::player::{Life, Presence};
use crate::tuning::{Buff, Tuning};

fn swing_east() -> PlayerInput {
    PlayerInput {
        attack: true,
        aim: AimDir::new(1.0, 0.0),
        ..PlayerInput::default()
    }
}

/// 플레이어가 지금 가진 버프입니다. 앞이 공격 속도, 뒤가 이동 속도입니다. 쓰러졌거나 사망한 플레이어는 버프를 가질 수 없습니다.
fn buffs_of(s: &Scenario, id: PlayerId) -> (bool, bool) {
    match &s.player(id).life {
        Life::Alive { buffs, .. } => (buffs.has_haste(), buffs.has_swift()),
        Life::Downed { .. } | Life::Dead => (false, false),
    }
}

/// 공격 키를 누르고 있으면서 `ticks` Tick을 진행하고, 공격이 나간 Tick을 돌려줍니다. 진행을 시작한 뒤 몇 번째 Tick인지를 1부터 셉니다.
fn swing_ticks_in(s: &mut Scenario, id: PlayerId, ticks: u32) -> Vec<u32> {
    s.hold(id, swing_east());
    let mut swings = Vec::new();
    let mut last = s.player(id).last_attack.map(|attack| attack.tick);
    for tick in 1..=ticks {
        s.run_ticks(1);
        let now = s.player(id).last_attack.map(|attack| attack.tick);
        if now != last {
            swings.push(tick);
            last = now;
        }
    }
    swings
}

/// `swing_ticks_in`이 찾은 공격의 횟수입니다.
fn swings_in(s: &mut Scenario, id: PlayerId, ticks: u32) -> usize {
    swing_ticks_in(s, id, ticks).len()
}

/// 가로가 60타일인 빈 들판에서 `x`에 선 플레이어입니다. 가득 찬 게이지로 달리는 거리가 맵 안에 들어갑니다.
fn on_a_wide_field(x: f32) -> (Scenario, PlayerId) {
    let mut s = Scenario::with_tuning(|t| t.map.width = 60.0);
    let a = s.player_at(x, 5.0);
    (s, a)
}

#[test]
fn the_default_buffs_are_70_percent_of_the_cooldown_and_130_percent_of_the_speed() {
    let t = Tuning::default();
    assert_eq!(t.effects.haste_cooldown_factor, 0.7);
    assert_eq!(t.effects.swift_speed_factor, 1.3);
}

#[test]
fn a_haste_buff_shortens_the_cooldown_to_70_percent_while_it_lasts() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    // 검의 쿨다운은 0.5초, 10 Tick입니다. 버프가 있으면 7 Tick입니다.
    assert_eq!(swings_in(&mut s, a, 70), 7, "ten ticks apart");
    s.hold(a, PlayerInput::default());
    s.run_ticks(20);

    s.grant(a, Buff::Haste { secs: 2.0 });
    assert_eq!(buffs_of(&s, a), (true, false));
    assert_eq!(
        swings_in(&mut s, a, 40),
        6,
        "seven ticks apart: 1, 8, 15, 22, 29, 36"
    );
    assert_eq!(
        buffs_of(&s, a),
        (false, false),
        "the buff ended after 2 seconds"
    );
    assert_eq!(swings_in(&mut s, a, 70), 7, "back to ten ticks");
}

#[test]
fn a_haste_received_again_restarts_its_time_and_does_not_get_stronger() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.grant(a, Buff::Haste { secs: 1.0 });
    s.run_ticks(s.ticks(0.5));
    s.grant(a, Buff::Haste { secs: 1.0 });
    s.run_ticks(s.ticks(0.75));
    assert_eq!(
        buffs_of(&s, a),
        (true, false),
        "1.25 seconds after the first one"
    );

    // 남은 5 Tick 동안 공격이 한 번 나가고, 쿨다운은 7 Tick입니다. 70%가 겹치면 5 Tick입니다.
    assert_eq!(swing_ticks_in(&mut s, a, 12), [1, 8]);
}

#[test]
fn a_swift_buff_multiplies_the_sprint_and_ends_with_its_time() {
    let (mut s, a) = on_a_wide_field(1.0);
    s.hold_sprint(a, 1.0, 0.0);
    s.run_ticks(1);
    assert!(close(s.player(a).pos.x - 1.0, 0.32), "4 x 1.6 x 0.05");

    s.grant(a, Buff::Swift { secs: 1.0 });
    let before = s.player(a).pos.x;
    s.run_ticks(19);
    assert_eq!(buffs_of(&s, a), (false, true), "19 of its 20 ticks");
    s.run_ticks(1);
    assert_eq!(buffs_of(&s, a), (false, false), "the 20th tick ended it");
    assert!(
        close(s.player(a).pos.x - before, 20.0 * 0.416),
        "4 x 1.6 x 1.3 x 0.05 for each of the 20 ticks"
    );

    let before = s.player(a).pos.x;
    s.run_ticks(1);
    assert!(close(s.player(a).pos.x - before, 0.32), "back to a sprint");
}

#[test]
fn a_swift_player_walking_is_130_percent() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    s.hold_move(a, 1.0, 0.0);
    s.grant(a, Buff::Swift { secs: 5.0 });

    s.run_ticks(1);

    assert!(close(s.player(a).pos.x - 2.0, 0.26), "4 x 1.3 x 0.05");
}

#[test]
fn a_buff_received_again_restarts_its_time_and_does_not_get_stronger() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    s.hold_move(a, 1.0, 0.0);
    s.grant(a, Buff::Swift { secs: 1.0 });
    s.run_ticks(s.ticks(0.5));
    s.grant(a, Buff::Swift { secs: 1.0 });
    s.run_ticks(s.ticks(0.75));
    assert_eq!(
        buffs_of(&s, a),
        (false, true),
        "1.25 seconds after the first one"
    );

    let before = s.player(a).pos.x;
    s.run_ticks(1);
    assert!(
        close(s.player(a).pos.x - before, 0.26),
        "130 percent, not 169"
    );
    s.run_ticks(s.ticks(0.25));
    assert_eq!(buffs_of(&s, a), (false, false));
}

#[test]
fn a_buff_shorter_than_one_tick_is_not_received_and_leaves_a_held_buff_alone() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    for secs in [0.0, -1.0, f32::NAN] {
        s.grant(a, Buff::Haste { secs });
        s.grant(a, Buff::Swift { secs });
        assert_eq!(buffs_of(&s, a), (false, false), "a length of {secs}");
    }

    s.grant(a, Buff::Haste { secs: 1.0 });
    s.grant(a, Buff::Swift { secs: 1.0 });
    for secs in [0.0, -1.0, f32::NAN] {
        s.grant(a, Buff::Haste { secs });
        s.grant(a, Buff::Swift { secs });
    }
    s.run_ticks(s.ticks(1.0) - 1);
    assert_eq!(buffs_of(&s, a), (true, true), "the first second still runs");
    s.run_ticks(1);
    assert_eq!(buffs_of(&s, a), (false, false));
}

#[test]
fn haste_and_swift_are_held_together() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    s.grant(a, Buff::Haste { secs: 3.0 });
    s.grant(a, Buff::Swift { secs: 1.0 });
    assert_eq!(buffs_of(&s, a), (true, true));

    s.run_ticks(s.ticks(1.0));

    assert_eq!(
        buffs_of(&s, a),
        (true, false),
        "each ends with its own time"
    );
}

#[test]
fn a_player_who_is_downed_loses_the_buffs_and_a_revived_player_has_none() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    let helper = s.player_at(2.5, 5.0);
    s.grant(a, Buff::Haste { secs: 30.0 });
    s.grant(a, Buff::Swift { secs: 30.0 });

    s.knock_down(a);
    assert_eq!(buffs_of(&s, a), (false, false));
    s.grant(a, Buff::Haste { secs: 30.0 });
    assert_eq!(
        buffs_of(&s, a),
        (false, false),
        "a downed player takes none"
    );

    s.hold_interact(helper);
    s.run_until(200, |sim| sim.player(a).is_some_and(|p| p.is_alive()));
    assert_eq!(buffs_of(&s, a), (false, false), "revived without them");
}

#[test]
fn a_buff_of_a_living_player_continues_into_the_next_day() {
    let plans = nights(&[&[1], &[1]]);
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.nights.plans = plans;
    });
    let a = s.player_at(2.0, 2.0);
    s.grant(a, Buff::Swift { secs: 600.0 });

    s.run_until(2000, |sim| {
        matches!(
            phase(sim),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });
    s.clear_monsters();
    s.run_until(400, |sim| matches!(phase(sim), Phase::Day { day: 2, .. }));

    assert_eq!(buffs_of(&s, a), (false, true));
}

#[test]
fn a_new_expedition_starts_without_buffs() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    s.grant(a, Buff::Haste { secs: 600.0 });
    s.grant(a, Buff::Swift { secs: 600.0 });

    s.restart_expedition();

    assert_eq!(buffs_of(&s, a), (false, false));
}

#[test]
fn a_player_who_returns_to_the_camp_has_no_buffs() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    s.grant(a, Buff::Haste { secs: 600.0 });
    s.sim.end_expedition(Outcome::Defeat);

    s.tap_confirm(a);

    assert!(in_camp(&s.sim));
    assert_eq!(buffs_of(&s, a), (false, false));
}

#[test]
fn the_time_of_a_buff_runs_while_the_player_is_away() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 5.0);
    s.grant(a, Buff::Swift { secs: 1.0 });
    s.sim.set_presence(a, Presence::Away);

    s.run_ticks(s.ticks(1.0));

    assert_eq!(buffs_of(&s, a), (false, false));
}

#[test]
fn the_time_of_a_buff_runs_in_the_camp_too() {
    let mut s = Scenario::camp(|_| {});
    let a = s.player_at(2.0, 5.0);
    s.grant(a, Buff::Haste { secs: 1.0 });
    s.run_ticks(19);
    assert_eq!(buffs_of(&s, a), (true, false));

    s.run_ticks(1);

    assert_eq!(buffs_of(&s, a), (false, false));
}
