//! 클라이언트와 서버 사이의 메시지 전체입니다.
//!
//! 전송 형식(postcard)은 열거형의 변형을 이름이 아니라 선언 순서의 번호로 기록합니다.
//! 따라서 변형의 순서를 바꾸면 이전 빌드와 호환되지 않습니다. 새 변형은 끝에 추가하고, 형식이 바뀌면 `PROTOCOL_VERSION`을 올립니다.

use serde::{Deserialize, Serialize};

use crate::snapshot::Snapshot;

/// 전송 형식이 바뀔 때 올리는 번호입니다. 서버는 다른 번호로 입장을 요청한 클라이언트를 거절합니다.
pub const PROTOCOL_VERSION: u32 = 1;

/// 클라이언트에서 서버로 가는 메시지입니다. 위치나 피해량은 들어 있지 않습니다. 서버가 모든 결과를 판정합니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// 접속한 뒤 처음 보내는 메시지입니다. `room`은 플레이어가 입력한 그대로의 문자열이고, 서버가 `RoomCode`로 해석합니다.
    Join { version: u32, room: String },
    /// 플레이어의 입력입니다. `seq`는 클라이언트가 입력을 보낼 때마다 1씩 늘리는 순서 번호입니다.
    Input { seq: u32 },
}

/// 서버에서 클라이언트로 가는 메시지입니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    /// 입장을 받아들였다는 답입니다. `you`는 이 클라이언트의 플레이어 번호입니다.
    Welcome { you: u32, tick_hz: u32 },
    /// 입장을 거절했다는 답입니다. 서버는 이 메시지를 보낸 뒤 접속을 닫습니다.
    JoinRejected { reason: RejectReason },
    /// Tick마다 보내는 방 전체의 상태입니다.
    Snapshot(Snapshot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    RoomFull { capacity: u32 },
    BadRoomCode,
    VersionMismatch { server: u32 },
}
