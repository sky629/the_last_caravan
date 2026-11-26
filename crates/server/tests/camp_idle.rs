//! 야영지에서 조작이 없는 사람을 공개 API(`GameServer`)로만 확인합니다. 클라이언트가 보는 메시지(명단과 스냅샷)와 서버가 알리는 일만 봅니다.
//! 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.
//!
//! 클라이언트는 가만히 있어도 Tick마다 입력을 보냅니다. 이 시험의 사람은 세 가지 중 하나로 입력을 보냅니다.

mod common;

use caravan_protocol::{
    ClientMsg, ClientTransport, InputMsg, LoopbackClient, ServerLimits, WirePhase, WireVec2,
};
use caravan_rules::{Seed, Tuning};
use caravan_server::{GameServer, Notice, RoomSeeds, ServerError};
use common::{Rig, hear, input_with, last_snapshot, press_ready, rig, rig_in_expedition};
use std::time::Duration;

/// 입력을 보내는 방식입니다.
#[derive(Clone, Copy)]
enum Behavior {
    /// 아무것도 하지 않고 같은 입력만 보냅니다.
    Still,
    /// 1초마다 조준을 바꿉니다.
    Wiggling,
    /// 오른쪽 이동 키를 계속 누르고 있습니다.
    HoldingRight,
}

fn behaving(behavior: Behavior, seq: u32) -> ClientMsg {
    // 서버는 1초에 20 Tick이고, 이 시험은 Tick마다 입력을 하나씩 보냅니다.
    let second = seq / 20;
    let mut msg = InputMsg {
        seq,
        ..InputMsg::default()
    };
    match behavior {
        Behavior::Still => {}
        Behavior::Wiggling => {
            msg.aim = if second.is_multiple_of(2) {
                WireVec2::new(1.0, 0.0)
            } else {
                WireVec2::new(0.0, 1.0)
            };
        }
        Behavior::HoldingRight => msg.move_dir = WireVec2::new(1.0, 0.0),
    }
    ClientMsg::Input(msg)
}

/// 사람마다 `Behavior`대로 Tick마다 입력을 보내면서 `secs`초를 진행하고, 다음에 쓸 순서 번호를 돌려줍니다.
fn run(
    rig: &mut Rig,
    people: &mut [(&mut LoopbackClient, Behavior)],
    first_seq: u32,
    secs: f32,
) -> u32 {
    let mut seq = first_seq;
    for _ in 0..rig.ticks(secs) {
        for (client, behavior) in people.iter_mut() {
            client.send(&behaving(*behavior, seq));
        }
        seq += 1;
        rig.run_ticks(1);
    }
    seq
}

fn here_of(client: &mut LoopbackClient, id: u32) -> bool {
    hear(client)
        .last_roster()
        .iter()
        .find(|member| member.id == id)
        .unwrap_or_else(|| panic!("player {id} is not in the roster"))
        .here
}

