//! 이름, 서버가 만드는 방 코드, 명단을 공개 API(`GameServer`)로만 확인합니다.
//! 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.

mod common;

use std::collections::VecDeque;

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, LoopbackClient, RejectReason, ServerLimits, ServerMsg,
    Welcome, WireMember, encode_server,
};
use caravan_rules::{Seed, Tuning};
use caravan_server::{Notice, RandomSource, RoomSeeds};
use common::{NAME, Rig, drain, join_msg, rig, rig_with, rig_with_random, snapshots};

/// 입장을 받아들인 답에서 입장 안내를 꺼냅니다.
fn welcome_of(answer: ServerMsg) -> Welcome {
    match answer {
        ServerMsg::Welcome(welcome) => *welcome,
        other => panic!("expected Welcome, got {other:?}"),
    }
}

/// 지금까지 받은 명단을 받은 순서대로, `(번호, 이름)`의 목록으로 돌려줍니다.
fn rosters(client: &mut LoopbackClient) -> Vec<Vec<(u32, String)>> {
    drain(client)
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Message(ServerMsg::Roster(roster)) => Some(
                roster
                    .members
                    .into_iter()
                    .map(|WireMember { id, name, .. }| (id, name))
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn who(list: &[(u32, &str)]) -> Vec<(u32, String)> {
    list.iter()
        .map(|&(id, name)| (id, name.to_string()))
        .collect()
}

#[test]
fn a_name_that_is_empty_too_long_or_has_a_forbidden_character_is_refused() {
    for name in ["", "   ", "ABCDEFGHIJKLM", "한글", "kang!", "tab\there"] {
        let mut rig = rig();
        let (mut client, answer) = rig.join_as(Some("test"), name);

        assert_eq!(
            answer,
            ServerMsg::JoinRejected {
                reason: RejectReason::BadName
            },
            "name {name:?}"
        );
        assert!(
            drain(&mut client).is_empty(),
            "the link stays open until the client has read the reason ({name:?})"
        );
        assert_eq!(
            rig.server.stats().rooms,
            0,
            "a refused name creates no room ({name:?})"
        );
    }
}

#[test]
fn a_name_of_twelve_letters_is_accepted_and_spaces_around_it_are_dropped() {
    let mut rig = rig();
    let (_a, answer) = rig.join_as(Some("test"), "ABCDEFGHIJKL");
    welcome_of(answer);
    let (mut b, answer) = rig.join_as(Some("test"), "  MIRO  ");
    welcome_of(answer);

    assert_eq!(
        rosters(&mut b).last().cloned(),
        Some(who(&[(1, "ABCDEFGHIJKL"), (2, "MIRO")]))
    );
}

#[test]
fn two_players_with_the_same_name_can_share_a_room() {
    let mut rig = rig();
    let (_a, answer) = rig.join_as(Some("test"), "KANG");
    welcome_of(answer);
    let (mut b, answer) = rig.join_as(Some("test"), "KANG");
    welcome_of(answer);

    assert_eq!(
        rosters(&mut b).last().cloned(),
        Some(who(&[(1, "KANG"), (2, "KANG")]))
    );
}

#[test]
fn a_join_without_a_room_code_creates_a_room_with_a_new_five_letter_code() {
    let mut rig = rig();
    let (_a, answer) = rig.join_as(None, "KANG");
    let first = welcome_of(answer);
    let (_b, answer) = rig.join_as(None, "MIRO");
    let second = welcome_of(answer);

    for code in [&first.room, &second.room] {
        assert_eq!(code.len(), 5, "{code}");
        assert!(
            code.chars()
                .all(|c| (c.is_ascii_lowercase() || c.is_ascii_digit()) && !"0o1li".contains(c)),
            "no letter that looks like another: {code}"
        );
    }
    assert_ne!(first.room, second.room);
    assert_eq!(
        rig.server.stats().rooms,
        2,
        "each blank room code made its own room"
    );
}

#[test]
fn a_friend_who_types_the_new_code_joins_the_same_room() {
    let mut rig = rig();
    let (mut kang, answer) = rig.join_as(None, "KANG");
    let code = welcome_of(answer).room;

    let (mut miro, answer) = rig.join_as(Some(&code.to_uppercase()), "MIRO");

    assert_eq!(
        welcome_of(answer).room,
        code,
        "the code is not case sensitive"
    );
    assert_eq!(rig.server.stats().rooms, 1);
    assert_eq!(
        rosters(&mut kang).last().cloned(),
        Some(who(&[(1, "KANG"), (2, "MIRO")]))
    );
    assert_eq!(
        rosters(&mut miro).last().cloned(),
        Some(who(&[(1, "KANG"), (2, "MIRO")]))
    );
}

/// 정해 둔 바이트를 차례로 주는 무작위 원천입니다. 바이트가 모자라면 시험이 실패합니다.
struct Queue(VecDeque<u8>);

impl RandomSource for Queue {
    fn fill(&mut self, bytes: &mut [u8]) {
        for slot in bytes {
            *slot = self.0.pop_front().expect("the queue ran out of bytes");
        }
    }
}

#[test]
fn a_new_code_is_never_the_code_of_a_room_that_already_exists() {
    // 먼저 들어온 사람의 표식 16바이트, 그다음에 만들 코드 둘(171은 t, 172는 u)과 표식 16바이트입니다.
    let bytes = [1; 16]
        .into_iter()
        .chain([171; 5])
        .chain([172; 5])
        .chain([2; 16]);
    let mut rig = rig_with_random(
        Tuning::default(),
        ServerLimits::default(),
        RoomSeeds::Fixed(Seed(1)),
        Box::new(Queue(bytes.collect())),
    );
    let (_squatter, answer) = rig.join_as(Some("ttttt"), "KANG");
    assert_eq!(welcome_of(answer).room, "ttttt");

    let (_new, answer) = rig.join_as(None, "MIRO");
    let made = welcome_of(answer).room;

    assert_eq!(made, "uuuuu", "the code the server first made was taken");
    assert_eq!(rig.server.stats().rooms, 2);
    assert_eq!(
        rig.server.stats().members,
        2,
        "the newcomer is alone in a new room, not in the room that held the code"
    );
}

#[test]
fn a_join_with_a_code_that_has_no_room_creates_that_room() {
    let mut rig = rig();
    let (_a, answer) = rig.join_as(Some("Friends"), "KANG");

    assert_eq!(welcome_of(answer).room, "friends");
    assert_eq!(rig.server.stats().rooms, 1);
}

#[test]
fn a_new_room_is_reported_with_its_code() {
    let mut rig = rig();
    let mut client = rig.connector.connect();
    client.poll();
    client.send(&join_msg(caravan_protocol::PROTOCOL_VERSION, None, NAME));
    let notices = rig.pump();
    let answer = match client.poll() {
        Some(ClientEvent::Message(msg)) => msg,
        other => panic!("expected an answer, got {other:?}"),
    };
    let welcome = welcome_of(answer);

    assert!(
        notices.iter().any(|notice| notice
            .to_string()
            .starts_with(&format!("room {} created", welcome.room))),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|notice| matches!(notice, Notice::Joined { .. })),
        "{notices:?}"
    );
}

#[test]
fn a_bad_room_code_is_still_refused_when_the_name_is_fine() {
    let mut rig = rig();
    let (_a, answer) = rig.join_as(Some("no spaces"), "KANG");

    assert_eq!(
        answer,
        ServerMsg::JoinRejected {
            reason: RejectReason::BadRoomCode
        }
    );
    assert_eq!(rig.server.stats().rooms, 0);
}

#[test]
fn the_roster_reaches_everyone_when_a_player_joins_and_when_a_player_leaves() {
    let mut rig = rig();
    let (mut a, answer) = rig.join_as(Some("test"), "KANG");
    welcome_of(answer);
    assert_eq!(
        rosters(&mut a),
        vec![who(&[(1, "KANG")])],
        "the first player gets the roster once, after the welcome"
    );

    let (mut b, answer) = rig.join_as(Some("test"), "MIRO");
    welcome_of(answer);
    let both = who(&[(1, "KANG"), (2, "MIRO")]);
    assert_eq!(rosters(&mut a), vec![both.clone()], "the one who was here");
    assert_eq!(rosters(&mut b), vec![both], "the newcomer, once");

    b.send(&ClientMsg::Leave);
    rig.pump();
    assert_eq!(
        rosters(&mut a),
        vec![who(&[(1, "KANG")])],
        "the one who stays learns that the other left"
    );
}

#[test]
fn every_new_roster_of_a_room_has_a_larger_number_than_the_one_before() {
    let mut rig = rig();
    let (mut a, _) = rig.join_as(Some("test"), "KANG");
    let (mut b, _) = rig.join_as(Some("test"), "MIRO");
    b.send(&ClientMsg::Leave);
    rig.pump();

    let revisions: Vec<u32> = drain(&mut a)
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Message(ServerMsg::Roster(roster)) => Some(roster.revision),
            _ => None,
        })
        .collect();

    assert_eq!(
        revisions,
        [1, 2, 3],
        "joined, joined by another, left alone"
    );
}

