//! 클라이언트가 보낸 입력이 서버를 거쳐 모든 접속의 스냅샷에서 플레이어의 움직임으로 나타나는 과정을 공개 API(`GameServer`)로만 확인합니다.
//!
//! 기대하는 값은 기본 수치에서 구합니다. 플레이어의 속도는 초당 4타일이고 서버는 초당 20번 계산하므로, 한 Tick에 0.2타일 움직입니다.
//! 플레이어는 (32, 29.5)와 (32, 34.5)에서 시작하고, 낮이 90초이므로 이 테스트의 몇 Tick 동안 몬스터는 없습니다.

mod common;

use caravan_protocol::{
    ClientMsg, ClientTransport, InputMsg, ServerLimits, Snapshot, WireLife, WirePhase,
    WireStructureKind, WireVec2, WireWeaponKind,
};
use caravan_rules::{Loadout, Tuning, WeaponKind as RulesWeapon};
use common::{
    assert_near, input_with, last_snapshot, player, position, press_ready, rig, rig_in_expedition,
    rig_with, rig_with_limits,
};

/// 한 Tick에 움직이는 거리(타일)입니다.
const STEP: f32 = 0.2;
/// 첫 플레이어가 시작하는 위치입니다.
const START_X: f32 = 32.0;
const START_Y: f32 = 29.5;
#[test]
fn one_members_input_moves_that_player_in_the_other_members_snapshot() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, b_id) = rig.join_ok("test");

    a.send(&input_with(1, (1.0, 0.0), false));
    rig.run_ticks(1);

    let seen_by_b = last_snapshot(&mut b);
    assert_near(position(&seen_by_b, a_id).x, START_X + STEP, "A moved east");
    assert_near(
        position(&seen_by_b, a_id).y,
        START_Y,
        "A did not move north",
    );
    assert_near(position(&seen_by_b, b_id).x, START_X, "B stayed");
    assert_near(position(&seen_by_b, b_id).y, 34.5, "B stayed");
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
        START_X + 5.0 * STEP,
        "five ticks of one input",
    );
}

#[test]
fn holding_sprint_makes_a_player_run_1_6_times_as_far_and_spends_stamina_in_every_snapshot() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, _) = rig.join_ok("test");
    let sprint = ClientMsg::Input(InputMsg {
        seq: 1,
        move_dir: WireVec2::new(1.0, 0.0),
        sprint: true,
        ..InputMsg::default()
    });

    a.send(&sprint);
    rig.run_ticks(5);

    let seen_by_b = last_snapshot(&mut b);
    assert_near(
        position(&seen_by_b, a_id).x,
        START_X + 5.0 * STEP * 1.6,
        "five ticks at the sprint speed",
    );
    let WireLife::Alive { stamina, .. } = player(&seen_by_b, a_id).life else {
        panic!("alive");
    };
    // 가득 찬 게이지로 80 Tick을 달리므로, 5 Tick을 달리면 6.25퍼센트가 줄어서 93.75퍼센트이고, 스냅샷은 내림해서 93을 보냅니다.
    assert_eq!(stamina, 93);
    assert_eq!(last_snapshot(&mut a), seen_by_b, "both members see it");
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
        START_X + 2.0 * STEP,
        "still walking east",
    );
    assert_near(position(&snapshot, a_id).y, START_Y, "never went north");
    assert_eq!(player(&snapshot, a_id).last_input_seq, 5);

    a.send(&input_with(6, (0.0, 1.0), false));
    rig.run_ticks(1);
    let snapshot = last_snapshot(&mut a);
    assert_near(
        position(&snapshot, a_id).y,
        START_Y + STEP,
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
    let swing = player(&snapshot, a_id)
        .last_attack
        .map(|attack| attack.tick);
    assert_eq!(swing, Some(snapshot.tick), "the click swung the weapon");
    assert_near(
        position(&snapshot, a_id).x,
        START_X,
        "axes come from the newest input",
    );
    assert_near(
        position(&snapshot, a_id).y,
        START_Y + STEP,
        "axes come from the newest input",
    );
    assert_eq!(player(&snapshot, a_id).last_input_seq, 2);

    // 뗀 상태가 이어지므로, 쿨다운(0.5초)이 끝난 뒤에도 다시 휘두르지 않습니다.
    rig.run_ticks(15);
    assert_eq!(
        player(&last_snapshot(&mut a), a_id)
            .last_attack
            .map(|attack| attack.tick),
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
    assert_eq!(
        player(&last_snapshot(&mut a), a_id)
            .last_attack
            .map(|attack| attack.tick),
        None
    );
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
        START_X + 5.0 * STEP,
        "the last input is reused for 4 ticks",
    );

    rig.run_ticks(10);
    let snapshot = last_snapshot(&mut a);
    assert_near(
        position(&snapshot, a_id).x,
        START_X + 5.0 * STEP,
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
        START_X + 6.0 * STEP,
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
    assert_near(position(&snapshot, a_id).x, START_X + 3.0 * STEP, "A moved");
    assert_near(
        position(&snapshot, b_id).x,
        START_X,
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
        interact_pressed: false,
        build: false,
        sprint: false,
        switch_weapon: false,
        toggle_ready: false,
        confirm: false,
        cycle_tower: false,
    }));
    rig.run_ticks(3);

    let snapshot = last_snapshot(&mut fast);
    assert_near(
        position(&snapshot, fast_id).x,
        START_X + 3.0 * STEP,
        "a long vector is cut to length 1",
    );
    assert_near(
        position(&snapshot, broken_id).x,
        START_X,
        "NaN means standing still",
    );
    assert_near(
        position(&snapshot, broken_id).y,
        34.5,
        "NaN means standing still",
    );
}

