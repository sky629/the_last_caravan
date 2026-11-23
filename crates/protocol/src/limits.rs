//! 서버가 접속 하나와 메시지 하나에 허용하는 한계입니다.
//!
//! 게임 서버(`caravan_server`)와 서버 쪽 접속 방식(`caravan_ws_server`)이 함께 읽으므로, 둘이 공통으로 의존하는 이 크레이트에 둡니다.
//! 정상적인 클라이언트가 지켜야 하는 약속이기도 합니다. 기본값은 플레이어 네 명이 초당 20번 입력을 보내도 닿지 않을 만큼 넉넉하게 정했습니다.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerLimits {
    /// WebSocket 연결 절차(handshake)를 끝내야 하는 시간입니다.
    /// 연결 절차를 끝내지 않는 상대가 접속 자리를 계속 차지하지 못하게 합니다.
    pub handshake_timeout: Duration,
    /// 동시에 열어 둘 수 있는 접속(연결 절차 중인 접속 포함)의 수입니다.
    /// 접속을 많이 여는 상대가 메모리와 파일 핸들을 모두 쓰지 못하게 합니다. 넘치는 접속은 받자마자 닫습니다.
    pub max_links: usize,
    /// IP 주소 하나가 동시에 열어 둘 수 있는 접속의 수입니다.
    /// 상대 하나가 `max_links`의 자리를 모두 차지하지 못하게 합니다. 한 집이나 한 장소에서 여러 명이 같은 IP 주소로 접속해도 닿지 않게 넉넉하게 정했습니다.
    /// 리버스 프록시 뒤에 서버를 두면 모든 접속이 프록시의 IP 주소 하나로 보이므로, 이 값을 올리거나 프록시가 전달한 주소로 세도록 바꿔야 합니다.
    pub max_links_per_ip: usize,
    /// 네트워크 스레드가 게임 스레드에 넘기고 아직 처리되지 않은 이벤트의 최대 수입니다 (모든 접속 합계).
    /// 가득 차면 읽기 작업이 기다리고, 그동안 소켓을 읽지 않으므로 보내는 쪽이 느려집니다. 메모리가 끝없이 늘지 않습니다.
    pub inbound_capacity: usize,
    /// 접속 하나가 1초에 보낼 수 있는 메시지의 수입니다. 넘으면 접속을 닫습니다.
    /// 정상적인 클라이언트는 Tick 속도(20 Hz)로 입력을 보내므로, 브라우저가 지연된 입력을 몰아서 보내도 닿지 않게 여유를 둡니다.
    pub max_messages_per_sec: u32,
    /// 클라이언트가 보내는 메시지 하나의 최대 크기(바이트)입니다. 클라이언트의 메시지는 작으므로, 큰 메시지로 메모리를 쓰게 하지 못하게 합니다.
    pub max_message_bytes: usize,
    /// 접속 하나에 쌓아 둘 수 있는 송신 메시지의 수입니다 (20 Hz에서 약 3초).
    /// 가득 차면 그 클라이언트가 따라오지 못하는 것으로 보고 접속을 끊습니다. 느린 클라이언트 하나 때문에 서버의 메모리가 늘지 않습니다.
    pub outbox_capacity: usize,
    /// 소켓 쓰기 하나가 끝나야 하는 시간입니다. 클라이언트가 읽기를 멈추면 쓰기가 끝나지 않으므로, 이 시간이 지나면 접속을 닫습니다.
    pub write_timeout: Duration,
    /// `GameServer::pump` 한 번이 처리하는 접속 이벤트의 최대 수입니다.
    /// 이벤트가 몰려도 남은 이벤트는 다음 `pump`로 넘기고 때가 된 Tick은 실행하므로, 게임이 멈추지 않습니다.
    pub events_per_pump: usize,
    /// 접속한 뒤 `Join`을 보내야 하는 시간입니다. 입장하지 않고 접속 자리만 차지하는 상대를 닫습니다.
    pub join_deadline: Duration,
    /// 입장한 접속이 아무 메시지도 보내지 않고 지날 수 있는 시간입니다. 넘으면 접속을 닫고 플레이어를 방에서 뺍니다.
    /// 클라이언트는 Tick마다 입력을 보내므로 살아 있는 클라이언트는 닿지 않습니다. 소켓은 열려 있지만 멈춘 클라이언트가 자리를 차지하지 못하게 합니다.
    pub idle_timeout: Duration,
}

impl Default for ServerLimits {
    fn default() -> ServerLimits {
        ServerLimits {
            handshake_timeout: Duration::from_secs(5),
            max_links: 64,
            max_links_per_ip: 16,
            inbound_capacity: 1024,
            max_messages_per_sec: 100,
            max_message_bytes: 4096,
            outbox_capacity: 64,
            write_timeout: Duration::from_secs(3),
            events_per_pump: 512,
            join_deadline: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(30),
        }
    }
}
