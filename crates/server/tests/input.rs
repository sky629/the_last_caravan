//! 클라이언트가 보낸 입력이 서버를 거쳐 모든 접속의 스냅샷에서 플레이어의 움직임으로 나타나는 과정을 공개 API(`GameServer`)로만 확인합니다.
//!
//! 기대하는 값은 기본 수치에서 구합니다. 플레이어의 속도는 초당 4타일이고 서버는 초당 20번 계산하므로, 한 Tick에 0.2타일 움직입니다.
//! 플레이어는 (12, 9.5)와 (12, 14.5)에서 시작하고, 첫 무리는 3초(60 Tick) 뒤에 나타나므로 이 테스트의 몇 Tick 동안 몬스터는 없습니다.

mod common;

use caravan_protocol::{ClientMsg, ClientTransport, InputMsg, ServerLimits, WireVec2};
use caravan_rules::Tuning;
use common::{assert_near, input_with, last_snapshot, player, position, rig, rig_with_limits};

/// 한 Tick에 움직이는 거리(타일)입니다.
const STEP: f32 = 0.2;
#[test]
fn one_members_input_moves_that_player_in_the_other_members_snapshot() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, b_id) = rig.join_ok("test");

    a.send(&input_with(1, (1.0, 0.0), false));
    rig.run_ticks(1);

    let seen_by_b = last_snapshot(&mut b);
    assert_near(position(&seen_by_b, a_id).x, 12.0 + STEP, "A moved east");
    assert_near(position(&seen_by_b, a_id).y, 9.5, "A did not move north");
    assert_near(position(&seen_by_b, b_id).x, 12.0, "B stayed");
    assert_near(position(&seen_by_b, b_id).y, 14.5, "B stayed");
    assert_eq!(
        player(&seen_by_b, a_id).last_input_seq,
        1,
        "the snapshot says which input was applied"
    );
    assert_eq!(player(&seen_by_b, b_id).last_input_seq, 0);

    let seen_by_a = last_snapshot(&mut a);
    assert_eq!(seen_by_a, seen_by_b, "both members see the same state");
}

#[test]
fn a_held_input_keeps_moving_the_player_on_ticks_without_a_new_message() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    a.send(&input_with(1, (1.0, 0.0), false));
    rig.run_ticks(5);
    assert_near(
        position(&last_snapshot(&mut a), a_id).x,
        12.0 + 5.0 * STEP,
        "five ticks of one input",
    );
}

#[test]
fn an_input_that_is_not_newer_than_the_last_accepted_one_is_dropped() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    a.send(&input_with(5, (1.0, 0.0), false));
    rig.run_ticks(1);

    // 늦게 도착한 옛 입력과 같은 번호로 다시 온 입력입니다. 둘 다 북쪽으로 가라는 입력입니다.
    a.send(&input_with(3, (0.0, 1.0), false));
    a.send(&input_with(5, (0.0, 1.0), false));
    rig.run_ticks(1);

    let snapshot = last_snapshot(&mut a);
    assert_near(
        position(&snapshot, a_id).x,
        12.0 + 2.0 * STEP,
        "still walking east",
    );
    assert_near(position(&snapshot, a_id).y, 9.5, "never went north");
    assert_eq!(player(&snapshot, a_id).last_input_seq, 5);

    a.send(&input_with(6, (0.0, 1.0), false));
    rig.run_ticks(1);
    let snapshot = last_snapshot(&mut a);
    assert_near(
        position(&snapshot, a_id).y,
        9.5 + STEP,
        "a newer input is accepted",
    );
}