/// 검과 활을 두 칸에 가지고 시작하는 수치입니다. Q가 서버까지 한 번만 전달되는지 보려면 바꿀 무기가 있어야 합니다.
fn armed() -> Tuning {
    let mut tuning = Tuning::default();
    tuning.player.start_loadout = Loadout::new(RulesWeapon::Sword, Some(RulesWeapon::Bow));
    tuning
}

fn press_q(seq: u32) -> ClientMsg {
    ClientMsg::Input(InputMsg {
        seq,
        switch_weapon: true,
        ..InputMsg::default()
    })
}

#[test]
fn one_press_of_q_switches_the_weapon_once_and_the_ticks_without_a_message_do_not_switch_it_again()
{
    let mut rig = rig_with(armed());
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, _) = rig.join_ok("test");
    a.send(&input_with(1, (0.0, 0.0), false));
    rig.run_ticks(1);
    assert_eq!(
        player(&last_snapshot(&mut b), a_id).loadout.held,
        WireWeaponKind::Sword
    );

    a.send(&press_q(2));
    rig.run_ticks(1);
    assert_eq!(
        player(&last_snapshot(&mut b), a_id).loadout.held,
        WireWeaponKind::Bow
    );

    // 입력이 없는 Tick은 마지막 입력을 다시 쓰지만, 누른 순간은 다시 쓰지 않습니다.
    rig.run_ticks(5);
    assert_eq!(
        player(&last_snapshot(&mut b), a_id).loadout.held,
        WireWeaponKind::Bow,
        "the held input did not press Q again"
    );
}

#[test]
fn a_q_pressed_and_released_between_two_ticks_still_switches_the_weapon() {
    let mut rig = rig_with(armed());
    let (mut a, a_id) = rig.join_ok("test");

    a.send(&press_q(1));
    a.send(&input_with(2, (0.0, 0.0), false));
    rig.run_ticks(1);

    assert_eq!(
        player(&last_snapshot(&mut a), a_id).loadout.held,
        WireWeaponKind::Bow
    );
}

#[test]
fn a_late_or_repeated_input_with_q_does_not_switch_the_weapon() {
    let mut rig = rig_with(armed());
    let (mut a, a_id) = rig.join_ok("test");
    a.send(&input_with(5, (0.0, 0.0), false));
    rig.run_ticks(1);

    a.send(&press_q(3));
    a.send(&press_q(5));
    rig.run_ticks(1);

    assert_eq!(
        player(&last_snapshot(&mut a), a_id).loadout.held,
        WireWeaponKind::Sword
    );
}

/// 종류는 원정 중에만 바뀌므로, 이 시험들은 야영지를 거치지 않고 원정으로 시작하는 서버를 씁니다.
fn press_e(seq: u32) -> ClientMsg {
    ClientMsg::Input(InputMsg {
        seq,
        cycle_tower: true,
        ..InputMsg::default()
    })
}

#[test]
fn one_press_of_e_chooses_the_next_tower_kind_once_and_the_ticks_without_a_message_do_not_choose_again()
 {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, _) = rig.join_ok("test");
    a.send(&input_with(1, (0.0, 0.0), false));
    rig.run_ticks(1);
    assert_eq!(
        player(&last_snapshot(&mut b), a_id).build_kind,
        WireStructureKind::Watchtower
    );

    a.send(&press_e(2));
    rig.run_ticks(1);
    assert_eq!(
        player(&last_snapshot(&mut b), a_id).build_kind,
        WireStructureKind::SlowTower
    );

    // 입력이 없는 Tick은 마지막 입력을 다시 쓰지만, 누른 순간은 다시 쓰지 않습니다.
    rig.run_ticks(5);
    assert_eq!(
        player(&last_snapshot(&mut b), a_id).build_kind,
        WireStructureKind::SlowTower,
        "the held input did not press E again"
    );
}

