//! 쓰러짐, 부활, 출혈을 확인합니다.

use super::support::{Scenario, close};
use crate::ids::PlayerId;
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::math::Vec2;
use crate::player::Life;

fn bleed_left(s: &Scenario, id: PlayerId) -> u32 {
    match s.player(id).life {
        Life::Downed { bleed_left_ticks } => bleed_left_ticks,
        ref other => panic!("expected a downed player, got {other:?}"),
    }
}

fn revive_ticks(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().revive.revive_secs)
}

/// 쓰러진 플레이어 a와 그 곁에서 F를 누르고 있는 플레이어 b입니다.
fn downed_and_reviver() -> (Scenario, PlayerId, PlayerId) {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    s.knock_down(a);
    s.hold(
        b,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    (s, a, b)
}

#[test]
fn holding_f_for_the_revive_time_brings_the_player_back_with_30_percent_hp() {
    let (mut s, a, b) = downed_and_reviver();
    let needed = revive_ticks(&s);

    s.run_ticks(needed - 1);
    assert!(s.player(a).is_downed(), "one tick short of the revive time");

    s.run_ticks(1);
    assert!(s.player(a).is_alive(), "C7");
    assert_eq!(s.player(a).hp(), 30, "30% of 100 max HP, D21");
    assert!(
        s.player(b).interaction().is_none(),
        "the reviver is free again"
    );
}

#[test]
fn the_revive_progress_grows_while_f_is_held() {
    let (mut s, a, _) = downed_and_reviver();
    assert_eq!(s.sim.revive_progress(a), None, "nobody has started yet");

    s.run_ticks(revive_ticks(&s) / 2);

    let progress = s.sim.revive_progress(a).expect("a revive is in progress");
    assert!(close(progress, 0.5), "{progress}");
}

#[test]
fn a_revived_player_walks_and_attacks_again() {
    let (mut s, a, _) = downed_and_reviver();
    s.run_ticks(revive_ticks(&s));
    let monster = s.monster_at(11.5, 10.0);

    s.hold(
        a,
        PlayerInput {
            move_dir: MoveDir::new(0.0, -1.0),
            aim: AimDir::new(1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(s.ticks(1.0));

    assert!(
        close(s.player(a).pos.y, 10.0 - s.sim.tuning().player.speed),
        "full speed"
    );
    assert!(
        s.monster(monster).is_none_or(|m| m.hp < 60),
        "it swings again"
    );
}

#[test]
fn the_bleed_timer_does_not_run_while_someone_is_reviving() {
    let (mut s, a, _) = downed_and_reviver();
    let before = bleed_left(&s, a);

    s.run_ticks(60);

    assert_eq!(bleed_left(&s, a), before, "D22");
}

#[test]
fn releasing_f_cancels_the_revive_and_the_bleed_timer_resumes_where_it_stopped() {
    let (mut s, a, b) = downed_and_reviver();
    let before = bleed_left(&s, a);
    s.run_ticks(40);

    s.hold(b, PlayerInput::default());
    s.run_ticks(1);

    assert_eq!(s.sim.revive_progress(a), None, "C8: released");
    assert_eq!(
        bleed_left(&s, a),
        before - 1,
        "D22: the bleed timer resumed"
    );

    s.hold(
        b,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(1);
    let restarted = s.sim.revive_progress(a).unwrap();
    assert!(
        restarted < 0.01,
        "a new revive starts from zero, got {restarted}"
    );
}

#[test]
fn a_cancelled_revive_must_be_held_for_the_whole_time_again() {
    let (mut s, a, b) = downed_and_reviver();
    let needed = revive_ticks(&s);
    s.run_ticks(needed - 5);
    s.hold(b, PlayerInput::default());
    s.run_ticks(1);

    s.hold(
        b,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(needed - 1);
    assert!(s.player(a).is_downed(), "progress started over, D22");
    s.run_ticks(1);
    assert!(s.player(a).is_alive());
}

#[test]
fn the_reviver_going_down_cancels_the_revive() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.attack_damage = 100);
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    s.knock_down(a);
    s.hold(
        b,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(20);
    assert!(s.sim.revive_progress(a).is_some());

    s.monster_at(11.8, 10.0);
    s.run_ticks(2);

    assert!(
        s.player(b).is_downed(),
        "the monster's hit took all of b's HP"
    );
    assert_eq!(s.sim.revive_progress(a), None, "C8");
}

#[test]
fn being_hit_does_not_cancel_the_revive() {
    let (mut s, a, b) = downed_and_reviver();
    s.monster_at(11.8, 10.0);

    s.run_ticks(60);

    assert!(s.player(b).hp() < 100, "b took hits");
    assert!(s.player(b).is_alive());
    let progress = s.sim.revive_progress(a).expect("still reviving");
    assert!(
        close(progress, 60.0 / revive_ticks(&s) as f32),
        "D38, got {progress}"
    );
}

#[test]
fn the_downed_player_crawling_out_of_range_cancels_the_revive() {
    let (mut s, a, _) = downed_and_reviver();
    s.hold_move(a, -1.0, 0.0);

    // 기어가는 속도 0.8타일/초로, 거리 1.0에서 범위 1.5를 벗어나는 데 0.7초가 걸립니다.
    s.run_ticks(s.ticks(1.0));

    assert_eq!(s.sim.revive_progress(a), None, "C8");
    assert!(s.player(a).is_downed());
}

#[test]
fn a_reviver_stays_in_place_and_cannot_attack() {
    let (mut s, a, b) = downed_and_reviver();
    let monster = s.monster_at(11.8, 10.0);
    s.hold(
        b,
        PlayerInput {
            interact: true,
            attack: true,
            move_dir: MoveDir::new(1.0, 0.0),
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let start = s.player(b).pos;

    s.run_ticks(20);

    assert_eq!(s.player(b).pos, start, "C18: cannot move");
    assert_eq!(s.player(b).last_attack_tick, None, "C18: cannot attack");
    assert_eq!(s.monster(monster).unwrap().hp, 60);
    assert!(s.sim.revive_progress(a).is_some());
}

#[test]
fn a_player_who_holds_f_with_nothing_to_revive_can_still_move_and_attack() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.hold(
        a,
        PlayerInput {
            interact: true,
            move_dir: MoveDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );

    s.run_ticks(s.ticks(1.0));

    assert!(close(s.player(a).pos.x, 5.0 + s.sim.tuning().player.speed));
}

#[test]
fn a_downed_player_crawls_slowly_and_cannot_attack() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.player_at(15.0, 2.0);
    s.knock_down(a);
    let monster = s.monster_at(5.5, 5.0);
    s.hold(
        a,
        PlayerInput {
            move_dir: MoveDir::new(0.0, 1.0),
            aim: AimDir::new(1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    );

    s.run_ticks(s.ticks(2.0));

    let crawled = s.player(a).pos.y - 5.0;
    assert!(
        close(crawled, 2.0 * s.sim.tuning().player.crawl_speed),
        "D37, crawled {crawled}"
    );
    assert_eq!(s.player(a).last_attack_tick, None, "C18");
    assert_eq!(s.monster(monster).unwrap().hp, 60);
}

#[test]
fn bleeding_out_makes_the_player_dead_on_the_last_tick_of_the_timer() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    s.player_at(15.0, 15.0); // 살아 있는 플레이어가 있어야 원정이 실패하지 않습니다.
    s.knock_down(a);
    let bleed = s.ticks(s.sim.tuning().revive.bleed_secs);
    assert_eq!(bleed, 600, "30 seconds at 20 Hz, D21");

    s.run_ticks(bleed - 1);
    assert!(s.player(a).is_downed());

    s.run_ticks(1);
    assert_eq!(s.player(a).life, Life::Dead, "C9");
}

#[test]
fn a_dead_player_cannot_be_revived() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let c = s.player_at(10.5, 10.0);
    s.kill(a);
    s.hold(
        c,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );

    s.run_ticks(revive_ticks(&s) + 10);

    assert_eq!(s.player(a).life, Life::Dead, "D39");
    assert_eq!(s.sim.revive_progress(a), None);
    assert!(s.player(c).interaction().is_none());
}

#[test]
fn only_one_player_can_revive_a_downed_player_at_a_time() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(11.0, 10.0);
    let c = s.player_at(9.0, 10.0);
    s.knock_down(a);
    for id in [b, c] {
        s.hold(
            id,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
    }

    s.run_ticks(revive_ticks(&s) / 2);

    let progress = s.sim.revive_progress(a).unwrap();
    assert!(
        close(progress, 0.5),
        "two revivers do not double the speed, got {progress}"
    );
    let helping = [b, c]
        .iter()
        .filter(|id| s.player(**id).interaction().is_some())
        .count();
    assert_eq!(helping, 1);
}

#[test]
fn a_revive_stops_when_the_downed_player_leaves_the_room() {
    let (mut s, a, b) = downed_and_reviver();
    s.run_ticks(20);

    s.sim.remove_player(a);
    s.run_ticks(1);

    assert!(s.player(b).interaction().is_none(), "D25");
}

#[test]
fn a_player_far_from_the_downed_ally_does_not_start_a_revive() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    let b = s.player_at(8.0, 5.0);
    s.knock_down(a);
    s.hold(
        b,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    );

    s.run_ticks(10);

    assert_eq!(s.sim.revive_progress(a), None);
    assert_eq!(s.player(b).pos, Vec2::new(8.0, 5.0));
}
