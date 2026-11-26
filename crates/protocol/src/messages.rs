//! 클라이언트와 서버 사이의 메시지 전체입니다.
//!
//! 전송 형식(postcard)은 열거형의 변형을 이름이 아니라 선언 순서의 번호로 기록합니다.
//! 따라서 변형의 순서를 바꾸면 이전 빌드와 호환되지 않습니다. 새 변형은 끝에 추가하고, 형식이 바뀌면 `PROTOCOL_VERSION`을 올립니다.

use serde::{Deserialize, Serialize};

use crate::report::WireReport;
use crate::roster::WireRoster;
use crate::seat_token::SeatToken;
use crate::snapshot::Snapshot;
use crate::welcome::Welcome;

/// 전송 형식이 바뀔 때 올리는 번호입니다. 서버는 다른 번호로 입장을 요청한 클라이언트를 거절합니다.
pub const PROTOCOL_VERSION: u32 = 11;

/// 클라이언트에서 서버로 가는 메시지입니다. 이 열거형이 클라이언트가 서버에 할 수 있는 말의 전부입니다.
/// 위치, 피해량, 공격 대상, 명중 여부는 어디에도 없습니다. 서버가 모든 결과를 판정합니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// 접속한 뒤 처음 보내는 메시지입니다. 모든 값은 플레이어가 입력한 그대로이고, 서버가 검사합니다.
    Join {
        version: u32,
        /// 방 코드입니다. 없으면 서버가 쓰이지 않는 새 코드로 방을 만듭니다. 있으면 `RoomCode`로 해석하고, 그 방이 없으면 만듭니다.
        room: Option<String>,
        /// 플레이어의 이름입니다. 서버가 `PlayerName`으로 해석하고, 맞지 않으면 `BadName`으로 거절합니다.
        name: String,
        /// 이 기기가 그 방에서 받았던 자리의 표식입니다. 서버가 아는 자리의 표식과 같으면 그 자리로 돌아가고,
        /// 없거나 틀리면 새 플레이어로 들어갑니다. 표식은 비밀 값이므로 `Debug`로 출력해도 값이 보이지 않습니다.
        token: Option<SeatToken>,
    },
    Input(InputMsg),
    /// 플레이어가 스스로 방을 나갑니다. 서버는 자리를 곧바로 내놓고, 표식은 쓸 수 없게 됩니다.
    /// 접속이 끊긴 것은 이 메시지가 아니고, 끊긴 자리는 정해진 시간 동안 남습니다.
    Leave,
}

/// 플레이어가 한 Tick 동안 요청하는 조작입니다. 클라이언트가 서버에 보내는 게임 정보는 이 열한 필드가 전부입니다.
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
    /// 상호작용 키(F)를 누른 순간입니다. 누르고 있는 상태가 아니라 눌렀다는 사실 하나이고, 클라이언트는 키를 누른 입력에 한 번만 실어서 보냅니다.
    /// 무엇을 줍는지와 줍는 결과는 서버가 정하고, 바뀐 무기는 스냅샷으로 돌아옵니다.
    pub interact_pressed: bool,
    /// 건설 키(B)를 누르고 있습니다. 어느 타일에 세울지는 서버가 플레이어의 위치와 조준 방향에서 정하므로, 타일의 좌표는 보내지 않습니다.
    pub build: bool,
    /// 달리기 키(Shift)를 누르고 있습니다. 달릴 수 있는지(움직이는지, 게이지가 남았는지)는 서버가 정합니다.
    pub sprint: bool,
    /// 무기 전환 키(Q)를 누른 순간입니다. 누르고 있는 상태가 아니라 눌렀다는 사실 하나이고, 클라이언트는 키를 누른 입력에 한 번만 실어서 보냅니다.
    /// 어느 무기로 바꾸는지와 바꿀 수 있는지는 서버가 정하고, 바뀐 무기는 스냅샷으로 돌아옵니다.
    pub switch_weapon: bool,
    /// 준비 키(R)를 누른 순간입니다. 누르고 있는 상태가 아니라 눌렀다는 사실 하나이고, 클라이언트는 키를 누른 입력에 한 번만 실어서 보냅니다.
    /// 준비를 켜고 끄는 것은 야영지에서만 일어나고, 서버가 판정합니다.
    pub toggle_ready: bool,
    /// 확인 키(Enter)를 누른 순간입니다. 누르고 있는 상태가 아니라 눌렀다는 사실 하나이고, 클라이언트는 키를 누른 입력에 한 번만 실어서 보냅니다.
    /// 결과 화면에서만 일어나고, 서버가 판정합니다.
    pub confirm: bool,
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
    /// 상자에 담는 까닭은 `Snapshot`과 같습니다. 전송 형식은 상자에 담지 않은 값과 같습니다.
    Welcome(Box<Welcome>),
    /// 입장을 거절했다는 답입니다. 서버는 접속을 닫지 않고, 이유를 읽은 클라이언트가 닫기를 기다립니다.
    JoinRejected { reason: RejectReason },
    /// Tick마다 보내는 방 전체의 상태입니다. 상자에 담는 까닭은 스냅샷이 커서, 상자에 담지 않으면 이 열거형과 이것을 감싼 모든 열거형이
    /// 가장 큰 변형의 크기를 가지기 때문입니다. 전송 형식은 상자에 담지 않은 값과 같습니다.
    Snapshot(Box<Snapshot>),
    /// 방에 있는 사람의 이름입니다. 입장한 접속에 한 번 보내고, 그 뒤에는 누가 들어오거나 나갈 때만 방의 모든 접속에 보냅니다.
    /// 이름은 스냅샷에 넣지 않으므로 Tick마다 보내는 크기가 이름의 길이와 관계가 없습니다.
    Roster(WireRoster),
    /// 같은 표식으로 새 접속이 이 접속의 자리를 이어받았다는 알림입니다. 서버는 이 접속을 닫지 않고, 알림을 읽은 클라이언트가 닫기를 기다립니다.
    /// 브라우저는 닫힘을 이 메시지보다 먼저 알릴 수 있어서, 보내자마자 닫으면 이유가 화면에 나오지 않습니다.
    SeatTakenOver,
    /// 끝난 원정의 결과입니다. 결과 화면이 열린 Tick에 방의 모든 접속에 한 번 보내고, 결과 화면 중에 들어오거나 돌아온 접속에는 입장 안내와 명단 다음에 보냅니다.
    /// 요약과 기록은 스냅샷에 싣지 않으므로 Tick마다 보내는 크기가 결과의 크기와 관계가 없습니다.
    Debrief(WireReport),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    RoomFull {
        capacity: u32,
    },
    BadRoomCode,
    VersionMismatch {
        server: u32,
    },
    /// 이름이 비었거나, 12자보다 길거나, 쓸 수 없는 문자가 있습니다.
    BadName,
    /// 서버가 둘 수 있는 방의 수(`ServerLimits::max_rooms`)에 닿아서 새 방을 만들 수 없습니다. 있는 방에는 들어갈 수 있습니다.
    ServerFull,
}
