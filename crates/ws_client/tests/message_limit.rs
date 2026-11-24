//! 127.0.0.1의 실제 소켓으로 `WsClient`가 서버의 메시지 크기를 제한하는지 확인합니다. 서버 역할은 tungstenite가 합니다.
//! 브라우저에서는 ewebsock이 프레임 크기 설정을 무시하므로 이 시험은 PC에서만 실행합니다.
#![cfg(not(target_arch = "wasm32"))]

use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientEvent, ClientTransport, RejectReason, ServerMsg, Snapshot, WireAngle, WireMonster,
    WireMonsterKind, WirePhase, WirePos, WireWagon, encode_server,
};
use caravan_ws_client::{MAX_SERVER_MESSAGE_BYTES, WsClient};
use tungstenite::Message;

/// 접속을 하나 받아서 `payloads`를 이진 메시지로 차례로 보내고, 클라이언트가 닫을 때까지 열어 두는 서버입니다. 주소를 돌려줍니다.
fn serve_once(payloads: Vec<Vec<u8>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let url = format!("ws://{}", listener.local_addr().expect("bound address"));
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("the client connects");
        let mut socket = tungstenite::accept(stream).expect("WebSocket handshake");
        for payload in payloads {
            if socket.send(Message::binary(payload)).is_err() {
                return;
            }
        }
        while socket.read().is_ok() {}
    });
    url
}

fn snapshot_of(monsters: u32) -> Snapshot {
    Snapshot {
        tick: 1,
        expedition: 1,
        phase: WirePhase::Fighting { wave: 1 },
        wave_count: 3,
        wagon: WireWagon { hp: 1, max_hp: 1 },
        stash: 0,
        towers: vec![],
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

/// 마지막 이벤트(`Disconnected`)가 올 때까지, 또는 `stop`이 참이 될 때까지 이벤트를 모읍니다.
fn collect(client: &mut WsClient, stop: impl Fn(&ClientEvent) -> bool) -> Vec<ClientEvent> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        match client.poll() {
            Some(event) => {
                let done = stop(&event) || matches!(event, ClientEvent::Disconnected { .. });
                events.push(event);
                if done {
                    return events;
                }
            }
            None => thread::sleep(Duration::from_millis(5)),
        }
    }
    panic!("no final event within 10 seconds: {events:?}");
}

#[test]
fn a_snapshot_with_thousands_of_monsters_fits_under_the_limit() {
    let snapshot = snapshot_of(10_000);
    let bytes = encode_server(&ServerMsg::Snapshot(snapshot.clone()));
    assert!(
        bytes.len() < MAX_SERVER_MESSAGE_BYTES,
        "{} bytes",
        bytes.len()
    );
    let mut client = WsClient::connect(&serve_once(vec![bytes])).unwrap();

    let events = collect(&mut client, |event| {
        matches!(event, ClientEvent::Message(ServerMsg::Snapshot(_)))
    });

    let Some(ClientEvent::Message(ServerMsg::Snapshot(received))) = events.last() else {
        panic!("expected the snapshot last, got {events:?}");
    };
    assert_eq!(received, &snapshot);
}

#[test]
fn a_message_above_the_limit_ends_the_connection_with_a_reason() {
    let small = encode_server(&ServerMsg::JoinRejected {
        reason: RejectReason::BadRoomCode,
    });
    let too_big = vec![0u8; MAX_SERVER_MESSAGE_BYTES + 1];
    let mut client = WsClient::connect(&serve_once(vec![small, too_big])).unwrap();

    let events = collect(&mut client, |_| false);

    assert!(
        matches!(events.first(), Some(ClientEvent::Connected)),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::Message(ServerMsg::JoinRejected { .. }))),
        "the small message before it still arrives: {events:?}"
    );
    let Some(ClientEvent::Disconnected { reason }) = events.last() else {
        panic!("expected Disconnected last, got {events:?}");
    };
    assert!(!reason.is_empty());
    assert_eq!(client.poll().map(|_| ()), None, "nothing after the end");
}
