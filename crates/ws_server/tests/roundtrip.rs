//! 127.0.0.1의 실제 소켓으로 `WsServer`를 확인합니다. 클라이언트는 tokio-tungstenite입니다.

use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, InputMsg, LinkId, PROTOCOL_VERSION, RejectReason, ServerEvent, ServerLimits,
    ServerMsg, ServerTransport, Snapshot, WireAngle, WireMonster, WireMonsterKind, WirePhase,
    WirePos, WireWagon, decode_server, encode_client,
};
use caravan_ws_server::WsServer;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

/// 어떤 메시지든 상관없는 테스트가 쓰는 가장 작은 서버 메시지입니다.
fn some_server_msg() -> ServerMsg {
    ServerMsg::JoinRejected {
        reason: RejectReason::BadRoomCode,
    }
}

/// 몬스터가 `monsters`마리인 스냅샷입니다. 몬스터 하나가 약 9바이트입니다.
fn snapshot_of(monsters: u32) -> Snapshot {
    Snapshot {
        tick: 1,
        expedition: 1,
        phase: WirePhase::Day {
            day: 1,
            secs_left: 90.0,
        },
        wagon: WireWagon { hp: 1, max_hp: 1 },
        stash: 0,
        towers: vec![],
        nodes: vec![],
        piles: vec![],
        players: vec![],
        monsters: (1..=monsters)
            .map(|id| WireMonster {
                id,
                kind: WireMonsterKind::Grunt,
                pos: WirePos { x: 1000, y: 1000 },
                facing: WireAngle(0),
                hp: 60,
            })
            .collect(),
    }
}

