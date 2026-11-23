//! 전송 형식의 바이트를 고정하는 테스트입니다.
//!
//! 기대하는 바이트는 postcard의 규칙으로 직접 계산했습니다:
//! 열거형의 변형은 선언 순서의 번호(가변 길이 정수), 부호 없는 정수는 가변 길이 정수(7비트씩, 계속되면 최상위 비트가 1),
//! 부호 있는 정수는 지그재그 변환(0, -1, 1, -2 ... 를 0, 1, 2, 3 ... 으로) 뒤의 가변 길이 정수,
//! 문자열과 목록은 길이 뒤에 내용입니다.
//! 이 테스트가 실패하면 전송 형식이 바뀐 것입니다. 기대 바이트를 고칠 때 `PROTOCOL_VERSION`도 함께 올립니다.

use caravan_protocol::{
    ClientMsg, PROTOCOL_VERSION, RejectReason, ServerMsg, Snapshot, WirePos, decode_client,
    decode_server, encode_client, encode_server,
};

fn assert_client_wire(msg: ClientMsg, bytes: &[u8]) {
    assert_eq!(encode_client(&msg), bytes, "encoding of {msg:?}");
    assert_eq!(decode_client(bytes), Ok(msg));
}

fn assert_server_wire(msg: ServerMsg, bytes: &[u8]) {
    assert_eq!(encode_server(&msg), bytes, "encoding of {msg:?}");
    assert_eq!(decode_server(bytes), Ok(msg));
}

#[test]
fn the_protocol_version_matches_these_golden_bytes() {
    assert_eq!(PROTOCOL_VERSION, 1);
}

#[test]
fn join_is_variant_zero_then_version_then_room_text() {
    assert_client_wire(
        ClientMsg::Join {
            version: 1,
            room: "test".to_string(),
        },
        &[0x00, 0x01, 0x04, b't', b'e', b's', b't'],
    );
}

#[test]
fn input_carries_only_its_sequence_number() {
    // 300 = 0b10_0101100 -> 0xAC(하위 7비트 + 계속 비트), 0x02
    assert_client_wire(ClientMsg::Input { seq: 300 }, &[0x01, 0xAC, 0x02]);
}

#[test]
fn welcome_and_join_rejected_match_golden_bytes() {
    assert_server_wire(
        ServerMsg::Welcome {
            you: 2,
            tick_hz: 20,
        },
        &[0x00, 0x02, 0x14],
    );
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::RoomFull { capacity: 4 },
        },
        &[0x01, 0x00, 0x04],
    );
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::BadRoomCode,
        },
        &[0x01, 0x01],
    );
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::VersionMismatch { server: 1 },
        },
        &[0x01, 0x02, 0x01],
    );
}

#[test]
fn snapshot_matches_golden_bytes() {
    let snapshot = Snapshot {
        tick: 128,
        players: vec![1, 2],
        marker: WirePos { x: 400, y: -3 },
    };
    assert_server_wire(
        ServerMsg::Snapshot(snapshot),
        &[
            0x02, // Snapshot 변형
            0x80, 0x01, // tick 128
            0x02, 0x01, 0x02, // players: 길이 2, 1, 2
            0xA0, 0x06, // x 400 -> 지그재그 800
            0x05, // y -3 -> 지그재그 5
        ],
    );
}

#[test]
fn bytes_that_are_not_exactly_one_message_are_rejected() {
    assert!(decode_client(&[]).is_err(), "empty frame");
    assert!(decode_client(&[0x07]).is_err(), "unknown variant");
    assert!(decode_client(&[0x01, 0xAC]).is_err(), "truncated varint");
    assert!(decode_client(&[0x01, 0x05, 0x00]).is_err(), "trailing byte");
    assert!(
        decode_server(&[0x00, 0x01, 0x04, b't']).is_err(),
        "client bytes on the server channel"
    );
}
