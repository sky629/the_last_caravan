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
    /// 같은 열쇠(IPv6는 같은 /64 대역을 하나로 본 주소)로 끊긴 자리의 수(`max_held_seats_per_ip`)도 셉니다.
    pub max_links_per_ip: usize,
    /// 웹 클라이언트의 파일을 동시에 보낼 수 있는 응답의 수입니다. 파일을 내주는 서버(`caravan_ws_server`의 `bind_with_files`)만 읽습니다.
    /// 파일 요청은 요청의 머리를 읽는 동안만 접속의 자리(`max_links`, `max_links_per_ip`)를 차지하고, 파일을 보내는 동안에는 이 자리를 차지합니다.
    /// 그래서 느리게 받는 다운로드가 많아도 게임 접속의 자리는 줄지 않습니다. 이 자리가 모두 차 있으면 새 파일 요청에 `503`으로 답하고 닫습니다.
    pub max_file_responses: usize,
    /// 파일 응답 하나가 끝나야 하는 시간은 이 값에 「본문의 크기 ÷ `file_min_bytes_per_sec`」를 더한 것입니다.
    /// 조각마다의 쓰기 시간 제한(`write_timeout`)만으로는 조금씩 계속 읽는 상대가 파일 응답의 자리를 오래 차지할 수 있으므로, 응답 전체의 시간을 따로 묶습니다.
    pub file_response_grace: Duration,
    /// 파일을 받는 쪽에 요구하는 가장 느린 평균 속도(초당 바이트)입니다. 본문을 이 속도로 받는 시간에 `file_response_grace`를 더한 시간 안에 끝나지 않는 다운로드는 닫힙니다.
    pub file_min_bytes_per_sec: usize,
    /// 접속이 끊겨서 자리를 유지하는 동안, 같은 상대(`max_links_per_ip`와 같은 열쇠)가 유지시킬 수 있는 자리의 수입니다.
    /// 끊긴 자리는 접속 수의 한도에 들지 않으므로, 입장과 끊기를 되풀이해서 접속자가 없는 방을 쌓는 상대를 이 값으로 묶습니다.
    /// 한 집에서 방 하나의 사람들이 함께 끊기는 경우를 받도록 방의 정원 이상이어야 하고, 서버가 시작할 때 확인합니다.
    /// 한 상대에게서 이 수보다 많은 자리가 동시에 끊기면 넘친 자리는 유지하지 않고 곧바로 내놓습니다.
    /// 리버스 프록시 뒤에서는 모든 접속이 주소 하나로 보이므로 `max_links_per_ip`처럼 이 값도 올려야 합니다.
    pub max_held_seats_per_ip: usize,
    /// 서버 전체에서 한꺼번에 유지할 수 있는 끊긴 자리의 수입니다. 여러 주소를 쓰는 상대가 주소마다의 한도(`max_held_seats_per_ip`)를 하나씩 채워도,
    /// 접속자가 없는 방이 이 수를 넘어 쌓이지 않습니다. 이 한도에 닿아 있는 동안 새로 끊기는 자리는 유지하지 않고 곧바로 내놓고, 이미 유지 중인 자리는 그대로 둡니다.
    /// 서버는 시작할 때 이 값이 `max_held_seats_per_ip` 이상이고 `max_rooms`보다 작은지 확인합니다. 그래서 끊긴 자리만으로는 방의 한도를 채우지 못하고, 새 방을 만드는 입장은 막히지 않습니다.
    pub max_held_seats: usize,
    /// 서버가 한꺼번에 둘 수 있는 방의 수입니다. 접속이 열려 있는 방의 수는 `max_links`가, 접속자가 없는 방의 수는 `max_held_seats`가 묶으므로,
    /// 기본 수치(64 + 128 < 256)에서는 정상적인 사용으로 닿지 않는 값입니다. 그래도 서버의 메모리를 끝까지 지키는 마지막 한도입니다.
    /// 넘으면 새 방을 만드는 입장을 거절하고(`RejectReason::ServerFull`), 있는 방에 들어가는 입장은 받습니다.
    /// 서버는 시작할 때 이 값이 한 상대가 차지할 수 있는 방의 수(`max_links_per_ip + max_held_seats_per_ip`)와 `max_held_seats`보다 큰지 확인합니다.
    pub max_rooms: usize,
    /// 네트워크 스레드가 게임 스레드에 넘기고 아직 처리되지 않은 이벤트의 최대 수입니다 (모든 접속 합계).
    /// 가득 차면 읽기 작업이 기다리고, 그동안 소켓을 읽지 않으므로 보내는 쪽이 느려집니다. 메모리가 끝없이 늘지 않습니다.
    pub inbound_capacity: usize,
    /// 접속 하나가 1초에 보낼 수 있는 메시지의 수입니다. 넘으면 접속을 닫습니다.
    /// 기본값은 Tick 속도(20 Hz)로 입력을 보내는 클라이언트가 몇 초 멈췄다가 밀린 입력을 몰아서 보내도 닿지 않는 값입니다.
    /// 서버는 시작할 때 이 값이 Tick 속도에 비해 충분한지 확인하며, 얼마나 긴 정체를 견디는지는 `caravan_server`의 `LINK_RATE_HEADROOM`에 적혀 있습니다.
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
    /// 입장한 접속이 아무 메시지도 보내지 않고 지날 수 있는 시간입니다. 넘으면 접속을 닫습니다. 플레이어는 방에서 빼지 않고 자리를 유지합니다.
    /// 클라이언트는 Tick마다 입력을 보내므로 살아 있는 클라이언트는 닿지 않습니다. 소켓은 열려 있지만 멈춘 클라이언트가 접속 자리를 차지하지 못하게 합니다.
    /// 자리를 유지하는 시간(`seat_hold`)이 마지막 메시지부터 세므로, 이 값은 `seat_hold` 이하여야 합니다. 서버가 시작할 때 확인합니다.
    pub idle_timeout: Duration,
    /// 자리의 입력이 이 시간 동안 오지 않으면, 접속이 열려 있어도 자리를 비운 것으로 봅니다. 캐릭터는 그때부터 보호를 받습니다.
    /// 같은 접속에서 입력이 다시 오면 곧바로 자리에 있는 것으로 돌아갑니다. 서버는 시작할 때 `seat_hold`보다 짧은지 확인합니다.
    pub away_after: Duration,
    /// 야영지에서 이 시간 동안 조작(이동, 조준, 버튼의 변화)이 없는 사람은 「모두 준비」의 대상에서 빠지고 자리를 비운 것으로 보입니다.
    /// 자리는 내놓지 않습니다. 접속이 열려 있고 입력이 계속 오기 때문입니다. 조작이 다시 오면 준비하지 않은 상태로 곧바로 돌아옵니다.
    /// 클라이언트는 가만히 있어도 Tick마다 입력을 보내므로, 입력이 오는지가 아니라 입력의 내용이 바뀌는지를 봅니다.
    pub camp_idle: Duration,
    /// 자리의 마지막 메시지부터 이 시간이 지나면 자리를 내놓습니다. 처리는 스스로 나간 것과 같습니다(든 자원과 상자에서 얻은 무기가 떨어집니다).
    /// 접속이 끊긴 자리와 입력이 멈춘 자리에 같습니다. 방이 멈춘 동안에도 셉니다.
    pub seat_hold: Duration,
    /// 입력 메시지가 이 Tick 수만큼 오지 않으면 그 플레이어를 아무것도 누르지 않은 상태로 움직입니다.
    /// 그 전까지는 마지막 입력을 다시 씁니다. 입력이 한두 Tick 늦어도 캐릭터는 계속 움직이고, 백그라운드에서 실행이 제한되어 입력이 끊긴 브라우저 탭의 캐릭터는 멈춥니다.
    /// 소켓이 열려 있는 한 접속은 `idle_timeout`까지 유지되므로, 이 값은 접속이 아니라 캐릭터의 움직임만 다룹니다.
    pub input_stale_ticks: u32,
}

impl Default for ServerLimits {
    fn default() -> ServerLimits {
        ServerLimits {
            handshake_timeout: Duration::from_secs(5),
            max_links: 64,
            max_links_per_ip: 16,
            max_file_responses: 16,
            file_response_grace: Duration::from_secs(10),
            file_min_bytes_per_sec: 128 * 1024,
            max_held_seats_per_ip: 8,
            max_held_seats: 128,
            max_rooms: 256,
            inbound_capacity: 1024,
            max_messages_per_sec: 100,
            max_message_bytes: 4096,
            outbox_capacity: 64,
            write_timeout: Duration::from_secs(3),
            events_per_pump: 512,
            join_deadline: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(30),
            away_after: Duration::from_secs(3),
            camp_idle: Duration::from_secs(60),
            seat_hold: Duration::from_secs(60),
            input_stale_ticks: 10,
        }
    }
}
