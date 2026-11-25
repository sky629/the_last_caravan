//! 자리를 유지하는 기능이 서버의 한도를 우회하지 못하는지 공개 API(`GameServer`)로만 확인합니다.
//! 끊긴 자리는 접속이 없어도 방에 남으므로, 접속 수의 한도에 들지 않습니다. 그래서 상대(주소)마다 유지하는 자리의 수와
//! 방의 수를 따로 묶고, 약속을 어긴 접속의 처리를 확인합니다.

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::rc::Rc;
use std::time::Instant;

use caravan_protocol::{
    ClientMsg, ClientTransport, LinkId, PROTOCOL_VERSION, PeerKey, RejectReason, RoomCode,
    ServerEvent, ServerLimits, ServerMsg, ServerTransport,
};
use caravan_rules::{PlayerId, Seed, Tuning};
use caravan_server::{GameServer, Notice, PredictableRandom, RoomSeeds, ServerError};
use common::{address, join_msg, rig_with, rig_with_limits};

/// 방의 정원이 `max_players`이고, 한 상대가 유지시킬 수 있는 자리가 `held`개이고, 방은 `max_rooms`개까지 두는 서버입니다.
/// 한 상대의 접속은 1개까지로 정해서, 한 상대가 방을 `1 + held`개까지 차지할 수 있습니다.
fn small_server(max_players: usize, held: usize, max_rooms: usize) -> common::Rig {
    let tuning = Tuning {
        max_players,
        ..Tuning::default()
    };
    let limits = ServerLimits {
        max_rooms,
        max_links_per_ip: 1,
        max_held_seats_per_ip: held,
        max_held_seats: max_rooms - 1,
        ..ServerLimits::default()
    };
    rig_with_limits(tuning, limits)
}

fn held_count(notices: &[Notice]) -> usize {
    notices
        .iter()
        .filter(|notice| matches!(notice, Notice::Held { .. }))
        .count()
}

fn left_count(notices: &[Notice]) -> usize {
    notices
        .iter()
        .filter(|notice| matches!(notice, Notice::Left { .. }))
        .count()
}

/// 방의 수가 한도에 닿으면 새 방을 만드는 입장은 이유와 함께 거절되고, 있는 방에 들어가는 입장은 받습니다.
/// 접속자가 없는 방도 없어지지 않으므로, 그 방의 자리를 가진 사람은 돌아올 수 있습니다.
#[test]
fn a_server_at_its_room_limit_refuses_a_new_room_but_neither_a_join_to_an_existing_one_nor_a_held_seat()
 {
    let mut rig = small_server(2, 2, 4);
    let mut clients = Vec::new();
    let mut welcomes = Vec::new();
    for (index, room) in ["first", "second", "third", "fourth"]
        .into_iter()
        .enumerate()
    {
        let (client, welcome) =
            rig.join_welcomed_from(address(index as u8 + 1), Some(room), "KANG", None);
        clients.push(client);
        welcomes.push(welcome);
    }
    rig.drop_link(clients.remove(0));

    let refused = ServerMsg::JoinRejected {
        reason: RejectReason::ServerFull,
    };
    let (_made_by_server, answer) = rig.join_as(None, "NEWCOMER");
    assert_eq!(answer, refused, "no room code: the server would make one");
    let (_named, answer) = rig.join_as(Some("fifth"), "NEWCOMER");
    assert_eq!(answer, refused, "a code that has no room yet");
    assert_eq!(rig.server.stats().rooms, 4, "nobody else's room was ended");

    let (_joined, joined) = rig.join_welcomed("second", "NEWCOMER", None);
    assert_eq!(
        joined.room, "second",
        "a room that exists still takes people"
    );
    let (_back, back) =
        rig.join_welcomed_from(address(1), Some("first"), "KANG", Some(welcomes[0].token));
    assert_eq!(
        (back.you, back.token),
        (welcomes[0].you, welcomes[0].token),
        "the empty room is still there for the one who holds its seat"
    );
}

