//! 클라이언트와 서버가 합의해야 하는 모든 것입니다.
//!
//! - `messages`, `snapshot`, `welcome`: 주고받는 메시지의 정의. 게임 상태를 전송 형식으로 바꾸는 함수는 `Snapshot::from_sim` 하나입니다.
//!   `Welcome::from_sim`은 접속 중에 바뀌지 않는 설정을 입장할 때 한 번 바꿉니다.
//! - `player_name`, `room_code`: 플레이어가 입력하는 이름과 방 코드의 검사. 서버와 클라이언트가 같은 함수를 씁니다.
//! - `roster`: 방에 있는 사람의 이름입니다. 명단이 바뀔 때만 보냅니다.
//! - `units`: 위치와 방향을 정수로 줄이는 변환과 그 오차. 정수와 실수 사이의 변환은 이 파일에만 있습니다.
//! - `codec`: 메시지를 바이트로 바꾸는 함수와 되돌리는 함수. 전송 형식을 아는 파일은 이것 하나입니다.
//! - `transport`: 접속 방식 인터페이스 두 개. 서버와 클라이언트는 이 트레이트만 압니다.
//! - `loopback`: 같은 프로세스 안에서 메시지를 전달하는 접속 방식. 서버 테스트가 사용합니다.
//! - `limits`: 서버가 접속 하나와 메시지 하나에 허용하는 한계. 게임 서버와 서버 쪽 접속 방식이 함께 읽습니다.
//! - `welcome_check`: `Welcome`이 믿고 쓸 만한 값인지 확인하는 상한. 클라이언트는 받은 `Welcome`에, 서버는 시작할 때 보낼 `Welcome`에 같은 확인을 합니다.
//!
//! 읽는 순서: `messages`, `snapshot`, `welcome`, `units`, `transport`, `codec`, `loopback`.

mod codec;
mod limits;
mod loopback;
mod messages;
mod player_name;
mod room_code;
mod roster;
mod snapshot;
mod transport;
mod units;
mod welcome;
mod welcome_check;

pub use codec::{DecodeError, decode_client, decode_server, encode_client, encode_server};
pub use limits::ServerLimits;
pub use loopback::{LoopbackClient, LoopbackConnector, LoopbackServer, loopback};
pub use messages::{ClientMsg, InputMsg, PROTOCOL_VERSION, RejectReason, ServerMsg, WireVec2};
pub use player_name::{NameError, PlayerName};
pub use room_code::{RoomCode, RoomCodeError};
pub use roster::{WireMember, WireRoster};
pub use snapshot::{
    Snapshot, WireAttack, WireBuildRefusal, WireCannotBuild, WireChest, WireGroundWeapon,
    WireInteraction, WireLife, WireLoadout, WireMonster, WireMonsterKind, WireNightStep, WireNode,
    WireOutcome, WirePhase, WirePile, WirePlayer, WireProjectile, WirePrompt, WireRepairTarget,
    WireShot, WireStruck, WireTower, WireTowerState, WireWagon, WireWeaponKind,
};
pub use transport::{ClientEvent, ClientTransport, LinkId, ServerEvent, ServerTransport};
pub use units::{WireAngle, WirePos, WireRect, WireTile};
pub use welcome::{
    MonsterKindInfo, NodeInfo, RackInfo, WeaponInfo, Welcome, WireDelivery, WorldInfo,
};
pub use welcome_check::check_welcome;