/// 게임 스레드처럼 `poll`을 반복해서 다음 이벤트를 기다립니다.
async fn next_event(server: &mut WsServer, within: Duration) -> ServerEvent {
    let deadline = Instant::now() + within;
    loop {
        if let Some(event) = server.poll() {
            return event;
        }
        assert!(
            Instant::now() < deadline,
            "no server event within {within:?}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn connected_link(server: &mut WsServer) -> LinkId {
    match next_event(server, Duration::from_secs(5)).await {
        ServerEvent::Connected(link) => link,
        other => panic!("expected Connected, got {other:?}"),
    }
}

/// 서버가 `link`를 한도 때문에 닫는 과정을 읽습니다. 이유를 알리는 `LimitReached`가 한 번 오고, 그 바로 뒤에 `Disconnected`가 와야 합니다.
async fn limit_reason_before_disconnect(server: &mut WsServer, link: LinkId) -> String {
    let reason = match next_event(server, Duration::from_secs(10)).await {
        ServerEvent::LimitReached(from, reason) if from == link => reason,
        other => panic!("expected the reason the link was closed, got {other:?}"),
    };
    match next_event(server, Duration::from_secs(10)).await {
        ServerEvent::Disconnected(gone) => assert_eq!(gone, link),
        other => panic!("expected Disconnected after the reason, got {other:?}"),
    }
    reason
}

#[tokio::test]
async fn messages_cross_a_real_socket_as_binary_frames_and_close_delivers_the_last_one() {
    let mut server = WsServer::bind("127.0.0.1:0", ServerLimits::default()).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (mut client, _) = tokio_tungstenite::connect_async(url).await.unwrap();

    let join = ClientMsg::Join {
        version: PROTOCOL_VERSION,
        room: "test".to_string(),
    };
    client
        .send(Message::binary(encode_client(&join)))
        .await
        .unwrap();

    let link = connected_link(&mut server).await;
    match next_event(&mut server, Duration::from_secs(5)).await {
        ServerEvent::Message(from, msg) => {
            assert_eq!(from, link);
            assert_eq!(msg, join);
        }
        other => panic!("expected the Join, got {other:?}"),
    }

    let answer = some_server_msg();
    server.send(link, &answer);
    server.close(link);

    match client.next().await {
        Some(Ok(Message::Binary(bytes))) => assert_eq!(decode_server(&bytes), Ok(answer)),
        other => panic!("expected a binary answer, got {other:?}"),
    }
    assert!(matches!(client.next().await, Some(Ok(Message::Close(_)))));
    match next_event(&mut server, Duration::from_secs(5)).await {
        ServerEvent::Disconnected(gone) => assert_eq!(gone, link),
        other => panic!("expected Disconnected, got {other:?}"),
    }
}

#[tokio::test]
async fn an_undecodable_frame_closes_only_that_link() {
    let mut server = WsServer::bind("127.0.0.1:0", ServerLimits::default()).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (mut bad, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let bad_link = connected_link(&mut server).await;
    let (mut good, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let good_link = connected_link(&mut server).await;

    bad.send(Message::binary(vec![0xFF, 0xFF])).await.unwrap();
    match next_event(&mut server, Duration::from_secs(5)).await {
        ServerEvent::Disconnected(gone) => assert_eq!(gone, bad_link),
        other => panic!("expected the bad link to close, got {other:?}"),
    }

    server.send(good_link, &some_server_msg());
    assert!(matches!(good.next().await, Some(Ok(Message::Binary(_)))));
}

#[tokio::test]
async fn a_client_that_stops_reading_is_disconnected() {
    let mut server = WsServer::bind("127.0.0.1:0", ServerLimits::default()).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (_silent_client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let link = connected_link(&mut server).await;

    // 메시지 하나가 약 60 KB이므로 500개(약 30 MB)는 운영체제의 소켓 버퍼에 다 들어가지 않습니다.
    let big = ServerMsg::Snapshot(snapshot_of(7_000));
    for _ in 0..500 {
        server.send(link, &big);
    }
    let reason = limit_reason_before_disconnect(&mut server, link).await;
    assert!(reason.contains("reads too slowly"), "{reason}");
}

/// 클라이언트가 보낸 입력 메시지 하나입니다.
fn input_frame(seq: u32) -> Message {
    Message::binary(encode_client(&ClientMsg::Input(InputMsg {
        seq,
        ..InputMsg::default()
    })))
}

#[tokio::test]
async fn a_full_outbox_reports_its_limit_after_the_messages_already_received_from_that_link() {
    let limits = ServerLimits {
        outbox_capacity: 1,
        ..ServerLimits::default()
    };
    let mut server = WsServer::bind("127.0.0.1:0", limits).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (mut client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let link = connected_link(&mut server).await;
    for seq in 1..=3 {
        client.send(input_frame(seq)).await.unwrap();
    }
    // 세 메시지가 게임 스레드에 도착해 있지만 아직 `poll`로 꺼내지 않은 상태에서 대기열을 채웁니다.
    tokio::time::sleep(Duration::from_millis(300)).await;
    for _ in 0..1000 {
        server.send(link, &some_server_msg());
    }

    let mut messages = 0;
    let reason = loop {
        match next_event(&mut server, Duration::from_secs(5)).await {
            ServerEvent::Message(from, _) if from == link => messages += 1,
            ServerEvent::LimitReached(from, reason) if from == link => break reason,
            other => panic!("unexpected event {other:?}"),
        }
    };
    assert_eq!(
        messages, 3,
        "every message received before the limit comes first"
    );
    assert!(reason.contains("reads too slowly"), "{reason}");
    assert!(matches!(
        next_event(&mut server, Duration::from_secs(5)).await,
        ServerEvent::Disconnected(gone) if gone == link
    ));
}

#[tokio::test]
async fn a_link_that_sends_faster_than_the_rate_limit_is_disconnected() {
    let limits = ServerLimits {
        max_messages_per_sec: 10,
        ..ServerLimits::default()
    };
    let mut server = WsServer::bind("127.0.0.1:0", limits).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (mut client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let link = connected_link(&mut server).await;

    for seq in 1..=50 {
        let input = encode_client(&ClientMsg::Input(InputMsg {
            seq,
            ..InputMsg::default()
        }));
        if client.send(Message::binary(input)).await.is_err() {
            break;
        }
    }

    let mut delivered = 0;
    let reason = loop {
        match next_event(&mut server, Duration::from_secs(5)).await {
            ServerEvent::Message(from, _) if from == link => delivered += 1,
            ServerEvent::LimitReached(from, reason) if from == link => break reason,
            other => panic!("unexpected event {other:?}"),
        }
    };
    assert_eq!(
        delivered, 10,
        "only the messages within the limit reach the game"
    );
    assert_eq!(reason, "sent more than 10 messages in one second");
    assert!(matches!(
        next_event(&mut server, Duration::from_secs(5)).await,
        ServerEvent::Disconnected(gone) if gone == link
    ));
}

#[tokio::test]
async fn a_link_that_sends_exactly_the_rate_limit_and_closes_is_not_reported_as_over_it() {
    let limits = ServerLimits {
        max_messages_per_sec: 10,
        ..ServerLimits::default()
    };
    let mut server = WsServer::bind("127.0.0.1:0", limits).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (mut client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let link = connected_link(&mut server).await;

    for seq in 1..=10 {
        client.send(input_frame(seq)).await.unwrap();
    }
    client.close(None).await.unwrap();

    let mut delivered = 0;
    loop {
        match next_event(&mut server, Duration::from_secs(5)).await {
            ServerEvent::Message(from, _) if from == link => delivered += 1,
            ServerEvent::Disconnected(gone) if gone == link => break,
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert_eq!(delivered, 10);
}

#[tokio::test]
async fn a_message_larger_than_the_size_limit_closes_the_link_with_the_reason() {
    let limits = ServerLimits {
        max_message_bytes: 64,
        ..ServerLimits::default()
    };
    let mut server = WsServer::bind("127.0.0.1:0", limits).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (mut client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let link = connected_link(&mut server).await;

    client.send(Message::binary(vec![0; 65])).await.unwrap();

    let reason = limit_reason_before_disconnect(&mut server, link).await;
    assert_eq!(reason, "sent a message larger than 64 bytes");
}

#[tokio::test]
async fn a_link_that_leaves_on_its_own_reports_no_limit() {
    let mut server = WsServer::bind("127.0.0.1:0", ServerLimits::default()).unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let link = connected_link(&mut server).await;

    drop(client);

    assert!(matches!(
        next_event(&mut server, Duration::from_secs(5)).await,
        ServerEvent::Disconnected(gone) if gone == link
    ));
}

#[tokio::test]
async fn sockets_over_the_link_cap_are_refused_until_a_stalled_handshake_times_out() {
    let limits = ServerLimits {
        max_links: 1,
        handshake_timeout: Duration::from_millis(300),
        ..ServerLimits::default()
    };
    let mut server = WsServer::bind("127.0.0.1:0", limits).unwrap();
    let url = format!("ws://{}", server.local_addr());

    // 연결 절차를 시작하지 않는 상대가 하나뿐인 자리를 차지합니다.
    let _stalled = tokio::net::TcpStream::connect(server.local_addr())
        .await
        .unwrap();
    assert!(
        tokio_tungstenite::connect_async(&url).await.is_err(),
        "no seat left while the stalled socket holds it"
    );

    tokio::time::sleep(Duration::from_millis(600)).await;
    let (_client, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    connected_link(&mut server).await;
}

#[tokio::test]
async fn one_ip_cannot_open_more_links_than_its_cap_and_a_closed_link_frees_its_seat() {
    let limits = ServerLimits {
        max_links_per_ip: 2,
        ..ServerLimits::default()
    };
    let mut server = WsServer::bind("127.0.0.1:0", limits).unwrap();
    let url = format!("ws://{}", server.local_addr());

    let (mut first, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let first_link = connected_link(&mut server).await;
    let (mut second, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let second_link = connected_link(&mut server).await;
    assert!(
        tokio_tungstenite::connect_async(&url).await.is_err(),
        "a third link from 127.0.0.1 is refused"
    );

    let answer = some_server_msg();
    server.send(first_link, &answer);
    server.send(second_link, &answer);
    assert!(
        matches!(first.next().await, Some(Ok(Message::Binary(_)))),
        "first stays open"
    );
    assert!(
        matches!(second.next().await, Some(Ok(Message::Binary(_)))),
        "second stays open"
    );

    drop(first);
    match next_event(&mut server, Duration::from_secs(5)).await {
        ServerEvent::Disconnected(gone) => assert_eq!(gone, first_link),
        other => panic!("expected the first link to close, got {other:?}"),
    }
    let (_third, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    connected_link(&mut server).await;
}

#[test]
fn dropping_the_server_releases_its_port() {
    let server = WsServer::bind("127.0.0.1:0", ServerLimits::default()).unwrap();
    let addr = server.local_addr();
    drop(server);

    assert!(
        std::net::TcpListener::bind(addr).is_ok(),
        "the address can be bound again right after the drop"
    );
}
