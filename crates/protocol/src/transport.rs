//! 접속 방식 인터페이스 두 개입니다. 둘 다 호출한 쪽을 기다리게 하지 않는 폴링(polling) 방식이고, 타입이 있는 메시지를 다룹니다.
//!
//! 폴링 방식을 고른 이유: 서버의 Tick 반복과 Bevy의 프레임이 네트워크 작업을 언제 할지 정합니다.
//! 다른 스레드에서 게임 상태를 바꾸는 콜백이 없으므로 잠금이 필요하지 않습니다.
//! WebSocket 외의 접속 방식(Steam, 프로세스 내부)도 이 트레이트를 구현하면 서버와 클라이언트를 고치지 않고 연결됩니다.

use crate::messages::{ClientMsg, ServerMsg};

/// 서버 쪽에서 접속 하나를 가리키는 번호입니다. 접속 방식이 정하며, 다른 코드에서는 의미가 없는 값입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkId(pub u64);

#[derive(Debug)]
pub enum ServerEvent {
    Connected(LinkId),
    /// 이미 해석한 메시지입니다. 해석할 수 없는 데이터를 보낸 접속은 접속 방식이 닫습니다.
    /// 형식이 바뀐 빌드의 입장 요청은 해석할 수 없지만 닫지 않습니다. 접속 방식이 `VersionMismatch`로 답하고, 게임 서버는 이 접속이 입장하지 않은 채 기한을 넘기면 닫습니다.
    Message(LinkId, ClientMsg),
    /// 접속 방식이 한도를 넘은 접속을 직접 닫았습니다. 값은 운영자가 로그로 읽는 이유 문장입니다.
    /// 이 접속의 `Disconnected`가 바로 이어서 옵니다.
    LimitReached(LinkId, String),
    Disconnected(LinkId),
}

/// 게임 서버가 사용하는 접속 방식입니다. 모든 구현은 다음 약속을 지킵니다.
///
/// 1. `poll`은 기다리지 않고, 이벤트를 도착한 순서대로 하나씩 돌려줍니다. 이벤트가 없으면 `None`입니다.
/// 2. 접속 하나에 대해 `Connected`가 먼저 오고, `Message`가 0개 이상 온 뒤, `Disconnected`가 마지막에 한 번 옵니다.
///    서버가 `close`를 호출한 접속에도 `Disconnected`가 옵니다. 접속 방식이 한도를 넘었다는 이유로 닫은 접속에는, 그 접속의 `Message`가 모두 온 뒤 `Disconnected` 바로 앞에 `LimitReached`가 한 번 옵니다.
/// 3. `send`는 기다리지 않습니다. 모르는 접속이나 닫힌 접속에 보내면 아무 일도 하지 않습니다.
///    보낼 메시지를 따라가지 못하는 접속은 접속 방식이 끊고, 서버는 `Disconnected`를 받습니다.
/// 4. `close`는 이미 보낸 메시지를 먼저 전달한 뒤 접속을 닫습니다. 그래서 거절 사유가 클라이언트에 도착합니다.
pub trait ServerTransport {
    fn poll(&mut self) -> Option<ServerEvent>;
    fn send(&mut self, link: LinkId, msg: &ServerMsg);
    fn close(&mut self, link: LinkId);
}

#[derive(Debug)]
pub enum ClientEvent {
    /// 접속이 열렸습니다. 이 이벤트를 받은 뒤부터 메시지를 보낼 수 있습니다.
    Connected,
    Message(ServerMsg),
    /// 마지막 이벤트입니다. `reason`은 화면에 표시하는 문장입니다.
    Disconnected {
        reason: String,
    },
}

/// 클라이언트가 사용하는 접속 방식입니다.
///
/// `poll`은 기다리지 않습니다. `Connected`가 먼저 오고 `Disconnected`가 마지막에 한 번 옵니다.
/// `send`는 기다리지 않고, `Disconnected` 이후에는 아무 일도 하지 않습니다.
/// 브라우저의 WebSocket은 다른 스레드로 옮길 수 없으므로 이 트레이트는 `Send`를 요구하지 않습니다.
pub trait ClientTransport {
    fn poll(&mut self) -> Option<ClientEvent>;
    fn send(&mut self, msg: &ClientMsg);
}