#[test]
fn the_welcome_comes_before_the_roster() {
    let mut rig = rig();
    let (mut a, answer) = rig.join_as(Some("test"), "KANG");
    welcome_of(answer);
    let events = drain(&mut a);
    assert!(
        matches!(&events[..], [ClientEvent::Message(ServerMsg::Roster(_))]),
        "the roster follows the welcome that join_as already read: {events:?}"
    );
}

#[test]
fn a_refused_join_sends_no_roster_to_the_players_already_there() {
    let mut rig = rig_with(Tuning {
        max_players: 1,
        ..Tuning::default()
    });
    let (mut a, answer) = rig.join_as(Some("test"), "KANG");
    welcome_of(answer);
    rosters(&mut a);

    let (_b, answer) = rig.join_as(Some("test"), "MIRO");
    assert!(matches!(answer, ServerMsg::JoinRejected { .. }));

    assert_eq!(rosters(&mut a), Vec::<Vec<(u32, String)>>::new());
}

/// 같은 방에서 같은 Tick을 진행했을 때 클라이언트 둘이 받는 스냅샷의 바이트 수입니다.
fn snapshot_sizes_with_names(first: &str, second: &str) -> (Vec<usize>, Vec<usize>) {
    let mut rig: Rig = rig();
    let (mut a, answer) = rig.join_as(Some("test"), first);
    welcome_of(answer);
    let (mut b, answer) = rig.join_as(Some("test"), second);
    welcome_of(answer);
    rig.run_ticks(30);
    let sizes = |client: &mut LoopbackClient| {
        snapshots(client)
            .into_iter()
            .map(|snapshot| encode_server(&ServerMsg::Snapshot(Box::new(snapshot))).len())
            .collect()
    };
    (sizes(&mut a), sizes(&mut b))
}

#[test]
fn the_length_of_the_names_does_not_change_the_size_of_a_snapshot() {
    let short = snapshot_sizes_with_names("A", "B");
    let long = snapshot_sizes_with_names("ABCDEFGHIJKL", "MNOPQRSTUVWX");

    assert!(!short.0.is_empty());
    assert_eq!(short, long, "names are in the roster, not in the snapshots");
}
