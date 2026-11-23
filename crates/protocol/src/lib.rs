//! 클라이언트와 서버가 합의해야 하는 모든 것입니다.
//!
//! - `messages`, `snapshot`: 주고받는 메시지의 정의. 규칙의 상태를 전송 형식으로 바꾸는 함수는 `Snapshot::from_sim` 하나입니다.
//! - `codec`: 메시지를 바이트로 바꾸는 함수와 되돌리는 함수. 전송 형식을 아는 파일은 이것 하나입니다.
//! - `transport`: 접속 방식 인터페이스 두 개. 서버와 클라이언트는 이 트레이트만 압니다.
//! - `loopback`: 같은 프로세스 안에서 메시지를 전달하는 접속 방식. 서버 테스트가 사용합니다.
//! - `limits`: 서버가 접속 하나와 메시지 하나에 허용하는 한계. 게임 서버와 서버 쪽 접속 방식이 함께 읽습니다.
//!
//! 읽는 순서: `messages`, `snapshot`, `transport`, `codec`, `loopback`.

mod codec;
mod limits;
mod loopback;
mod messages;
mod room_code;
mod snapshot;
mod transport;

pub use codec::{DecodeError, decode_client, decode_server, encode_client, encode_server};
pub use limits::ServerLimits;
pub use loopback::{LoopbackClient, LoopbackConnector, LoopbackServer, loopback};
pub use messages::{ClientMsg, PROTOCOL_VERSION, RejectReason, ServerMsg};
pub use room_code::{RoomCode, RoomCodeError};
pub use snapshot::{Snapshot, WirePos};
pub use transport::{ClientEvent, ClientTransport, LinkId, ServerEvent, ServerTransport};
