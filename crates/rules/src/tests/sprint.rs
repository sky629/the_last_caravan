//! 달리기를 확인합니다. 달리기 키(Shift)를 누르고 움직이면 걷는 속도의 1.6배로 움직이고, 달리는 동안 게이지가 줄고, 키를 떼면 게이지가 찹니다.
//!
//! 기본 수치에서 게이지가 가득 차면 4초(80 Tick) 달릴 수 있고, 빈 게이지는 10초(200 Tick)에 가득 찹니다.
//! 게이지는 `Player::stamina_percent`(0에서 100)로 읽습니다. 달리기로 줄어든 값은 Tick마다 가득 찬 값의 1.25%이고, 찰 때는 Tick마다 0.5%입니다.

use super::support::{Scenario, close};
use crate::expedition::Phase;
use crate::ids::PlayerId;
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::math::Rect;

/// 한 Tick을 달리는 거리(타일)입니다. 걷는 속도 4에 배율 1.6을 곱하고 Tick의 길이 0.05초를 곱한 값입니다.
const SPRINT_STEP: f32 = 0.32;
/// 한 Tick을 걷는 거리(타일)입니다.
const WALK_STEP: f32 = 0.2;
/// 가득 찬 게이지로 달릴 수 있는 Tick 수입니다.
const FULL_GAUGE_TICKS: u32 = 80;
/// 빈 게이지가 가득 차는 데 걸리는 Tick 수입니다.
const REFILL_TICKS: u32 = 200;

/// 가로가 60타일인 빈 들판입니다. 가득 찬 게이지로 달리는 거리(25.6타일)가 맵 안에 들어갑니다.
fn wide_field() -> Scenario {
    Scenario::with_tuning(|t| t.map.width = 60.0)
}

fn percent(s: &Scenario, id: PlayerId) -> u8 {
    s.player(id).stamina_percent(s.sim.tuning())
}

#[test]
fn the_default_sprint_is_1_6_times_walking_for_4_seconds_and_refills_in_10() {
    let t = crate::tuning::Tuning::default();
    assert_eq!(t.sprint.speed_multiplier, 1.6);
    assert_eq!(t.ticks(t.sprint.sprint_secs), FULL_GAUGE_TICKS);
    assert_eq!(t.ticks(t.sprint.refill_secs), REFILL_TICKS);
    assert!(close(
        t.player.speed * t.sprint.speed_multiplier * t.dt(),
        SPRINT_STEP
    ));
}

#[test]
fn a_new_player_has_a_full_gauge() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    assert_eq!(percent(&s, a), 100);
}

#[test]
fn holding_sprint_while_moving_moves_at_walking_speed_times_the_multiplier() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.hold_sprint(a, 1.0, 0.0);

    s.run_ticks(s.ticks(1.0));

    assert!(
        close(s.player(a).pos.x, 1.0 + 20.0 * SPRINT_STEP),
        "got {}",
        s.player(a).pos.x
    );
    assert!(close(s.player(a).pos.y, 5.0));
}

#[test]
fn sprinting_diagonally_is_no_faster_than_sprinting_straight() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.hold_sprint(a, 1.0, 1.0);

    s.run_ticks(10);

    let walked = s.player(a).pos.distance(crate::math::Vec2::new(1.0, 5.0));
    assert!(close(walked, 10.0 * SPRINT_STEP), "got {walked}");
}

#[test]
fn a_full_gauge_lasts_4_seconds_of_sprinting_and_then_the_player_walks() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.hold_sprint(a, 1.0, 0.0);

    s.run_ticks(FULL_GAUGE_TICKS - 1);
    assert!(percent(&s, a) > 0, "one tick of gauge is left");
    s.run_ticks(1);
    assert_eq!(percent(&s, a), 0);
    let after_the_gauge = s.player(a).pos.x;
    assert!(
        close(after_the_gauge, 1.0 + FULL_GAUGE_TICKS as f32 * SPRINT_STEP),
        "all 80 ticks were at the sprint speed, got {after_the_gauge}"
    );

    s.run_ticks(20);
    assert!(
        close(s.player(a).pos.x, after_the_gauge + 20.0 * WALK_STEP),
        "then walking speed"
    );
}

#[test]
fn an_empty_gauge_fills_in_10_seconds_while_the_key_is_up() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.drain_stamina(a);

    s.run_ticks(REFILL_TICKS - 1);
    assert_eq!(percent(&s, a), 99, "one tick short of full");
    s.run_ticks(1);
    assert_eq!(percent(&s, a), 100);
    s.run_ticks(50);
    assert_eq!(percent(&s, a), 100, "and no more than full");
}

#[test]
fn the_gauge_fills_while_the_player_walks_without_the_sprint_key() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.drain_stamina(a);
    s.hold_move(a, 1.0, 0.0);

    s.run_ticks(10);

    assert_eq!(percent(&s, a), 5);
    assert!(close(s.player(a).pos.x, 1.0 + 10.0 * WALK_STEP));
}

