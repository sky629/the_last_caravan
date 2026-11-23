//! WebSocket으로 `ServerTransport`를 구현합니다. tokio와 tokio-tungstenite는 이 크레이트 안에서만 사용합니다.
//!
//! 스레드 두 개가 각자 소유한 것만 씁니다. 그래서 잠금이 없습니다.
//!
//! - 게임 스레드: `WsServer`를 소유하고 `poll`, `send`, `close`를 호출합니다. 접속마다 송신 대기열의 보내는 끝을 가집니다.
//! - 네트워크 스레드: tokio 실행기 하나가 소켓을 모두 소유합니다. 접속마다 작업(task) 하나가 읽기와 쓰기를 함께 처리합니다.
//!
//! 네트워크 스레드는 열린 접속, 받은 메시지, 닫힌 접속을 채널 하나로 게임 스레드에 알립니다.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::mpsc;
use std::time::Duration;

use caravan_protocol::{
    ClientMsg, LinkId, ServerEvent, ServerMsg, ServerTransport, decode_client, encode_server,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

/// 접속 하나에 쌓아 둘 수 있는 송신 메시지의 수입니다 (20 Hz에서 약 3초).
/// 대기열이 가득 차면 그 클라이언트가 따라오지 못하는 것으로 보고 접속을 끊습니다.
const OUTBOX_CAPACITY: usize = 64;
/// 클라이언트가 보내는 메시지는 작습니다. 이보다 큰 메시지는 접속을 닫습니다.
const MAX_INCOMING_BYTES: usize = 4096;
/// 소켓 쓰기가 이 시간 안에 끝나지 않으면 클라이언트가 읽지 않는 것으로 보고 접속을 닫습니다.
const WRITE_TIMEOUT: Duration = Duration::from_secs(3);

pub struct WsServer {
    inbound: mpsc::Receiver<Inbound>,
    outboxes: HashMap<LinkId, tokio::sync::mpsc::Sender<Vec<u8>>>,
    local_addr: SocketAddr,
}

/// 네트워크 스레드가 게임 스레드에 보내는 알림입니다.
enum Inbound {
    Opened {
        link: LinkId,
        outbox: tokio::sync::mpsc::Sender<Vec<u8>>,
    },
    Message(LinkId, ClientMsg),
    Closed(LinkId),
}

impl WsServer {
    /// 주소를 바로 점유하므로, 이미 사용 중인 주소는 여기서 오류가 됩니다.
    /// `"127.0.0.1:0"`을 넘기면 운영체제가 빈 포트를 고르고, `local_addr`가 그 포트를 알려 줍니다.
    pub fn bind(addr: &str) -> io::Result<WsServer> {
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
        let (inbound_tx, inbound) = mpsc::channel();
        std::thread::Builder::new()
            .name("caravan-ws".to_string())
            .spawn(move || runtime.block_on(accept_loop(listener, inbound_tx)))?;

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

async fn accept_loop(listener: TcpListener, inbound: mpsc::Sender<Inbound>) {
    let mut next_link = 1;
    loop {
        let Ok((stream, _peer)) = listener.accept().await else {
            continue;
        };
        let link = LinkId(next_link);
        next_link += 1;
        tokio::spawn(serve_link(stream, link, inbound.clone()));
    }
}

/// 접속 하나를 처리합니다. 받은 프레임을 해석해서 게임 스레드에 넘기고, 송신 대기열의 메시지를 소켓에 씁니다.
/// 게임 스레드가 보내는 끝을 버리면, 대기열에 남은 메시지를 모두 쓴 뒤 닫기 프레임을 보내고 끝납니다.
async fn serve_link(stream: TcpStream, link: LinkId, inbound: mpsc::Sender<Inbound>) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_INCOMING_BYTES))
        .max_frame_size(Some(MAX_INCOMING_BYTES));
    let Ok(mut ws) = tokio_tungstenite::accept_async_with_config(stream, Some(config)).await else {
        return;
    };
    let (outbox, mut outgoing) = tokio::sync::mpsc::channel::<Vec<u8>>(OUTBOX_CAPACITY);
    if inbound.send(Inbound::Opened { link, outbox }).is_err() {
        return;
    }

    loop {
        tokio::select! {
            frame = ws.next() => match frame {
                Some(Ok(Message::Binary(bytes))) => match decode_client(&bytes) {
                    Ok(msg) => {
                        if inbound.send(Inbound::Message(link, msg)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                },
                // tungstenite가 Ping에 직접 답합니다.
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                // 모든 메시지는 이진 프레임입니다. 문자열 프레임은 약속 위반입니다.
                Some(Ok(Message::Text(_) | Message::Close(_)) | Err(_)) | None => break,
            },
            bytes = outgoing.recv() => match bytes {
                Some(bytes) => {
                    let write = tokio::time::timeout(WRITE_TIMEOUT, ws.send(Message::binary(bytes)));
                    if !matches!(write.await, Ok(Ok(()))) {
                        break;
                    }
                }
                None => {
                    let _ = tokio::time::timeout(WRITE_TIMEOUT, ws.close(None)).await;
                    break;
                }
            },
        }
    }
    let _ = inbound.send(Inbound::Closed(link));
}
