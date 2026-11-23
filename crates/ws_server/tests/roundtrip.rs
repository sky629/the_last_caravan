//! 127.0.0.1의 실제 소켓으로 `WsServer`를 확인합니다. 클라이언트는 tokio-tungstenite입니다.

use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, LinkId, PROTOCOL_VERSION, ServerEvent, ServerMsg, ServerTransport, Snapshot,
    WirePos, decode_server, encode_client,
};
use caravan_ws_server::WsServer;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

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

#[tokio::test]
async fn messages_cross_a_real_socket_as_binary_frames_and_close_delivers_the_last_one() {
    let mut server = WsServer::bind("127.0.0.1:0").unwrap();
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

    let welcome = ServerMsg::Welcome {
        you: 1,
        tick_hz: 20,
    };
    server.send(link, &welcome);
    server.close(link);

    match client.next().await {
        Some(Ok(Message::Binary(bytes))) => assert_eq!(decode_server(&bytes), Ok(welcome)),
        other => panic!("expected a binary Welcome, got {other:?}"),
    }
    assert!(matches!(client.next().await, Some(Ok(Message::Close(_)))));
    match next_event(&mut server, Duration::from_secs(5)).await {
        ServerEvent::Disconnected(gone) => assert_eq!(gone, link),
        other => panic!("expected Disconnected, got {other:?}"),
    }
}

#[tokio::test]
async fn an_undecodable_frame_closes_only_that_link() {
    let mut server = WsServer::bind("127.0.0.1:0").unwrap();
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

    server.send(
        good_link,
        &ServerMsg::Welcome {
            you: 2,
            tick_hz: 20,
        },
    );
    assert!(matches!(good.next().await, Some(Ok(Message::Binary(_)))));
}

#[tokio::test]
async fn a_client_that_stops_reading_is_disconnected() {
    let mut server = WsServer::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", server.local_addr());
    let (_silent_client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let link = connected_link(&mut server).await;

    // 메시지 하나가 약 60 KB이므로 500개(약 30 MB)는 운영체제의 소켓 버퍼에 다 들어가지 않습니다.
    let big = ServerMsg::Snapshot(Snapshot {
        tick: 1,
        players: (100_000..120_000).collect(),
        marker: WirePos { x: 0, y: 0 },
    });
    for _ in 0..500 {
        server.send(link, &big);
    }
    match next_event(&mut server, Duration::from_secs(10)).await {
        ServerEvent::Disconnected(gone) => assert_eq!(gone, link),
        other => panic!("expected the slow link to be dropped, got {other:?}"),
    }
}