/// 방이 하나 없어지면 새 방을 만들 수 있습니다. 한도는 서버가 지금 두고 있는 방의 수입니다.
#[test]
fn a_room_that_ends_makes_room_for_a_new_one() {
    let mut rig = small_server(2, 2, 4);
    let (mut kang, _) = rig.join_welcomed_from(address(1), Some("first"), "KANG", None);
    let _others: Vec<_> = ["second", "third", "fourth"]
        .into_iter()
        .enumerate()
        .map(|(index, room)| {
            rig.join_welcomed_from(address(index as u8 + 2), Some(room), "MIRO", None)
        })
        .collect();
    let (_refused, answer) = rig.join_as(Some("fifth"), "NEWCOMER");
    assert!(matches!(answer, ServerMsg::JoinRejected { .. }));

    kang.send(&ClientMsg::Leave);
    rig.pump();

    let (_made, made) = rig.join_welcomed("fifth", "NEWCOMER", None);
    assert_eq!(made.room, "fifth");
}

/// 한 상대가 입장과 끊기를 되풀이해도 접속자가 없는 방은 그 상대의 한도(`max_held_seats_per_ip`)를 넘어 쌓이지 않습니다.
/// 넘친 자리는 곧바로 없어지고, 입장은 계속 받아들여집니다.
#[test]
fn one_address_joining_and_dropping_again_and_again_holds_no_more_seats_than_its_limit() {
    let mut rig = small_server(2, 2, 10);
    let (mut held, mut given_up) = (0, 0);

    for _ in 0..12 {
        let (client, answer) = rig.join_as(None, "KANG");
        assert!(matches!(answer, ServerMsg::Welcome(_)), "{answer:?}");
        let notices = rig.drop_link(client);
        held += held_count(&notices);
        given_up += left_count(&notices);
        let stats = rig.server.stats();
        assert!(stats.members <= 2 && stats.rooms <= 2, "{stats:?}");
    }

    assert_eq!((held, given_up), (2, 10));
}

/// 한 상대가 한도를 채워도 다른 상대의 유지 중인 자리는 영향을 받지 않고, 돌아올 수 있습니다.
#[test]
fn another_address_keeps_its_held_seat_while_one_address_is_at_its_limit() {
    let mut rig = small_server(2, 2, 10);
    let (neighbor, neighbor_welcome) =
        rig.join_welcomed_from(address(2), Some("neighbor"), "MIRO", None);
    assert_eq!(held_count(&rig.drop_link(neighbor)), 1);

    let mut notices = Vec::new();
    for _ in 0..6 {
        let (client, _) = rig.join_welcomed_from(address(1), None, "KANG", None);
        notices.extend(rig.drop_link(client));
    }
    assert_eq!(
        (held_count(&notices), left_count(&notices)),
        (2, 4),
        "the other address holds its own two, whatever the neighbor holds"
    );

    let (_back, back) = rig.join_welcomed_from(
        address(2),
        Some("neighbor"),
        "MIRO",
        Some(neighbor_welcome.token),
    );
    assert_eq!(
        (back.you, back.token),
        (neighbor_welcome.you, neighbor_welcome.token),
        "the neighbor's seat was held all along"
    );
}

/// 한 주소의 네 명이 함께 끊겨도(한 집에서 한 방의 사람들이 함께) 네 자리가 모두 유지됩니다.
#[test]
fn four_people_of_one_address_dropping_together_keep_all_four_seats() {
    let mut rig = rig_with(Tuning::default());
    let mut clients = Vec::new();
    let mut welcomes = Vec::new();
    for name in ["KANG", "MIRO", "JUNE", "ALEX"] {
        let (client, welcome) = rig.join_welcomed_from(address(1), Some("home"), name, None);
        clients.push(client);
        welcomes.push(welcome);
    }

    let mut notices = Vec::new();
    for client in clients {
        notices.extend(rig.drop_link(client));
    }

    assert_eq!((held_count(&notices), left_count(&notices)), (4, 0));
    assert_eq!(rig.server.stats().members, 4);
    for (name, welcome) in ["KANG", "MIRO", "JUNE", "ALEX"].into_iter().zip(&welcomes) {
        let (_back, back) =
            rig.join_welcomed_from(address(1), Some("home"), name, Some(welcome.token));
        assert_eq!(back.you, welcome.you, "{name} is the same player");
    }
}

