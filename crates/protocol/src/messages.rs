//! 클라이언트와 서버 사이의 메시지 전체입니다.
//!
//! 전송 형식(postcard)은 열거형의 변형을 이름이 아니라 선언 순서의 번호로 기록합니다.
//! 따라서 변형의 순서를 바꾸면 이전 빌드와 호환되지 않습니다. 새 변형은 끝에 추가하고, 형식이 바뀌면 `PROTOCOL_VERSION`을 올립니다.

use serde::{Deserialize, Serialize};

use crate::snapshot::Snapshot;
use crate::welcome::Welcome;

/// 전송 형식이 바뀔 때 올리는 번호입니다. 서버는 다른 번호로 입장을 요청한 클라이언트를 거절합니다.
pub const PROTOCOL_VERSION: u32 = 3;

/// 클라이언트에서 서버로 가는 메시지입니다. 이 열거형이 클라이언트가 서버에 할 수 있는 말의 전부입니다.
/// 위치, 피해량, 공격 대상, 명중 여부는 어디에도 없습니다. 서버가 모든 결과를 판정합니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// 접속한 뒤 처음 보내는 메시지입니다. `room`은 플레이어가 입력한 그대로의 문자열이고, 서버가 `RoomCode`로 해석합니다.
    Join {
        version: u32,
        room: String,
    },
    Input(InputMsg),
}

/// 플레이어가 한 Tick 동안 요청하는 조작입니다. 클라이언트가 서버에 보내는 게임 정보는 이 여섯 필드가 전부입니다.
/// 필드를 더하려면 이 구조체를 고쳐야 하므로 변경이 바로 드러납니다. 위치나 피해량 같은 필드를 넣어서는 안 됩니다.
///
/// 값은 믿을 수 없는 입력입니다. 서버가 `caravan_rules`의 `MoveDir::new`와 `AimDir::new`로 검사하고 고칩니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InputMsg {
    /// 클라이언트가 입력을 보낼 때마다 1씩 늘리는 순서 번호입니다. 서버는 이미 받은 번호보다 크지 않은 입력을 버립니다.
    pub seq: u32,
    /// 이동하려는 방향입니다. 길이가 1보다 길어도 서버가 1로 줄입니다.
    pub move_dir: WireVec2,
    /// 조준 방향입니다. 길이가 0이면 서버가 지금 보는 방향을 유지합니다.
    pub aim: WireVec2,
    /// 공격 버튼을 누르고 있습니다.
    pub attack: bool,
    /// 상호작용 키(F)를 누르고 있습니다.
    pub interact: bool,
    /// 건설 키(B)를 누르고 있습니다. 어느 타일에 세울지는 서버가 플레이어의 위치와 조준 방향에서 정하므로, 타일의 좌표는 보내지 않습니다.
    pub build: bool,
}

/// 입력의 방향 벡터입니다. 서버가 검사하기 전의 값이므로 NaN이나 무한대일 수 있습니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WireVec2 {
    pub x: f32,
    pub y: f32,
}

impl WireVec2 {
    pub const ZERO: WireVec2 = WireVec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> WireVec2 {
        WireVec2 { x, y }
    }
}

/// 서버에서 클라이언트로 가는 메시지입니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    /// 입장을 받아들였다는 답입니다. 세계의 고정된 정보를 담고 있으므로 접속마다 한 번만 보냅니다.
    Welcome(Welcome),
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
