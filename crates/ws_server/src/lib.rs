//! WebSocket으로 `ServerTransport`를 구현합니다. tokio와 tokio-tungstenite는 이 크레이트 안에서만 사용합니다.
//!
//! 스레드 두 개가 각자 소유한 것만 씁니다. 그래서 잠금이 없습니다.
//!
//! - 게임 스레드: `WsServer`를 소유하고 `poll`, `send`, `close`를 호출합니다. 접속마다 송신 대기열의 보내는 끝을 가집니다.
//! - 네트워크 스레드: tokio 실행기 하나가 소켓을 모두 소유합니다. 접속마다 작업(task) 하나가 읽기와 쓰기를 함께 처리합니다.
//!
//! 네트워크 스레드는 열린 접속, 받은 메시지, 닫힌 접속을 크기가 정해진 채널 하나로 게임 스레드에 알립니다.
//! 접속 수, 메시지 크기, 메시지 빈도, 대기열 크기의 한계는 모두 `ServerLimits`에서 읽습니다.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use caravan_protocol::{
    ClientMsg, LinkId, ServerEvent, ServerLimits, ServerMsg, ServerTransport, decode_client,
    encode_server,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

/// 접속을 받지 못했을 때(예: 열린 파일이 너무 많음) 다시 시도하기 전에 쉬는 시간입니다.
/// 쉬지 않으면 같은 오류가 반복되는 동안 네트워크 스레드가 CPU를 모두 씁니다.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
/// 메시지 빈도를 세는 구간의 길이입니다.
const RATE_WINDOW: Duration = Duration::from_secs(1);

pub struct WsServer {
    inbound: mpsc::Receiver<Inbound>,
    outboxes: HashMap<LinkId, mpsc::Sender<Vec<u8>>>,
    local_addr: SocketAddr,
}

/// 네트워크 스레드가 게임 스레드에 보내는 알림입니다.
enum Inbound {
    Opened {
        link: LinkId,
        outbox: mpsc::Sender<Vec<u8>>,
    },
    Message(LinkId, ClientMsg),
    Closed(LinkId),
}

