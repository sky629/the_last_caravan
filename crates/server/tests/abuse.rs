//! 자리를 유지하는 기능이 서버의 한도를 우회하지 못하는지 공개 API(`GameServer`)로만 확인합니다.
//! 끊긴 자리는 접속이 없어도 방에 남으므로, 방의 수를 묶는 한도와 약속을 어긴 접속의 처리를 따로 확인합니다.

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

use caravan_protocol::{
    ClientMsg, ClientTransport, LinkId, PROTOCOL_VERSION, RejectReason, RoomCode, ServerEvent,
    ServerLimits, ServerMsg, ServerTransport,
};
use caravan_rules::{PlayerId, Seed, Tuning};
use caravan_server::{GameServer, Notice, PredictableRandom, RoomSeeds, ServerError};
use common::{join_msg, rig_with_limits};

/// 방의 수가 `max_rooms`이고, 한 주소가 둘 수 있는 접속은 그보다 하나 적습니다.
fn limits_with_rooms(max_rooms: usize) -> ServerLimits {
    ServerLimits {
        max_rooms,
        max_links_per_ip: max_rooms.saturating_sub(1),
        ..ServerLimits::default()
    }
}

fn refused() -> ServerMsg {
    ServerMsg::JoinRejected {
        reason: RejectReason::ServerFull,
    }
}

/// 방마다 접속한 사람이 있어서 없앨 방이 없으면, 새 방을 만드는 입장은 이유와 함께 거절되고 있는 방에 들어가는 입장은 받습니다.
#[test]
fn a_server_at_its_room_limit_with_everyone_connected_refuses_a_new_room_but_not_a_join_to_an_existing_one()
 {
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(2));
    let (_first, _) = rig.join_welcomed("first", "KANG", None);
    let (_second, _) = rig.join_welcomed("second", "MIRO", None);

    let (_made_by_server, answer) = rig.join_as(None, "NEWCOMER");
    assert_eq!(answer, refused(), "no room code: the server would make one");
    let (_named, answer) = rig.join_as(Some("third"), "NEWCOMER");
    assert_eq!(answer, refused(), "a code that has no room yet");
    assert_eq!(rig.server.stats().rooms, 2);

    let (_joined, joined) = rig.join_welcomed("first", "NEWCOMER", None);
    assert_eq!(
        joined.room, "first",
        "a room that exists still takes people"
    );
    assert_eq!(rig.server.stats().rooms, 2);
}

/// 방이 하나 없어지면 새 방을 만들 수 있습니다. 한도는 서버가 지금 두고 있는 방의 수입니다.
#[test]
fn a_room_that_ends_makes_room_for_a_new_one() {
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(2));
    let (mut kang, _) = rig.join_welcomed("first", "KANG", None);
    let (_miro, _) = rig.join_welcomed("second", "MIRO", None);
    let (_refused, answer) = rig.join_as(Some("third"), "NEWCOMER");
    assert_eq!(answer, refused());

    kang.send(&ClientMsg::Leave);
    rig.pump();

    let (_made, made) = rig.join_welcomed("third", "NEWCOMER", None);
    assert_eq!(made.room, "third");
}

/// 한도에 닿았을 때 새 방이 필요하면, 접속한 사람이 없는 방 가운데 가장 오래 비어 있던 방이 없어지고 그 자리에 새 방이 생깁니다.
/// 없어진 방의 표식으로 돌아온 사람은 새 플레이어로 들어갑니다. 방이 없으면 방 코드의 새 방에 새 플레이어로 들어가는 것과 같습니다.
#[test]
fn a_server_at_its_room_limit_ends_the_room_that_has_been_empty_longest_for_a_new_one() {
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(3));
    let (first, first_welcome) = rig.join_welcomed("first", "KANG", None);
    let (second, second_welcome) = rig.join_welcomed("second", "MIRO", None);
    let (_third, _) = rig.join_welcomed("third", "NEWCOMER", None);
    rig.drop_link(first);
    rig.run_ticks(rig.ticks(1.0));
    rig.drop_link(second);
    rig.run_ticks(rig.ticks(1.0));

    let (_made, answer) = rig.join_as(None, "FOURTH");
    let ServerMsg::Welcome(made) = answer else {
        panic!("expected a new room, got {answer:?}");
    };
    assert_ne!(made.room, "first");
    assert_eq!(rig.server.stats().rooms, 3, "never past the limit");
    assert_eq!(
        rig.server.stats().members,
        3,
        "the longest empty room went with its seat"
    );

    let (second_again, still) = rig.join_welcomed("second", "MIRO", Some(second_welcome.token));
    assert_eq!(
        (still.you, still.token),
        (second_welcome.you, second_welcome.token),
        "the room that was empty for less time is still there with its seat"
    );
    // 다시 비워 두어야 아래의 입장이 둘 곳을 얻습니다.
    rig.drop_link(second_again);
    let (_back, back) = rig.join_welcomed("first", "KANG", Some(first_welcome.token));
    assert_ne!(
        back.token, first_welcome.token,
        "the room is gone, so the old token opens nothing"
    );
    assert_eq!(back.you, 1, "a new player in a new room");
}