#[test]
fn holding_sprint_without_moving_neither_spends_nor_fills() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.hold_sprint(a, 1.0, 0.0);
    s.run_ticks(FULL_GAUGE_TICKS / 2);
    assert_eq!(percent(&s, a), 50);

    s.hold(
        a,
        PlayerInput {
            sprint: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(100);
    assert_eq!(
        percent(&s, a),
        50,
        "standing with the key down changes nothing"
    );

    s.hold(a, PlayerInput::default());
    s.run_ticks(20);
    assert_eq!(percent(&s, a), 60, "the key up fills 0.5 percent a tick");
}

#[test]
fn holding_sprint_with_an_empty_gauge_walks_every_tick_and_does_not_fill_it() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.drain_stamina(a);
    s.hold_sprint(a, 1.0, 0.0);

    for tick in 0..100 {
        let before = s.player(a).pos.x;
        s.run_ticks(1);
        assert!(
            close(s.player(a).pos.x - before, WALK_STEP),
            "tick {tick}: walking, not flickering between walking and sprinting"
        );
        assert_eq!(
            percent(&s, a),
            0,
            "tick {tick}: nothing fills while the key is down"
        );
    }
}

#[test]
fn a_downed_player_crawls_and_a_dead_player_stays_whatever_the_sprint_key_says() {
    let mut s = wide_field();
    let downed = s.player_at(1.0, 5.0);
    let dead = s.player_at(1.0, 8.0);
    // 살아 있는 플레이어가 없으면 원정이 실패해서 세계가 멈춥니다.
    s.player_at(30.0, 15.0);
    s.knock_down(downed);
    s.kill(dead);
    s.hold_sprint(downed, 1.0, 0.0);
    s.hold_sprint(dead, 1.0, 0.0);

    s.run_ticks(20);

    let crawl = s.sim.tuning().player.crawl_speed;
    assert!(close(s.player(downed).pos.x, 1.0 + crawl));
    assert!(close(s.player(dead).pos.x, 1.0));
    assert_eq!(percent(&s, downed), 0, "a downed player has no gauge");
    assert_eq!(percent(&s, dead), 0);
}

/// 마차의 서쪽 곁입니다. 마차의 가장자리까지 0.15타일이라 수리하거나 건설할 수 있는 곳입니다.
const BESIDE_THE_WAGON: (f32, f32) = (16.5, 18.5);

#[test]
fn a_player_who_repairs_or_builds_does_not_sprint_and_spends_nothing() {
    let busy = [
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
        PlayerInput {
            build: true,
            ..PlayerInput::default()
        },
    ];
    for doing in busy {
        let mut s = Scenario::open_field();
        s.set_wagon_hp(300);
        let a = s.player_at(BESIDE_THE_WAGON.0, BESIDE_THE_WAGON.1);
        s.hold(
            a,
            PlayerInput {
                move_dir: MoveDir::new(0.0, -1.0),
                aim: AimDir::new(-1.0, 0.0),
                sprint: true,
                ..doing
            },
        );
        let start = s.player(a).pos;

        s.run_ticks(20);

        assert!(s.player(a).interaction().is_some(), "{doing:?}");
        assert_eq!(s.player(a).pos, start, "{doing:?}: it did not move");
        assert_eq!(percent(&s, a), 100, "{doing:?}: and spent no stamina");
    }
}

#[test]
fn a_sprinting_player_does_not_pass_a_thin_wall() {
    let mut s = Scenario::with_tuning(|t| t.map.obstacles = vec![Rect::new(12.0, 1.0, 13.0, 20.0)]);
    let a = s.player_at(11.0, 10.0);
    s.hold_sprint(a, 1.0, 0.3);

    for _ in 0..100 {
        s.run_ticks(1);
        assert!(
            s.player(a).pos.x < 12.0,
            "crossed the wall: {:?}",
            s.player(a).pos
        );
    }
}

// 게이지가 가득 차는 때

#[test]
fn the_gauge_is_full_at_the_start_of_each_day() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.drain_stamina(a);
    s.sim.set_phase(Phase::Dawn {
        next_day: 2,
        ticks_left: 3,
    });

    s.run_ticks(2);
    assert!(matches!(s.sim.phase(), Phase::Dawn { .. }));
    assert_eq!(percent(&s, a), 1, "still the slow fill of the dawn");

    s.run_ticks(1);
    assert!(matches!(s.sim.phase(), Phase::Day { day: 2, .. }));
    assert_eq!(percent(&s, a), 100, "the new day fills it at once");
}

#[test]
fn the_gauge_is_full_at_a_new_expedition() {
    let mut s = wide_field();
    let a = s.player_at(1.0, 5.0);
    s.drain_stamina(a);
    s.sim.set_phase(Phase::Over {
        outcome: crate::expedition::Outcome::Defeat,
        ticks_left: 1,
    });

    s.run_ticks(1);

    assert_eq!(s.sim.expedition_number(), 2);
    assert_eq!(percent(&s, a), 100);
}

#[test]
fn a_revived_player_has_a_full_gauge() {
    let mut s = wide_field();
    let downed = s.player_at(5.0, 5.0);
    let reviver = s.player_at(5.5, 5.0);
    s.drain_stamina(downed);
    s.knock_down(downed);
    s.hold_interact(reviver);

    s.run_until(s.ticks(10.0), |sim| {
        sim.player(downed).is_some_and(|player| player.is_alive())
    });

    assert!(s.player(downed).is_alive());
    assert_eq!(percent(&s, downed), 100);
}
