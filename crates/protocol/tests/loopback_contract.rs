//! `ServerTransport`와 `ClientTransport`의 약속(transport.rs)을 프로세스 내부 전달로 확인합니다.

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, InputMsg, LinkId, RejectReason, ServerEvent,
    ServerMsg, ServerTransport, loopback,
};

fn input(seq: u32) -> ClientMsg {
    ClientMsg::Input(InputMsg {
        seq,
        ..InputMsg::default()
    })
}

/// 어떤 메시지든 상관없으므로 가장 작은 서버 메시지를 씁니다.
fn some_server_msg() -> ServerMsg {
    ServerMsg::JoinRejected {
        reason: RejectReason::BadRoomCode,
    }
}

#[test]
fn messages_flow_both_ways_in_order_after_connected() {
    let (mut server, mut connector) = loopback();
    let mut client = connector.connect();
    assert!(matches!(client.poll(), Some(ClientEvent::Connected)));
    client.send(&input(1));
    client.send(&input(2));

    let link = match server.poll() {
        Some(ServerEvent::Connected(link)) => link,
        other => panic!("expected Connected first, got {other:?}"),
    };
    assert!(
        matches!(server.poll(), Some(ServerEvent::Message(l, ClientMsg::Input(InputMsg { seq: 1, .. }))) if l == link)
    );
    assert!(
        matches!(server.poll(), Some(ServerEvent::Message(l, ClientMsg::Input(InputMsg { seq: 2, .. }))) if l == link)
    );
    assert!(server.poll().is_none());

    server.send(link, &some_server_msg());
    assert!(matches!(
        client.poll(),
        Some(ClientEvent::Message(ServerMsg::JoinRejected {
            reason: RejectReason::BadRoomCode
        }))
    ));
    assert!(client.poll().is_none());
}

#[test]
fn close_delivers_queued_messages_then_disconnects_both_ends_once() {
    let (mut server, mut connector) = loopback();
    let mut client = connector.connect();
    client.poll();
    let Some(ServerEvent::Connected(link)) = server.poll() else {
        panic!("expected Connected");
    };

    server.send(link, &some_server_msg());
    server.close(link);
    assert!(matches!(client.poll(), Some(ClientEvent::Message(_))));
    assert!(matches!(
        client.poll(),
        Some(ClientEvent::Disconnected { .. })
    ));
    assert!(client.poll().is_none());

    assert!(matches!(server.poll(), Some(ServerEvent::Disconnected(l)) if l == link));
    drop(client);
    assert!(
        server.poll().is_none(),
        "dropping a closed client adds no second Disconnected"
    );
}

#[test]
fn dropping_the_client_disconnects_it_on_the_server() {
    let (mut server, mut connector) = loopback();
    let client = connector.connect();
    let Some(ServerEvent::Connected(link)) = server.poll() else {
        panic!("expected Connected");
    };
    drop(client);
    assert!(matches!(server.poll(), Some(ServerEvent::Disconnected(l)) if l == link));
    assert!(server.poll().is_none());
}

#[test]
fn sending_to_an_unknown_link_does_nothing() {
    let (mut server, mut connector) = loopback();
    let mut client = connector.connect();
    client.poll();
    server.send(LinkId(99), &some_server_msg());
    server.close(LinkId(99));
    assert!(matches!(server.poll(), Some(ServerEvent::Connected(_))));
    assert!(server.poll().is_none());
    assert!(client.poll().is_none());
}