/// 한도에 닿은 상대도 자리가 끝나거나 돌아오면 다시 자리를 유지시킬 수 있습니다.
#[test]
fn an_address_at_its_limit_can_hold_again_after_a_seat_returns_or_ends() {
    let mut rig = small_server(2, 2, 10);
    let mut seats = Vec::new();
    for room in ["first", "second"] {
        let (client, welcome) = rig.join_welcomed_from(address(1), Some(room), "KANG", None);
        assert_eq!(held_count(&rig.drop_link(client)), 1);
        seats.push((room, welcome));
    }
    let (client, _) = rig.join_welcomed_from(address(1), Some("third"), "KANG", None);
    assert_eq!(
        left_count(&rig.drop_link(client)),
        1,
        "the third seat is over the limit"
    );

    // 한 자리가 돌아오면 그 자리는 더 이상 유지 중이 아니므로, 같은 상대가 다른 자리를 유지시킬 수 있습니다.
    let (room, welcome) = &seats[0];
    let (back, _) = rig.join_welcomed_from(address(1), Some(room), "KANG", Some(welcome.token));
    let (client, _) = rig.join_welcomed_from(address(1), Some("fourth"), "KANG", None);
    assert_eq!(
        held_count(&rig.drop_link(client)),
        1,
        "a seat that came back is not counted"
    );
    assert_eq!(
        left_count(&rig.drop_link(back)),
        1,
        "dropped again, it is over the limit"
    );

    // 유지하던 자리가 모두 끝나면 다시 유지시킬 수 있습니다.
    rig.run_ticks(rig.ticks(61.0));
    assert_eq!(rig.server.stats().members, 0);
    let (client, _) = rig.join_welcomed_from(address(1), Some("fifth"), "KANG", None);
    assert_eq!(held_count(&rig.drop_link(client)), 1);
}