/// KANG는 준비하고 계속 조작하고, MIRO는 야영지에서 가만히 있습니다. 60초가 지나면 MIRO가 빠져서 원정이 시작됩니다.
/// 원정이 시작된 뒤의 `Rig`와 두 사람, 다음 순서 번호, MIRO의 번호를 돌려줍니다.
fn start_without_the_one_who_does_nothing() -> (Rig, LoopbackClient, LoopbackClient, u32, u32) {
    let mut rig = rig();
    let (mut kang, _) = rig.join_welcomed("camp", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("camp", "MIRO", None);
    kang.send(&press_ready(1));

    let mut seq = run(
        &mut rig,
        &mut [
            (&mut kang, Behavior::Wiggling),
            (&mut miro, Behavior::Still),
        ],
        2,
        59.0,
    );
    assert!(
        matches!(
            last_snapshot(&mut kang).phase,
            WirePhase::Camp {
                starting_in: None,
                ..
            }
        ),
        "MIRO is still counted for 59 seconds"
    );

    seq = run(
        &mut rig,
        &mut [
            (&mut kang, Behavior::Wiggling),
            (&mut miro, Behavior::Still),
        ],
        seq,
        5.0,
    );
    (rig, kang, miro, seq, miro_welcome.you)
}

#[test]
fn a_person_who_does_nothing_in_the_camp_for_sixty_seconds_stops_holding_up_the_others() {
    let (mut rig, mut kang, _miro, _seq, miro_id) = start_without_the_one_who_does_nothing();

    let heard = hear(&mut kang);
    assert!(
        matches!(heard.last_snapshot().phase, WirePhase::Day { .. }),
        "KANG was ready and MIRO no longer counts: {:?}",
        heard.last_snapshot().phase
    );
    assert!(
        !heard
            .last_roster()
            .iter()
            .find(|member| member.id == miro_id)
            .unwrap()
            .here,
        "the others see MIRO as away"
    );
    assert_eq!(rig.server.stats().members, 2, "MIRO still has the seat");
    assert!(
        rig.run_ticks(1)
            .iter()
            .all(|notice| !matches!(notice, Notice::Left { .. })),
        "nobody left"
    );
}

#[test]
fn the_one_who_did_nothing_joins_the_expedition_at_once_when_he_operates() {
    let (mut rig, mut kang, mut miro, seq, miro_id) = start_without_the_one_who_does_nothing();
    hear(&mut kang);

    miro.send(&input_with(seq, (1.0, 0.0), false));
    kang.send(&behaving(Behavior::Wiggling, seq));
    rig.run_ticks(2);

    assert!(here_of(&mut kang, miro_id), "present again after one tick");
}

#[test]
fn a_dormant_person_who_returns_is_not_ready_and_the_ready_of_the_others_waits_for_him() {
    let mut rig = rig();
    let (mut kang, _) = rig.join_welcomed("camp", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("camp", "MIRO", None);
    miro.send(&press_ready(1));
    let seq = run(
        &mut rig,
        &mut [
            (&mut kang, Behavior::Wiggling),
            (&mut miro, Behavior::Still),
        ],
        2,
        62.0,
    );
    let ready_now = |client: &mut LoopbackClient| match last_snapshot(client).phase {
        WirePhase::Camp { ready, .. } => ready,
        other => panic!("expected the camp, got {other:?}"),
    };
    assert_eq!(
        ready_now(&mut kang),
        Vec::<u32>::new(),
        "MIRO's ready was taken back"
    );

    miro.send(&input_with(seq, (1.0, 0.0), false));
    kang.send(&press_ready(seq));
    run(
        &mut rig,
        &mut [
            (&mut kang, Behavior::Wiggling),
            (&mut miro, Behavior::Still),
        ],
        seq + 1,
        5.0,
    );

    let heard = hear(&mut kang);
    assert!(
        here_of_roster(&heard, miro_welcome.you),
        "MIRO is counted again"
    );
    let WirePhase::Camp {
        ready, starting_in, ..
    } = heard.last_snapshot().phase.clone()
    else {
        panic!("the camp goes on, MIRO is not ready");
    };
    assert_eq!(ready, vec![1], "only KANG is ready");
    assert_eq!(starting_in, None);
}

#[test]
fn a_solo_player_who_does_nothing_pauses_the_room_and_an_operation_starts_it_again() {
    let mut rig = rig();
    let (mut kang, welcome) = rig.join_welcomed("camp", "KANG", None);
    let seq = run(&mut rig, &mut [(&mut kang, Behavior::Still)], 1, 62.0);
    let before = last_snapshot(&mut kang).tick;

    let seq = run(&mut rig, &mut [(&mut kang, Behavior::Still)], seq, 5.0);
    assert!(
        hear(&mut kang).snapshots.is_empty(),
        "nobody is counted here, so the room stands still"
    );
    assert_eq!(rig.server.stats().members, 1, "the seat stays");

    kang.send(&input_with(seq, (1.0, 0.0), false));
    rig.run_ticks(1);

    let heard = hear(&mut kang);
    assert_eq!(heard.snapshots.len(), 1);
    assert_eq!(
        heard.last_snapshot().tick,
        before + 1,
        "it goes on from where it stood"
    );
    assert!(here_of_roster(&heard, welcome.you));
}

fn here_of_roster(heard: &common::Heard, id: u32) -> bool {
    heard
        .last_roster()
        .iter()
        .find(|member| member.id == id)
        .is_some_and(|member| member.here)
}

#[test]
fn holding_a_key_is_not_an_operation() {
    let mut rig = rig();
    let (mut kang, kang_welcome) = rig.join_welcomed("camp", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("camp", "MIRO", None);

    run(
        &mut rig,
        &mut [
            (&mut kang, Behavior::Wiggling),
            (&mut miro, Behavior::HoldingRight),
        ],
        1,
        61.0,
    );

    let heard = hear(&mut kang);
    assert!(here_of_roster(&heard, kang_welcome.you));
    assert!(
        !here_of_roster(&heard, miro_welcome.you),
        "a stuck key is no one at the keyboard"
    );
}

#[test]
fn standing_still_in_an_expedition_does_not_take_a_person_away() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, _) = rig.join_welcomed("night", "MIRO", None);

    run(
        &mut rig,
        &mut [(&mut kang, Behavior::Still), (&mut miro, Behavior::Still)],
        1,
        70.0,
    );

    let heard = hear(&mut kang);
    assert!(
        heard.last_roster().iter().all(|member| member.here),
        "only the camp counts operations"
    );
    assert!(matches!(
        heard.last_snapshot().phase,
        WirePhase::Day { .. } | WirePhase::Night { .. } | WirePhase::Dawn { .. }
    ));
}

/// 결과 화면이 시간이 다 되어 닫힐 때까지 아무것도 누르지 않은 사람도, 야영지에 들어선 때부터 60초를 새로 셉니다.
#[test]
fn the_time_without_an_operation_is_counted_from_entering_the_camp() {
    // 낮이 거의 없고 마차의 HP가 1이어서 원정이 곧바로 끝나고 결과 화면이 열립니다.
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 0.05;
    tuning.wagon.max_hp = 1;
    let mut rig = common::rig_with(tuning);
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, welcome) = rig.join_welcomed("night", "MIRO", None);
    let miro_id = welcome.you;
    kang.send(&press_ready(1));
    miro.send(&press_ready(1));
    rig.run_ticks(1);

    // 명단은 바뀔 때만 오므로, 마지막으로 받은 명단의 값을 기억합니다.
    let mut miro_here = true;
    let note = |heard: &common::Heard, miro_here: &mut bool| {
        if let Some(roster) = heard.rosters.last() {
            *miro_here = roster
                .members
                .iter()
                .any(|member| member.id == miro_id && member.here);
        }
    };
    let mut seq = 2;
    let mut seen_result = false;
    let mut back_in_camp = false;
    for _ in 0..400 {
        seq = run(
            &mut rig,
            &mut [(&mut kang, Behavior::Still), (&mut miro, Behavior::Still)],
            seq,
            1.0,
        );
        let heard = hear(&mut kang);
        note(&heard, &mut miro_here);
        match heard.last_snapshot().phase {
            WirePhase::Debrief { .. } => seen_result = true,
            WirePhase::Camp { .. } if seen_result => {
                back_in_camp = true;
                break;
            }
            _ => {}
        }
    }
    assert!(back_in_camp, "the result closed by its time limit");
    assert!(miro_here, "the minute on the result screen does not count");

    seq = run(
        &mut rig,
        &mut [
            (&mut kang, Behavior::Wiggling),
            (&mut miro, Behavior::Still),
        ],
        seq,
        55.0,
    );
    note(&hear(&mut kang), &mut miro_here);
    assert!(miro_here, "not a minute in the camp yet");
    run(
        &mut rig,
        &mut [
            (&mut kang, Behavior::Wiggling),
            (&mut miro, Behavior::Still),
        ],
        seq,
        7.0,
    );
    note(&hear(&mut kang), &mut miro_here);
    assert!(!miro_here, "a minute in the camp");
}

#[test]
fn a_server_refuses_a_camp_idle_time_of_less_than_one_tick() {
    let (transport, _connector) = caravan_protocol::loopback();
    let result = GameServer::new(
        Box::new(transport),
        Tuning::default(),
        ServerLimits {
            camp_idle: Duration::ZERO,
            ..ServerLimits::default()
        },
        RoomSeeds::Fixed(Seed(1)),
        common::random(),
        std::time::Instant::now(),
    );
    assert!(matches!(
        result,
        Err(ServerError::InvalidLimits(why)) if why.contains("camp_idle")
    ));
}