#[test]
fn an_e_pressed_and_released_between_two_ticks_still_chooses_the_next_tower_kind() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut a, a_id) = rig.join_ok("test");

    a.send(&press_e(1));
    a.send(&input_with(2, (0.0, 0.0), false));
    rig.run_ticks(1);

    assert_eq!(
        player(&last_snapshot(&mut a), a_id).build_kind,
        WireStructureKind::SlowTower
    );
}

#[test]
fn a_late_or_repeated_input_with_e_does_not_choose_the_next_tower_kind() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut a, a_id) = rig.join_ok("test");
    a.send(&input_with(5, (0.0, 0.0), false));
    rig.run_ticks(1);

    a.send(&press_e(3));
    a.send(&press_e(5));
    rig.run_ticks(1);

    assert_eq!(
        player(&last_snapshot(&mut a), a_id).build_kind,
        WireStructureKind::Watchtower
    );
}

/// 마차가 곧바로 부서져서 결과 화면이 빨리 열리는 수치입니다. 낮이 거의 없고 마차의 HP가 1입니다.
fn quick_defeat() -> Tuning {
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 0.05;
    tuning.wagon.max_hp = 1;
    tuning
}

fn press_enter(seq: u32) -> ClientMsg {
    ClientMsg::Input(InputMsg {
        seq,
        confirm: true,
        ..InputMsg::default()
    })
}

fn ready_ids(snapshot: &Snapshot) -> Vec<u32> {
    match &snapshot.phase {
        WirePhase::Camp { ready, .. } => ready.clone(),
        other => panic!("the room is not in the camp: {other:?}"),
    }
}

fn confirmed_ids(snapshot: &Snapshot) -> Vec<u32> {
    match &snapshot.phase {
        WirePhase::Debrief { confirmed, .. } => confirmed.clone(),
        other => panic!("the room is not showing the result: {other:?}"),
    }
}

#[test]
fn one_press_of_r_makes_the_player_ready_once_and_the_ticks_without_a_message_do_not_press_it_again()
 {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, _) = rig.join_ok("test");

    a.send(&press_ready(1));
    rig.run_ticks(1);
    assert_eq!(ready_ids(&last_snapshot(&mut b)), vec![a_id]);

    // 입력이 없는 Tick은 마지막 입력을 다시 쓰지만, 누른 순간은 다시 쓰지 않습니다.
    rig.run_ticks(5);
    assert_eq!(
        ready_ids(&last_snapshot(&mut b)),
        vec![a_id],
        "the held input did not press R again"
    );
}

#[test]
fn an_r_pressed_and_released_between_two_ticks_still_makes_the_player_ready() {
    let mut rig = rig();
    let (mut a, a_id) = rig.join_ok("test");
    let (_b, _) = rig.join_ok("test");

    a.send(&press_ready(1));
    a.send(&input_with(2, (0.0, 0.0), false));
    rig.run_ticks(1);

    assert_eq!(ready_ids(&last_snapshot(&mut a)), vec![a_id]);
}

#[test]
fn the_enter_key_confirms_the_result_once_and_the_ticks_without_a_message_do_not_press_it_again() {
    let mut rig = rig_with(quick_defeat());
    let (mut a, a_id) = rig.join_ok("test");
    let (mut b, _) = rig.join_ok("test");
    a.send(&press_ready(1));
    b.send(&press_ready(1));
    let mut result = None;
    let mut seq = 2;
    for _ in 0..120 {
        seq = rig.run_ticks_talking(&mut [&mut a, &mut b], seq, 20);
        let snapshot = last_snapshot(&mut a);
        if matches!(snapshot.phase, WirePhase::Debrief { .. }) {
            result = Some(snapshot);
            break;
        }
    }
    let snapshot = result.expect("the expedition ended within two minutes");
    assert_eq!(confirmed_ids(&snapshot), Vec::<u32>::new());

    a.send(&press_enter(seq));
    rig.run_ticks(1);
    assert_eq!(confirmed_ids(&last_snapshot(&mut b)), vec![a_id]);

    rig.run_ticks(5);
    assert_eq!(
        confirmed_ids(&last_snapshot(&mut b)),
        vec![a_id],
        "the result is still open because B has not confirmed"
    );
}
