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

fn limits_with_rooms(max_rooms: usize) -> ServerLimits {
    ServerLimits {
        max_rooms,
        ..ServerLimits::default()
    }
}

/// 방의 수가 한도에 닿으면 새 방을 만드는 입장은 이유와 함께 거절되고, 있는 방에 들어가는 입장은 받습니다.
#[test]
fn a_server_at_its_room_limit_refuses_a_new_room_and_still_admits_a_join_to_an_existing_one() {
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(2));
    let (_first, _) = rig.join_welcomed("first", "KANG", None);
    let (_second, _) = rig.join_welcomed("second", "MIRO", None);

    let refused = ServerMsg::JoinRejected {
        reason: RejectReason::ServerFull,
    };
    let (_made_by_server, answer) = rig.join_as(None, "NEWCOMER");
    assert_eq!(answer, refused, "no room code: the server would make one");
    let (_named, answer) = rig.join_as(Some("third"), "NEWCOMER");
    assert_eq!(answer, refused, "a code that has no room yet");
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
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(1));
    let (mut kang, _) = rig.join_welcomed("first", "KANG", None);
    let (_refused, answer) = rig.join_as(Some("second"), "MIRO");
    assert!(matches!(answer, ServerMsg::JoinRejected { .. }));

    kang.send(&ClientMsg::Leave);
    rig.pump();

    let (_made, made) = rig.join_welcomed("second", "MIRO", None);
    assert_eq!(made.room, "second");
}

/// 입장과 끊기를 되풀이해도 방은 한도를 넘어 쌓이지 않습니다. 자리가 끝나면 방이 사라지고 새 방을 만들 수 있습니다.
#[test]
fn joining_and_dropping_again_and_again_never_passes_the_room_limit() {
    let mut rig = rig_with_limits(Tuning::default(), limits_with_rooms(3));
    let (mut made, mut refused) = (0, 0);

    for _ in 0..20 {
        let (client, answer) = rig.join_as(None, "KANG");
        match answer {
            ServerMsg::Welcome(_) => made += 1,
            ServerMsg::JoinRejected {
                reason: RejectReason::ServerFull,
            } => refused += 1,
            other => panic!("unexpected answer {other:?}"),
        }
        rig.drop_link(client);
        assert!(
            rig.server.stats().rooms <= 3,
            "the held seats keep their rooms, but not past the limit"
        );
    }
    assert_eq!((made, refused), (3, 17));
    assert_eq!(rig.server.stats().members, 3, "the three held seats");

    rig.run_ticks(rig.ticks(61.0));
    assert_eq!(rig.server.stats().rooms, 0, "the held seats ended");
    let (_again, answer) = rig.join_as(None, "KANG");
    assert!(matches!(answer, ServerMsg::Welcome(_)));
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

#[test]
fn a_server_refuses_to_start_without_room_for_a_single_room() {
    let (transport, _connector) = caravan_protocol::loopback();
    let result = GameServer::new(
        Box::new(transport),
        Tuning::default(),
        limits_with_rooms(0),
        RoomSeeds::Fixed(Seed(1)),
        common::random(),
        Instant::now(),
    );
    assert!(matches!(
        result,
        Err(ServerError::InvalidLimits(why)) if why.contains("max_rooms")
    ));
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