#[test]
fn inputs_between_two_ticks_merge_so_a_short_click_is_not_lost() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");

    // 클릭이 Tick 하나보다 짧아서, 다음 Tick이 오기 전에 누르고 뗍니다. 이동 방향은 더 새로운 입력을 따릅니다.
    a.send(&input_with(1, (1.0, 0.0), true));
    a.send(&input_with(2, (0.0, 1.0), false));
    rig.run_ticks(1);
    let snapshot = last_snapshot(&mut a);
    let swing = player(&snapshot, a_id).last_attack_tick;
    assert_eq!(swing, Some(snapshot.tick), "the click swung the weapon");
    assert_near(
        position(&snapshot, a_id).x,
        12.0,
        "axes come from the newest input",
    );
    assert_near(
        position(&snapshot, a_id).y,
        9.5 + STEP,
        "axes come from the newest input",
    );
    assert_eq!(player(&snapshot, a_id).last_input_seq, 2);

    // 뗀 상태가 이어지므로, 쿨다운(0.5초)이 끝난 뒤에도 다시 휘두르지 않습니다.
    rig.run_ticks(15);
    assert_eq!(
        player(&last_snapshot(&mut a), a_id).last_attack_tick,
        swing,
        "the click is not repeated on the ticks that follow"
    );
}

#[test]
fn a_player_who_never_clicked_has_not_swung() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    a.send(&input_with(1, (1.0, 0.0), false));
    rig.run_ticks(1);
    assert_eq!(player(&last_snapshot(&mut a), a_id).last_attack_tick, None);
}

#[test]
fn after_the_stale_limit_without_input_the_player_stands_still_until_a_new_input() {
    let limits = ServerLimits {
        input_stale_ticks: 4,
        ..ServerLimits::default()
    };
    let mut rig = rig_with_limits(Tuning::default(), limits);
    let (mut a, a_id) = rig.join_ok("test");

    a.send(&input_with(1, (1.0, 0.0), false));
    // 입력을 처리한 Tick 하나와, 새 입력 없이 마지막 입력을 다시 쓴 Tick 네 개입니다.
    rig.run_ticks(5);
    assert_near(
        position(&last_snapshot(&mut a), a_id).x,
        12.0 + 5.0 * STEP,
        "the last input is reused for 4 ticks",
    );

    rig.run_ticks(10);
    let snapshot = last_snapshot(&mut a);
    assert_near(
        position(&snapshot, a_id).x,
        12.0 + 5.0 * STEP,
        "stopped after the stale limit",
    );
    assert_eq!(
        player(&snapshot, a_id).last_input_seq,
        1,
        "a neutral step keeps the sequence number"
    );

    a.send(&input_with(2, (1.0, 0.0), false));
    rig.run_ticks(1);
    assert_near(
        position(&last_snapshot(&mut a), a_id).x,
        12.0 + 6.0 * STEP,
        "a new input moves the player again",
    );
}

#[test]
fn a_member_who_has_not_sent_input_does_not_hold_back_the_others() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, b_id) = rig.join_ok("test");
    a.send(&input_with(1, (1.0, 0.0), false));
    rig.run_ticks(3);

    let snapshot = last_snapshot(&mut b);
    assert_near(position(&snapshot, a_id).x, 12.0 + 3.0 * STEP, "A moved");
    assert_near(
        position(&snapshot, b_id).x,
        12.0,
        "B, who sent nothing, stood",
    );
    assert_eq!(player(&snapshot, b_id).last_input_seq, 0);
}

#[test]
fn a_move_longer_than_one_or_not_a_number_cannot_beat_the_speed_limit() {
    let mut rig = rig();
    let (mut fast, fast_id) = rig.join_ok("test");
    let (mut broken, broken_id) = rig.join_ok("test");

    fast.send(&input_with(1, (500.0, 0.0), false));
    broken.send(&ClientMsg::Input(InputMsg {
        seq: 1,
        move_dir: WireVec2::new(f32::NAN, f32::INFINITY),
        aim: WireVec2::new(f32::NAN, f32::NAN),
        attack: false,
        interact: false,
        build: false,
    }));
    rig.run_ticks(3);

    let snapshot = last_snapshot(&mut fast);
    assert_near(
        position(&snapshot, fast_id).x,
        12.0 + 3.0 * STEP,
        "a long vector is cut to length 1",
    );
    assert_near(
        position(&snapshot, broken_id).x,
        12.0,
        "NaN means standing still",
    );
    assert_near(
        position(&snapshot, broken_id).y,
        14.5,
        "NaN means standing still",
    );
}