/// 서버가 입력이 없다고 닫은 접속의 자리도 같은 상대의 한도에 듭니다.
#[test]
fn a_silent_link_the_server_closed_counts_for_its_address() {
    let mut rig = small_server(1, 1, 5);
    let (_silent, _) = rig.join_welcomed_from(address(1), Some("first"), "KANG", None);
    let notices = rig.run_ticks(rig.ticks(31.0));
    assert_eq!(held_count(&notices), 1, "{notices:?}");

    let (client, _) = rig.join_welcomed_from(address(1), Some("second"), "MIRO", None);
    assert_eq!(
        left_count(&rig.drop_link(client)),
        1,
        "the address already holds one seat"
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
        ServerEvent::Connected(link, PeerKey::of(IpAddr::V4(Ipv4Addr::LOCALHOST))),
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

/// 상대마다 유지하는 자리의 수, 서버 전체의 유지하는 자리의 수, 방의 수는 서로 맞아야 서버가 시작합니다.
#[test]
fn a_server_refuses_to_start_when_the_held_seat_and_room_limits_do_not_fit_together() {
    // 사람이 `max_players`명인 방에서, 한 상대의 접속은 `links`개, 한 상대가 유지하는 자리는 `per_ip`개, 서버 전체는 `total`개, 방은 `rooms`개입니다.
    let try_start =
        |max_players: usize, links: usize, per_ip: usize, total: usize, rooms: usize| {
            let (transport, _connector) = caravan_protocol::loopback();
            GameServer::new(
                Box::new(transport),
                Tuning {
                    max_players,
                    ..Tuning::default()
                },
                ServerLimits {
                    max_links_per_ip: links,
                    max_held_seats_per_ip: per_ip,
                    max_held_seats: total,
                    max_rooms: rooms,
                    ..ServerLimits::default()
                },
                RoomSeeds::Fixed(Seed(1)),
                common::random(),
                Instant::now(),
            )
            .map(|_| ())
        };
    let refused_for = |result: Result<(), ServerError>, what: &str| match result {
        Err(ServerError::InvalidLimits(why)) => assert!(why.contains(what), "{why}"),
        other => panic!("expected a refusal about {what}, got {other:?}"),
    };

    assert_eq!(try_start(4, 16, 4, 10, 21), Ok(()));
    refused_for(try_start(4, 16, 3, 10, 100), "max_held_seats_per_ip");
    // 한 상대가 열린 접속과 유지시킨 자리로 방을 모두 차지할 수 있습니다(16 + 4 = 20).
    refused_for(try_start(4, 16, 4, 10, 20), "max_rooms");
    refused_for(try_start(4, 16, 4, 3, 100), "max_held_seats");
    // 끊긴 자리만으로 방의 한도를 채울 수 있습니다.
    refused_for(try_start(4, 16, 4, 100, 100), "max_held_seats");
    refused_for(try_start(4, 16, 4, 101, 100), "max_held_seats");
    assert_eq!(try_start(4, 16, 4, 99, 100), Ok(()));
    let defaults = ServerLimits::default();
    assert_eq!(
        try_start(
            4,
            defaults.max_links_per_ip,
            defaults.max_held_seats_per_ip,
            defaults.max_held_seats,
            defaults.max_rooms
        ),
        Ok(()),
        "the defaults pass their own check"
    );
}

/// IPv6 주소는 같은 /64 대역이면 한 상대로 세어지고, 다른 /64 대역은 따로 세어집니다.
#[test]
fn ipv6_addresses_of_one_64_bit_prefix_count_as_one_address() {
    let mut rig = small_server(1, 1, 6);
    let v6 = |text: &str| -> IpAddr { text.parse().unwrap() };

    let (client, _) = rig.join_welcomed_from(v6("2001:db8:1:2::1"), None, "KANG", None);
    assert_eq!(held_count(&rig.drop_link(client)), 1);
    let (client, _) = rig.join_welcomed_from(v6("2001:db8:1:2:ffff::7"), None, "MIRO", None);
    assert_eq!(
        left_count(&rig.drop_link(client)),
        1,
        "another address of the same /64 is the same peer, which already holds one seat"
    );
    let (client, _) = rig.join_welcomed_from(v6("2001:db8:1:3::1"), None, "JUNE", None);
    assert_eq!(
        held_count(&rig.drop_link(client)),
        1,
        "another /64 is counted on its own"
    );
}

/// 서버 전체의 끊긴 자리가 한도에 닿으면 새로 끊긴 자리는 곧바로 없어지고, 이미 유지 중인 자리는 그대로여서 돌아올 수 있습니다.
/// 한도에 닿아도 새 방의 입장은 받아들여집니다.
#[test]
fn a_server_at_its_held_seat_limit_gives_up_new_drops_keeps_the_held_ones_and_still_admits_new_rooms()
 {
    let tuning = Tuning {
        max_players: 1,
        ..Tuning::default()
    };
    let limits = ServerLimits {
        max_rooms: 6,
        max_links_per_ip: 1,
        max_held_seats_per_ip: 1,
        max_held_seats: 2,
        ..ServerLimits::default()
    };
    let mut rig = rig_with_limits(tuning, limits);
    let mut seats = Vec::new();
    for (index, room) in ["first", "second"].into_iter().enumerate() {
        let (client, welcome) =
            rig.join_welcomed_from(address(index as u8 + 1), Some(room), "KANG", None);
        assert_eq!(held_count(&rig.drop_link(client)), 1);
        seats.push((room, welcome));
    }
    let (client, _) = rig.join_welcomed_from(address(3), Some("third"), "MIRO", None);
    assert_eq!(
        left_count(&rig.drop_link(client)),
        1,
        "another address, but the server holds as many as it may"
    );
    assert_eq!(rig.server.stats().members, 2, "the held two stayed");

    // 한도에 닿아 있어도 새 방에 들어갈 수 있습니다. 끊긴 자리만으로는 방의 한도를 채우지 못합니다.
    let mut open = Vec::new();
    for index in 0..4 {
        let (client, welcome) = rig.join_welcomed_from(address(10 + index), None, "JUNE", None);
        open.push((client, welcome));
    }
    assert_eq!(rig.server.stats().rooms, 6);
    let (_refused, answer) = rig.join_as(None, "LAST");
    assert!(
        matches!(
            answer,
            ServerMsg::JoinRejected {
                reason: RejectReason::ServerFull
            }
        ),
        "only now the rooms are all in use: {answer:?}"
    );

    // 유지 중이던 자리는 돌아올 수 있고, 돌아오면 한 자리가 비어서 새로 끊긴 자리가 다시 유지됩니다.
    let (room, welcome) = &seats[0];
    let (back, again) = rig.join_welcomed_from(address(1), Some(room), "KANG", Some(welcome.token));
    assert_eq!((again.you, again.token), (welcome.you, welcome.token));
    let (client, _) = open.remove(0);
    assert_eq!(held_count(&rig.drop_link(client)), 1);
    drop(back);
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