/// 방을 없앨 때 그 방의 자리마다 퇴장이, 방 하나에 제거가 서버가 알리는 일로 나옵니다.
#[test]
fn ending_an_empty_room_for_a_new_one_is_reported_like_any_other_departure() {
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(2));
    let (first, first_welcome) = rig.join_welcomed("first", "KANG", None);
    let (_second, _) = rig.join_welcomed("second", "MIRO", None);
    rig.drop_link(first);

    let (_client, notices) = rig.join_noticed("third");

    let room = RoomCode::parse("first").unwrap();
    let left = Notice::Left {
        room: room.clone(),
        player: PlayerId(first_welcome.you),
        members: 0,
    };
    let removed = Notice::RoomRemoved { room };
    let position = |wanted: &Notice| notices.iter().position(|notice| notice == wanted);
    assert!(
        position(&left) < position(&removed) && position(&left).is_some(),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|notice| matches!(notice, Notice::RoomCreated { .. })),
        "{notices:?}"
    );
}

/// 입장과 끊기를 되풀이해도 방은 한도를 넘어 쌓이지 않고, 입장은 모두 받아들여집니다. 접속이 열려 있는 방이 없어지는 일은 없습니다.
#[test]
fn joining_and_dropping_again_and_again_never_passes_the_room_limit() {
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(3));
    let (_kept, _) = rig.join_welcomed("kept", "KANG", None);

    for _ in 0..20 {
        let (client, answer) = rig.join_as(None, "VISITOR");
        assert!(matches!(answer, ServerMsg::Welcome(_)), "{answer:?}");
        rig.drop_link(client);
        rig.run_ticks(1);
        assert!(rig.server.stats().rooms <= 3);
    }

    assert_eq!(
        rig.server.stats().rooms,
        3,
        "the kept room and two held ones"
    );
    let (_again, answer) = rig.join_as(Some("kept"), "MIRO");
    assert!(
        matches!(answer, ServerMsg::Welcome(_)),
        "the room with someone in it was never ended"
    );
}

/// 약속을 어긴 접속(입장한 뒤 입장을 한 번 더 보냄)의 자리는 유지하지 않고 곧바로 내놓습니다.
#[test]
fn a_link_closed_for_breaking_the_rules_gives_up_its_seat_at_once() {
    let mut rig = rig_with_limits(Tuning::default(), ServerLimits::default());
    let (mut kang, welcome) = rig.join_welcomed("night", "KANG", None);

    kang.send(&join_msg(PROTOCOL_VERSION, Some("night"), "KANG"));
    let notices = rig.pump();

    let room = RoomCode::parse("night").unwrap();
    assert!(
        notices
            .iter()
            .any(|notice| matches!(notice, Notice::LinkClosed { .. })),
        "{notices:?}"
    );
    assert!(
        notices.contains(&Notice::Left {
            room,
            player: PlayerId(welcome.you),
            members: 0
        }),
        "the seat was given up: {notices:?}"
    );
    assert!(
        !notices
            .iter()
            .any(|notice| matches!(notice, Notice::Held { .. })),
        "{notices:?}"
    );
    assert_eq!(rig.server.stats().rooms, 0);
    let (_again, again) = rig.join_welcomed("night", "KANG", Some(welcome.token));
    assert_ne!(
        again.token, welcome.token,
        "the token of a seat given up opens nothing"
    );
}