impl WsServer {
    /// 주소를 바로 점유하므로, 이미 사용 중인 주소는 여기서 오류가 됩니다.
    /// `"127.0.0.1:0"`을 넘기면 운영체제가 빈 포트를 고르고, `local_addr`가 그 포트를 알려 줍니다.
    pub fn bind(addr: &str, limits: ServerLimits) -> io::Result<WsServer> {
        let std_listener = std::net::TcpListener::bind(addr)?;
        std_listener.set_nonblocking(true)?;
        let local_addr = std_listener.local_addr()?;

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let listener = {
            let _inside_runtime = runtime.enter();
            TcpListener::from_std(std_listener)?
        };
        let (inbound_tx, inbound) = mpsc::channel(limits.inbound_capacity);
        std::thread::Builder::new()
            .name("caravan-ws".to_string())
            .spawn(move || runtime.block_on(accept_loop(listener, inbound_tx, limits)))?;

        Ok(WsServer {
            inbound,
            outboxes: HashMap::new(),
            local_addr,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl ServerTransport for WsServer {
    fn poll(&mut self) -> Option<ServerEvent> {
        match self.inbound.try_recv().ok()? {
            Inbound::Opened { link, outbox } => {
                self.outboxes.insert(link, outbox);
                Some(ServerEvent::Connected(link))
            }
            Inbound::Message(link, msg) => Some(ServerEvent::Message(link, msg)),
            Inbound::Closed(link) => {
                self.outboxes.remove(&link);
                Some(ServerEvent::Disconnected(link))
            }
        }
    }

    fn send(&mut self, link: LinkId, msg: &ServerMsg) {
        let Some(outbox) = self.outboxes.get(&link) else {
            return;
        };
        // 대기열이 가득 찼거나 작업이 끝났습니다. 보내는 끝을 버리면 작업이 남은 메시지를 보낸 뒤 접속을 닫고,
        // 게임 스레드는 곧 `Disconnected`를 받습니다.
        if outbox.try_send(encode_server(msg)).is_err() {
            self.outboxes.remove(&link);
        }
    }

    fn close(&mut self, link: LinkId) {
        self.outboxes.remove(&link);
    }
}

async fn accept_loop(listener: TcpListener, inbound: mpsc::Sender<Inbound>, limits: ServerLimits) {
    let mut seats = Seats::new(limits);
    let (release, mut released) = mpsc::unbounded_channel();
    let mut next_link = 1;
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(_) => {
                tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
                continue;
            }
        };
        while let Ok(ip) = released.try_recv() {
            seats.give_back(ip);
        }
        // 자리가 없으면 `stream`을 버려서 접속을 바로 닫습니다.
        if !seats.take(peer.ip()) {
            continue;
        }
        let seat = Seat {
            ip: peer.ip(),
            release: release.clone(),
        };
        let link = LinkId(next_link);
        next_link += 1;
        tokio::spawn(serve_link(stream, link, inbound.clone(), limits, seat));
    }
}

/// 열려 있는 접속(연결 절차 중인 접속 포함)의 수를 전체와 IP 주소별로 셉니다. accept 반복만 이 값을 소유합니다.
struct Seats {
    total: usize,
    per_ip: HashMap<IpAddr, usize>,
    max_links: usize,
    max_links_per_ip: usize,
}

impl Seats {
    fn new(limits: ServerLimits) -> Seats {
        Seats {
            total: 0,
            per_ip: HashMap::new(),
            max_links: limits.max_links,
            max_links_per_ip: limits.max_links_per_ip,
        }
    }

    /// 전체 자리와 그 IP 주소의 자리가 모두 남아 있으면 하나씩 차지하고 `true`를 돌려줍니다.
    fn take(&mut self, ip: IpAddr) -> bool {
        let used_by_ip = self.per_ip.get(&ip).copied().unwrap_or(0);
        if self.total >= self.max_links || used_by_ip >= self.max_links_per_ip {
            return false;
        }
        self.total += 1;
        self.per_ip.insert(ip, used_by_ip + 1);
        true
    }

    fn give_back(&mut self, ip: IpAddr) {
        self.total = self.total.saturating_sub(1);
        if let Some(count) = self.per_ip.get_mut(&ip) {
            *count -= 1;
            if *count == 0 {
                self.per_ip.remove(&ip);
            }
        }
    }
}

/// 접속 하나가 차지한 자리입니다. 접속 작업이 어느 경로로 끝나든(연결 절차의 실패나 시간 초과 포함)
/// 이 값이 버려지면서 accept 반복에 자리를 돌려줍니다.
struct Seat {
    ip: IpAddr,
    release: mpsc::UnboundedSender<IpAddr>,
}

impl Drop for Seat {
    fn drop(&mut self) {
        let _ = self.release.send(self.ip);
    }
}

/// 접속 하나를 처리합니다. 받은 프레임을 해석해서 게임 스레드에 넘기고, 송신 대기열의 메시지를 소켓에 씁니다.
/// 게임 스레드가 보내는 끝을 버리면, 대기열에 남은 메시지를 모두 쓴 뒤 닫기 프레임을 보내고 끝납니다.
async fn serve_link(
    stream: TcpStream,
    link: LinkId,
    inbound: mpsc::Sender<Inbound>,
    limits: ServerLimits,
    seat: Seat,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(limits.max_message_bytes))
        .max_frame_size(Some(limits.max_message_bytes));
    let handshake = tokio_tungstenite::accept_async_with_config(stream, Some(config));
    let Ok(Ok(mut ws)) = tokio::time::timeout(limits.handshake_timeout, handshake).await else {
        return;
    };
    let (outbox, mut outgoing) = mpsc::channel::<Vec<u8>>(limits.outbox_capacity);
    if inbound
        .send(Inbound::Opened { link, outbox })
        .await
        .is_err()
    {
        return;
    }

    let mut rate = RateWindow::new();
    loop {
        tokio::select! {
            frame = ws.next() => {
                if !rate.allow(Instant::now(), limits.max_messages_per_sec) {
                    break;
                }
                match frame {
                    Some(Ok(Message::Binary(bytes))) => match decode_client(&bytes) {
                        // 게임 스레드의 처리가 밀리면 여기서 기다립니다. 그동안 이 소켓을 읽지 않습니다.
                        Ok(msg) => {
                            if inbound.send(Inbound::Message(link, msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                    // tungstenite가 Ping에 직접 답합니다.
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                    // 모든 메시지는 이진 프레임입니다. 문자열 프레임은 약속 위반입니다.
                    Some(Ok(Message::Text(_) | Message::Close(_)) | Err(_)) | None => break,
                }
            }
            bytes = outgoing.recv() => match bytes {
                Some(bytes) => {
                    let write = tokio::time::timeout(limits.write_timeout, ws.send(Message::binary(bytes)));
                    if !matches!(write.await, Ok(Ok(()))) {
                        break;
                    }
                }
                None => {
                    let _ = tokio::time::timeout(limits.write_timeout, ws.close(None)).await;
                    break;
                }
            },
        }
    }
    // 자리를 먼저 돌려준 뒤 닫힘을 알립니다. 게임 스레드가 `Disconnected`를 받았을 때 그 자리는 이미 비어 있습니다.
    drop(seat);
    let _ = inbound.send(Inbound::Closed(link)).await;
}

/// 접속 하나가 지금의 1초 구간에 보낸 프레임의 수를 셉니다.
struct RateWindow {
    started: Instant,
    count: u32,
}

impl RateWindow {
    fn new() -> RateWindow {
        RateWindow {
            started: Instant::now(),
            count: 0,
        }
    }

    /// 프레임 하나를 세고, 이번 구간의 한도 안이면 `true`를 돌려줍니다.
    fn allow(&mut self, now: Instant, max_per_window: u32) -> bool {
        if now.duration_since(self.started) >= RATE_WINDOW {
            self.started = now;
            self.count = 0;
        }
        self.count += 1;
        self.count <= max_per_window
    }
}
