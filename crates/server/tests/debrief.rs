//! 끝난 원정의 결과(`ServerMsg::Debrief`)를 공개 API(`GameServer`)로만 확인합니다. 클라이언트가 받는 메시지만 봅니다.
//! 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.

mod common;

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, LoopbackClient, SeatToken, ServerMsg, WirePhase,
    WireReport,
};
use caravan_rules::Tuning;
use common::{Rig, hear, press_confirm, press_ready, rig_with};

/// 마차가 곧바로 부서져서 결과 화면이 빨리 열리는 수치입니다. 낮이 거의 없고 마차의 HP가 1입니다.
fn quick_defeat() -> Tuning {
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 0.05;
    tuning.wagon.max_hp = 1;
    tuning
}

/// 방 `night`에 KANG과 MIRO가 들어와서 둘 다 준비한 상태입니다.
struct Table {
    rig: Rig,
    kang: LoopbackClient,
    miro: LoopbackClient,
    miro_token: SeatToken,
    /// 다음 입력의 순서 번호입니다.
    seq: u32,
}

fn two_players_ready() -> Table {
    let mut rig = rig_with(quick_defeat());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    kang.send(&press_ready(1));
    miro.send(&press_ready(1));
    rig.run_ticks(1);
    Table {
        rig,
        kang,
        miro,
        miro_token: miro_welcome.token,
        seq: 2,
    }
}

/// 결과가 열리는 동안 클라이언트마다 받은 것을 모은 것입니다.
struct Opening {
    /// 클라이언트마다 받은 결과입니다. 순서는 넘긴 클라이언트의 순서입니다.
    reports: Vec<Vec<WireReport>>,
    /// `expedition`번째 원정의 첫 스냅샷의 Tick입니다. 원정은 이 Tick에 시작했습니다.
    began: u64,
    /// 결과 화면의 첫 스냅샷의 Tick입니다.
    opened: u64,
    /// 첫 클라이언트가 결과 화면의 첫 스냅샷을 받을 때 이미 결과를 받았는지입니다.
    report_first: bool,
}