/// 접속 방식이 한도를 넘었다고 닫은 접속(메시지 수, 크기, 읽을 수 없는 데이터)도 자리를 곧바로 내놓습니다.
#[test]
fn a_link_the_transport_closed_for_passing_a_limit_gives_up_its_seat_at_once() {
    let events = Rc::new(RefCell::new(VecDeque::new()));
    let mut server = GameServer::new(
        Box::new(Scripted(Rc::clone(&events))),
        Tuning::default(),
        ServerLimits::default(),
        RoomSeeds::Fixed(Seed(1)),
        Box::new(PredictableRandom::new(1)),
        Instant::now(),
    )
    .expect("the default numbers are valid");
    let now = Instant::now();
    let link = LinkId(1);
    events.borrow_mut().extend([
        ServerEvent::Connected(link),
        ServerEvent::Message(link, join_msg(PROTOCOL_VERSION, Some("night"), "KANG")),
    ]);
    server.pump(now);
    assert_eq!(server.stats().members, 1);

    events.borrow_mut().extend([
        ServerEvent::LimitReached(link, "sent too many messages".to_string()),
        ServerEvent::Disconnected(link),
    ]);
    let notices = server.pump(now);

    assert!(
        notices
            .iter()
            .any(|notice| matches!(notice, Notice::Left { members: 0, .. })),
        "{notices:?}"
    );
    assert!(
        !notices
            .iter()
            .any(|notice| matches!(notice, Notice::Held { .. })),
        "{notices:?}"
    );
    assert_eq!(server.stats().rooms, 0);
}

/// 입력이 오지 않는 접속은 서버가 닫아도 자리를 유지합니다. 창이 멈춘 사람이 돌아올 수 있습니다.
#[test]
fn a_silent_link_the_server_closed_keeps_its_seat() {
    let mut rig = rig_with_limits(Tuning::default(), ServerLimits::default());
    let (kang, _) = rig.join_welcomed("night", "KANG", None);

    let notices = rig.run_ticks(rig.ticks(31.0));

    assert!(
        notices
            .iter()
            .any(|notice| matches!(notice, Notice::Held { .. })),
        "{notices:?}"
    );
    assert_eq!(rig.server.stats().members, 1);
    drop(kang);
}

/// 접속한 사람이 있는 방은 접속 하나가 방 하나를 쓰므로, 방의 수는 한 주소가 둘 수 있는 접속의 수보다 커야 합니다.
#[test]
fn a_server_refuses_to_start_when_one_address_could_hold_every_room() {
    let try_start = |max_rooms: usize, max_links_per_ip: usize| {
        let (transport, _connector) = caravan_protocol::loopback();
        GameServer::new(
            Box::new(transport),
            Tuning::default(),
            ServerLimits {
                max_rooms,
                max_links_per_ip,
                ..ServerLimits::default()
            },
            RoomSeeds::Fixed(Seed(1)),
            common::random(),
            Instant::now(),
        )
        .map(|_| ())
    };

    assert_eq!(try_start(17, 16), Ok(()));
    for (max_rooms, max_links_per_ip) in [(16, 16), (3, 16), (0, 0)] {
        assert!(
            matches!(
                try_start(max_rooms, max_links_per_ip),
                Err(ServerError::InvalidLimits(why)) if why.contains("max_rooms")
            ),
            "{max_rooms} rooms, {max_links_per_ip} links per address"
        );
    }
    assert!(
        ServerLimits::default().max_rooms > ServerLimits::default().max_links_per_ip,
        "the defaults pass their own check"
    );
}

/// 정해 둔 이벤트를 차례로 돌려주는 접속 방식입니다. 접속 방식이 한도를 넘은 접속을 닫는 일처럼 가짜 접속으로 만들 수 없는 이벤트에 씁니다.
struct Scripted(Rc<RefCell<VecDeque<ServerEvent>>>);

impl ServerTransport for Scripted {
    fn poll(&mut self) -> Option<ServerEvent> {
        self.0.borrow_mut().pop_front()
    }

    fn send(&mut self, _link: LinkId, _msg: &ServerMsg) {}

    fn close(&mut self, _link: LinkId) {}
}
