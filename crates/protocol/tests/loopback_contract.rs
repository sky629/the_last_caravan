//! `ServerTransport`와 `ClientTransport`의 약속(transport.rs)을 프로세스 내부 전달로 확인합니다.

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, LinkId, ServerEvent, ServerMsg, ServerTransport,
    loopback,
};

#[test]
fn messages_flow_both_ways_in_order_after_connected() {
    let (mut server, mut connector) = loopback();
    let mut client = connector.connect();
    assert!(matches!(client.poll(), Some(ClientEvent::Connected)));
    client.send(&ClientMsg::Input { seq: 1 });
    client.send(&ClientMsg::Input { seq: 2 });

    let link = match server.poll() {
        Some(ServerEvent::Connected(link)) => link,
        other => panic!("expected Connected first, got {other:?}"),
    };
    assert!(
        matches!(server.poll(), Some(ServerEvent::Message(l, ClientMsg::Input { seq: 1 })) if l == link)
    );
    assert!(
        matches!(server.poll(), Some(ServerEvent::Message(l, ClientMsg::Input { seq: 2 })) if l == link)
    );
    assert!(server.poll().is_none());

    server.send(
        link,
        &ServerMsg::Welcome {
            you: 1,
            tick_hz: 20,
        },
    );
    assert!(matches!(
        client.poll(),
        Some(ClientEvent::Message(ServerMsg::Welcome {
            you: 1,
            tick_hz: 20
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

    server.send(
        link,
        &ServerMsg::Welcome {
            you: 1,
            tick_hz: 20,
        },
    );
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
    server.send(
        LinkId(99),
        &ServerMsg::Welcome {
            you: 1,
            tick_hz: 20,
        },
    );
    server.close(LinkId(99));
    assert!(matches!(server.poll(), Some(ServerEvent::Connected(_))));
    assert!(server.poll().is_none());
    assert!(client.poll().is_none());
}