/// `expedition`번째 원정의 결과 화면이 열릴 때까지 Tick마다 입력을 보내면서 진행하고, 열린 뒤에 `extra` Tick을 더 진행합니다.
/// `seq`는 다음 입력의 순서 번호이고, 진행한 만큼 늘어납니다.
fn play_until_the_result_of(
    rig: &mut Rig,
    clients: &mut [&mut LoopbackClient],
    seq: &mut u32,
    expedition: u32,
    extra: u32,
) -> Opening {
    let mut reports = vec![Vec::new(); clients.len()];
    let (mut began, mut opened, mut report_first) = (None, None, false);
    let mut after = 0;
    for _ in 0..600 {
        *seq = rig.run_ticks_talking(clients, *seq, 1);
        for (index, client) in clients.iter_mut().enumerate() {
            // 받은 순서대로 읽어서, 결과가 결과 화면의 첫 스냅샷보다 먼저 왔는지 봅니다.
            for event in common::drain(client) {
                match event {
                    ClientEvent::Message(ServerMsg::Debrief(report)) => reports[index].push(report),
                    ClientEvent::Message(ServerMsg::Snapshot(snapshot)) if index == 0 => {
                        if snapshot.expedition == expedition && began.is_none() {
                            began = Some(snapshot.tick);
                        }
                        let result = matches!(snapshot.phase, WirePhase::Debrief { .. });
                        if result && snapshot.expedition == expedition && opened.is_none() {
                            opened = Some(snapshot.tick);
                            report_first = !reports[0].is_empty();
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(opened) = opened {
            after += 1;
            if after > extra {
                return Opening {
                    reports,
                    began: began.expect("the expedition began before its result"),
                    opened,
                    report_first,
                };
            }
        }
    }
    panic!("the result of expedition {expedition} did not open within 600 ticks");
}

fn names(report: &WireReport) -> Vec<&str> {
    report.rows.iter().map(|row| row.name.as_str()).collect()
}

/// 지금까지 서버가 보낸 것을 종류의 순서로 적습니다.
fn kinds_heard(client: &mut LoopbackClient) -> Vec<&'static str> {
    std::iter::from_fn(|| client.poll())
        .map(|event| match event {
            ClientEvent::Message(ServerMsg::Roster(_)) => "roster",
            ClientEvent::Message(ServerMsg::Debrief(_)) => "debrief",
            ClientEvent::Message(ServerMsg::Snapshot(_)) => "snapshot",
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

#[test]
fn the_result_is_sent_once_to_everyone_in_the_room_when_it_opens() {
    let mut t = two_players_ready();

    let opening = play_until_the_result_of(
        &mut t.rig,
        &mut [&mut t.kang, &mut t.miro],
        &mut t.seq,
        1,
        30,
    );

    assert_eq!(opening.reports[0].len(), 1, "once, not every tick");
    assert_eq!(opening.reports[1], opening.reports[0], "the same for both");
    assert!(
        opening.report_first,
        "the result is in hand when the first snapshot of the result screen arrives"
    );
    let report = &opening.reports[0][0];
    assert_eq!(report.expedition, 1);
    assert_eq!(report.day, 1);
    assert_eq!(names(report), ["KANG", "MIRO"], "a row for each player");
    assert_eq!(
        u64::from(report.elapsed_secs),
        (opening.opened - opening.began) / 20,
        "the seconds from the first tick of the expedition to the tick the result opened"
    );
    assert!(
        report.elapsed_secs >= 3,
        "the first wave alone waits 3 seconds"
    );
    assert_eq!(report.tower_kills, 0);
}

#[test]
fn a_player_who_joins_during_the_result_receives_it_after_the_roster_and_only_once() {
    let mut t = two_players_ready();
    let opening = play_until_the_result_of(
        &mut t.rig,
        &mut [&mut t.kang, &mut t.miro],
        &mut t.seq,
        1,
        5,
    );

    let (mut late, _) = t.rig.join_welcomed("night", "HANA", None);

    assert_eq!(kinds_heard(&mut late), ["roster", "debrief"]);
    t.seq = t
        .rig
        .run_ticks_talking(&mut [&mut t.kang, &mut t.miro, &mut late], t.seq, 10);
    let heard = hear(&mut late);
    assert_eq!(heard.reports, [], "not sent again every tick");
    assert!(!heard.snapshots.is_empty());
    assert_eq!(opening.reports[0].len(), 1);
}

#[test]
fn the_table_is_the_players_of_the_end_of_the_expedition_and_a_late_joiner_is_not_in_it() {
    let mut t = two_players_ready();
    let opening = play_until_the_result_of(
        &mut t.rig,
        &mut [&mut t.kang, &mut t.miro],
        &mut t.seq,
        1,
        5,
    );

    let (mut late, _) = t.rig.join_welcomed("night", "HANA", None);

    let heard = hear(&mut late);
    assert_eq!(heard.reports, [opening.reports[0][0].clone()]);
    assert_eq!(names(&heard.reports[0]), ["KANG", "MIRO"]);
}

#[test]
fn a_player_who_leaves_during_the_result_stays_in_the_table_with_the_name() {
    let mut t = two_players_ready();
    play_until_the_result_of(
        &mut t.rig,
        &mut [&mut t.kang, &mut t.miro],
        &mut t.seq,
        1,
        5,
    );

    t.miro.send(&ClientMsg::Leave);
    t.seq = t.rig.run_ticks_talking(&mut [&mut t.kang], t.seq, 2);
    let (mut late, _) = t.rig.join_welcomed("night", "HANA", None);

    let heard = hear(&mut late);
    assert_eq!(
        names(&heard.reports[0]),
        ["KANG", "MIRO"],
        "MIRO left the room, and the table is still the one of the end of the expedition"
    );
    let in_the_room: Vec<&str> = heard
        .last_roster()
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    assert_eq!(in_the_room, ["KANG", "HANA"]);
}

#[test]
fn a_player_who_returns_to_a_held_seat_during_the_result_receives_it_again() {
    let mut t = two_players_ready();
    let opening = play_until_the_result_of(
        &mut t.rig,
        &mut [&mut t.kang, &mut t.miro],
        &mut t.seq,
        1,
        5,
    );
    t.rig.drop_link(t.miro);
    t.seq = t.rig.run_ticks_talking(&mut [&mut t.kang], t.seq, 5);

    let (mut back, _) = t.rig.join_welcomed("night", "MIRO", Some(t.miro_token));

    let heard = hear(&mut back);
    assert_eq!(heard.reports, [opening.reports[0][0].clone()]);
}

#[test]
fn the_result_is_dropped_when_the_result_closes_and_the_next_expedition_sends_a_new_one() {
    let mut t = two_players_ready();
    play_until_the_result_of(
        &mut t.rig,
        &mut [&mut t.kang, &mut t.miro],
        &mut t.seq,
        1,
        5,
    );
    t.kang.send(&press_confirm(t.seq));
    t.miro.send(&press_confirm(t.seq));
    t.seq = t
        .rig
        .run_ticks_talking(&mut [&mut t.kang, &mut t.miro], t.seq + 1, 5);
    let back_in_the_camp = common::last_snapshot(&mut t.kang);
    assert!(
        matches!(back_in_the_camp.phase, WirePhase::Camp { .. }),
        "everyone confirmed"
    );

    let (mut late, _) = t.rig.join_welcomed("night", "HANA", None);
    assert_eq!(
        kinds_heard(&mut late),
        ["roster"],
        "the camp has no result to show"
    );

    for client in [&mut t.kang, &mut t.miro, &mut late] {
        client.send(&press_ready(t.seq + 100));
    }
    t.seq += 101;
    let second = play_until_the_result_of(
        &mut t.rig,
        &mut [&mut t.kang, &mut t.miro, &mut late],
        &mut t.seq,
        2,
        5,
    );

    assert_eq!(second.reports[0].len(), 1);
    assert_eq!(second.reports[0][0].expedition, 2);
    assert_eq!(
        names(&second.reports[0][0]),
        ["KANG", "MIRO", "HANA"],
        "a new table of the people in the room now"
    );
}
