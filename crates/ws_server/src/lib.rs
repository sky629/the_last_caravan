//! WebSocket으로 `ServerTransport`를 구현합니다. tokio와 tokio-tungstenite는 이 크레이트 안에서만 사용합니다.
//!
//! 스레드 두 개가 각자 소유한 것만 씁니다. 그래서 잠금이 없습니다.
//!
//! - 게임 스레드: `WsServer`를 소유하고 `poll`, `send`, `close`를 호출합니다. 접속마다 송신 대기열의 보내는 끝을 가집니다.
//! - 네트워크 스레드: tokio 실행기 하나가 소켓을 모두 소유합니다. 접속마다 작업(task) 하나가 읽기와 쓰기를 함께 처리합니다.
//!
//! 네트워크 스레드는 열린 접속, 받은 메시지, 한도를 넘어 닫은 이유, 닫힌 접속을 크기가 정해진 채널 하나로 게임 스레드에 알립니다.
//! 접속 수, 메시지 크기, 메시지 빈도, 대기열 크기의 한계는 모두 `ServerLimits`에서 읽습니다.
//!
//! 해석할 수 없는 메시지를 보낸 접속은 닫습니다. 다만 다른 `version`의 입장 요청(`DecodeError::foreign_join_version`)은 게임 스레드를 거치지 않고
//! 여기서 `VersionMismatch`로 답합니다. 형식이 바뀐 이전 클라이언트가 이유 없이 끊기지 않게 하려는 것입니다.
//!
//! `bind_with_files`로 만든 서버는 같은 포트로 웹 클라이언트의 파일도 내줍니다(`files`). 접속마다 요청의 머리를 먼저 읽고,
//! `Upgrade: websocket` 머리가 있으면 읽은 바이트를 그대로 tungstenite에 다시 읽혀서 게임 접속으로 받고, 없으면 파일 요청으로 답한 뒤 닫습니다.
//! 파일 요청은 머리를 읽는 동안만 접속의 자리를 차지하고, 파일을 보내는 동안에는 파일 응답의 자리(`max_file_responses`)를 차지합니다.
//! `bind`로 만든 서버는 머리를 미리 읽지 않고 모든 접속을 WebSocket으로 받습니다.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Cursor};
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use caravan_protocol::{
    ClientMsg, LinkId, PROTOCOL_VERSION, PeerKey, RejectReason, ServerEvent, ServerLimits,
    ServerMsg, ServerTransport, decode_client, encode_server,
};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, Chain, Join};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error, Message};

mod files;

use files::Answer;
pub use files::{PublicUrl, PublicUrlError, WebFiles};

/// 접속을 받지 못했을 때(예: 열린 파일이 너무 많음) 다시 시도하기 전에 쉬는 시간입니다.
/// 쉬지 않으면 같은 오류가 반복되는 동안 네트워크 스레드가 CPU를 모두 씁니다.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
/// 메시지 빈도를 세는 구간의 길이입니다.
const RATE_WINDOW: Duration = Duration::from_secs(1);
/// 요청의 머리(요청 줄과 머리 줄)의 최대 크기와 머리 줄의 최대 수입니다. 넘으면 답하지 않고 닫습니다.
const MAX_HEAD_BYTES: usize = 16 * 1024;
const MAX_HEAD_LINES: usize = 64;
/// 파일의 본문을 이 크기로 나누어 씁니다. 조각마다 `write_timeout` 안에 써져야 합니다.
const FILE_CHUNK: usize = 64 * 1024;

pub struct WsServer {
    inbound: mpsc::Receiver<Inbound>,
    outboxes: HashMap<LinkId, mpsc::Sender<Vec<u8>>>,
    /// 송신 대기열이 가득 차서 게임 스레드가 보내는 끝을 버린 접속과 그 이유입니다.
    /// 그 접속의 `Closed`가 오면 `LimitReached`로 바꿔서 알립니다. 그때까지 도착한 `Message`가 먼저 나가야 하기 때문입니다.
    full_outboxes: HashMap<LinkId, String>,
    /// 방금 돌려준 `LimitReached` 바로 뒤에 돌려줄 `Disconnected`입니다.
    pending: VecDeque<ServerEvent>,
    local_addr: SocketAddr,
    /// 네트워크 스레드에 끝내라고 알리는 신호입니다. `Drop`이 보냅니다.
    shutdown: Option<oneshot::Sender<()>>,
    /// `Drop`이 이 스레드가 끝나기를 기다립니다. 그래서 `WsServer`가 사라지면 포트도 이미 풀려 있습니다.
    network_thread: Option<JoinHandle<()>>,
}

/// 네트워크 스레드가 게임 스레드에 보내는 알림입니다.
enum Inbound {
    Opened {
        link: LinkId,
        /// 접속의 상대를 구분하는 열쇠입니다. 접속 수를 세는 열쇠와 같습니다.
        peer: PeerKey,
        outbox: mpsc::Sender<Vec<u8>>,
    },
    Message(LinkId, ClientMsg),
    /// 접속 작업이 한도를 넘은 접속을 닫습니다. 바로 뒤에 `Closed`가 옵니다.
    LimitReached(LinkId, String),
    Closed(LinkId),
}

impl WsServer {
    /// 주소를 바로 점유하므로, 이미 사용 중인 주소는 여기서 오류가 됩니다.
    /// `"127.0.0.1:0"`을 넘기면 운영체제가 빈 포트를 고르고, `local_addr`가 그 포트를 알려 줍니다.
    pub fn bind(addr: &str, limits: ServerLimits) -> io::Result<WsServer> {
        WsServer::start(addr, limits, None)
    }

    /// `bind`와 같고, 같은 포트로 오는 HTTP 요청(`Upgrade: websocket` 머리가 없는 요청)에 `files`를 내줍니다.
    pub fn bind_with_files(
        addr: &str,
        limits: ServerLimits,
        files: WebFiles,
    ) -> io::Result<WsServer> {
        let web = Web {
            files,
            slots: Semaphore::new(limits.max_file_responses),
        };
        WsServer::start(addr, limits, Some(Arc::new(web)))
    }

    fn start(addr: &str, limits: ServerLimits, web: Option<Arc<Web>>) -> io::Result<WsServer> {
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
        let (shutdown, shutdown_signal) = oneshot::channel();
        // 실행기는 이 스레드가 끝날 때 함께 버려지고, 그때 접속 작업도 모두 끝납니다.
        let network_thread = std::thread::Builder::new()
            .name("caravan-ws".to_string())
            .spawn(move || {
                runtime.block_on(accept_loop(
                    listener,
                    inbound_tx,
                    limits,
                    web,
                    shutdown_signal,
                ));
            })?;

        Ok(WsServer {
            inbound,
            outboxes: HashMap::new(),
            full_outboxes: HashMap::new(),
            pending: VecDeque::new(),
            local_addr,
            shutdown: Some(shutdown),
            network_thread: Some(network_thread),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl Drop for WsServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.network_thread.take() {
            // 스레드가 패닉으로 끝났어도 이미 끝난 것이므로 결과는 확인하지 않습니다.
            let _ = thread.join();
        }
    }
}

impl ServerTransport for WsServer {
    fn poll(&mut self) -> Option<ServerEvent> {
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        match self.inbound.try_recv().ok()? {
            Inbound::Opened { link, peer, outbox } => {
                self.outboxes.insert(link, outbox);
                Some(ServerEvent::Connected(link, peer))
            }
            Inbound::Message(link, msg) => Some(ServerEvent::Message(link, msg)),
            Inbound::LimitReached(link, reason) => {
                // 이유는 접속마다 한 번만 알립니다. 접속 작업이 알려 온 이유가 있으면 그것을 씁니다.
                self.full_outboxes.remove(&link);
                Some(ServerEvent::LimitReached(link, reason))
            }
            Inbound::Closed(link) => {
                self.outboxes.remove(&link);
                match self.full_outboxes.remove(&link) {
                    Some(reason) => {
                        self.pending.push_back(ServerEvent::Disconnected(link));
                        Some(ServerEvent::LimitReached(link, reason))
                    }
                    None => Some(ServerEvent::Disconnected(link)),
                }
            }
        }
    }

    fn send(&mut self, link: LinkId, msg: &ServerMsg) {
        let Some(outbox) = self.outboxes.get(&link) else {
            return;
        };
        // 대기열이 가득 찼거나 작업이 끝났습니다. 보내는 끝을 버리면 작업이 남은 메시지를 보낸 뒤 접속을 닫고,
        // 게임 스레드는 곧 `Closed`를 받습니다. 가득 차서 닫는 접속은 그때 이유를 `Disconnected` 바로 앞에서 알립니다.
        if let Err(err) = outbox.try_send(encode_server(msg)) {
            if matches!(err, TrySendError::Full(_)) {
                self.full_outboxes.insert(
                    link,
                    format!(
                        "more than {} outgoing messages are waiting; the client reads too slowly",
                        outbox.max_capacity()
                    ),
                );
            }
            self.outboxes.remove(&link);
        }
    }

    fn close(&mut self, link: LinkId) {
        self.outboxes.remove(&link);
    }
}

/// 접속을 받는 반복입니다. `shutdown`이 오면 끝나면서 리스너를 닫습니다.
async fn accept_loop(
    listener: TcpListener,
    inbound: mpsc::Sender<Inbound>,
    limits: ServerLimits,
    web: Option<Arc<Web>>,
    mut shutdown: oneshot::Receiver<()>,
) {
    let mut seats = Seats::new(limits);
    let (release, mut released) = mpsc::unbounded_channel();
    let mut next_link = 1;
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            _ = &mut shutdown => return,
        };
        let (stream, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(_) => {
                tokio::select! {
                    () = tokio::time::sleep(ACCEPT_RETRY_DELAY) => {}
                    _ = &mut shutdown => return,
                }
                continue;
            }
        };
        while let Ok(key) = released.try_recv() {
            seats.give_back(key);
        }
        // 자리가 없으면 `stream`을 버려서 접속을 바로 닫습니다.
        let key = PeerKey::of(peer.ip());
        if !seats.take(key) {
            continue;
        }
        let seat = Seat {
            key,
            release: release.clone(),
        };
        let link = LinkId(next_link);
        next_link += 1;
        tokio::spawn(serve_link(
            stream,
            link,
            inbound.clone(),
            limits,
            seat,
            web.clone(),
        ));
    }
}

/// 열려 있는 접속(연결 절차 중인 접속 포함)의 수를 전체와 상대(`PeerKey`)별로 셉니다. accept 반복만 이 값을 소유합니다.
struct Seats {
    total: usize,
    per_key: HashMap<PeerKey, usize>,
    max_links: usize,
    max_links_per_ip: usize,
}

impl Seats {
    fn new(limits: ServerLimits) -> Seats {
        Seats {
            total: 0,
            per_key: HashMap::new(),
            max_links: limits.max_links,
            max_links_per_ip: limits.max_links_per_ip,
        }
    }

    /// 전체 자리와 그 열쇠의 자리가 모두 남아 있으면 하나씩 차지하고 `true`를 돌려줍니다.
    fn take(&mut self, key: PeerKey) -> bool {
        let used_by_key = self.per_key.get(&key).copied().unwrap_or(0);
        if self.total >= self.max_links || used_by_key >= self.max_links_per_ip {
            return false;
        }
        self.total += 1;
        self.per_key.insert(key, used_by_key + 1);
        true
    }

    fn give_back(&mut self, key: PeerKey) {
        self.total = self.total.saturating_sub(1);
        if let Some(count) = self.per_key.get_mut(&key) {
            *count -= 1;
            if *count == 0 {
                self.per_key.remove(&key);
            }
        }
    }
}

/// 접속 하나가 차지한 자리입니다. 접속 작업이 어느 경로로 끝나든(연결 절차의 실패나 시간 초과 포함)
/// 이 값이 버려지면서 accept 반복에 자리를 돌려줍니다.
struct Seat {
    key: PeerKey,
    release: mpsc::UnboundedSender<PeerKey>,
}

impl Drop for Seat {
    fn drop(&mut self) {
        let _ = self.release.send(self.key);
    }
}

/// 접속 하나를 처리합니다. 받은 프레임을 해석해서 게임 스레드에 넘기고, 송신 대기열의 메시지를 소켓에 씁니다.
/// 게임 스레드가 보내는 끝을 버리면, 대기열에 남은 메시지를 모두 쓴 뒤 닫기 프레임을 보내고 끝납니다.
/// 파일 요청이면 접속의 자리를 내놓고 파일을 보낸 뒤 끝납니다. 게임 스레드는 그 접속을 모릅니다.
async fn serve_link(
    stream: TcpStream,
    link: LinkId,
    inbound: mpsc::Sender<Inbound>,
    limits: ServerLimits,
    seat: Seat,
    web: Option<Arc<Web>>,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(limits.max_message_bytes))
        .max_frame_size(Some(limits.max_message_bytes));
    // 요청의 머리를 읽는 시간과 WebSocket 연결 절차를 합쳐서 `handshake_timeout` 안에 끝나야 합니다.
    let deadline = Instant::now() + limits.handshake_timeout;
    let replayed = match tokio::time::timeout_at(deadline, route(stream, web.as_ref())).await {
        Ok(Some(Routed::Game(replayed))) => replayed,
        Ok(Some(Routed::File(web, stream, request))) => {
            drop(seat);
            send_file(stream, &request, &web, &limits).await;
            return;
        }
        _ => return,
    };
    let handshake = tokio_tungstenite::accept_async_with_config(replayed, Some(config));
    let Ok(Ok(mut ws)) = tokio::time::timeout_at(deadline, handshake).await else {
        return;
    };
    let (outbox, mut outgoing) = mpsc::channel::<Vec<u8>>(limits.outbox_capacity);
    if inbound
        .send(Inbound::Opened {
            link,
            peer: seat.key,
            outbox,
        })
        .await
        .is_err()
    {
        return;
    }

    let mut rate = RateWindow::new();
    // 한도를 넘어서 접속을 닫을 때의 이유입니다. 게임 스레드가 로그에 남길 수 있게 알립니다.
    let mut limit_reached = None;
    // 게임 스레드에 메시지를 하나라도 넘겼는지와, 다른 버전의 입장 요청에 이미 답했는지입니다.
    let mut forwarded = false;
    let mut refused_a_version = false;
    loop {
        tokio::select! {
            frame = ws.next() => {
                // 닫기 프레임과 스트림의 끝은 클라이언트가 보낸 메시지가 아니므로 세지 않습니다.
                let counted = matches!(
                    frame,
                    Some(Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_)))
                );
                if counted && !rate.allow(Instant::now(), limits.max_messages_per_sec) {
                    limit_reached = Some(format!(
                        "sent more than {} messages in one second",
                        limits.max_messages_per_sec
                    ));
                    break;
                }
                match frame {
                    Some(Ok(Message::Binary(bytes))) => match decode_client(&bytes) {
                        // 게임 스레드의 처리가 밀리면 여기서 기다립니다. 그동안 이 소켓을 읽지 않습니다.
                        Ok(msg) => {
                            forwarded = true;
                            if inbound.send(Inbound::Message(link, msg)).await.is_err() {
                                break;
                            }
                        }
                        // 형식이 바뀐 빌드의 입장 요청은 게임 스레드가 해석할 수 없으므로 여기서 이유를 답합니다.
                        // 닫지는 않습니다. 브라우저는 닫힘을 그 이유보다 먼저 알릴 수 있어서, 이유를 읽은 클라이언트가 닫기를 기다리고
                        // 게임 스레드는 입장하지 않은 접속을 입장 기한에 닫습니다.
                        Err(err) if err.foreign_join_version().is_some() && !forwarded && !refused_a_version => {
                            refused_a_version = true;
                            let refusal = encode_server(&ServerMsg::JoinRejected {
                                reason: RejectReason::VersionMismatch { server: PROTOCOL_VERSION },
                            });
                            let write = tokio::time::timeout(limits.write_timeout, ws.send(Message::binary(refusal)));
                            if !matches!(write.await, Ok(Ok(()))) {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                    // tungstenite가 Ping에 직접 답합니다.
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                    Some(Err(Error::Capacity(_))) => {
                        limit_reached = Some(format!(
                            "sent a message larger than {} bytes",
                            limits.max_message_bytes
                        ));
                        break;
                    }
                    // 모든 메시지는 이진 프레임입니다. 문자열 프레임은 약속 위반입니다.
                    Some(Ok(Message::Text(_) | Message::Close(_)) | Err(_)) | None => break,
                }
            }
            bytes = outgoing.recv() => match bytes {
                Some(bytes) => {
                    let write = tokio::time::timeout(limits.write_timeout, ws.send(Message::binary(bytes)));
                    match write.await {
                        Ok(Ok(())) => {}
                        Ok(Err(_)) => break,
                        Err(_) => {
                            // 게임 스레드가 보내는 끝을 이미 버렸다면 그쪽이 닫은 이유를 알고 있으므로, 같은 접속의 이유를 두 번 알리지 않습니다.
                            if !outgoing.is_closed() {
                                limit_reached = Some(format!(
                                    "a write did not finish within {:?}; the client reads too slowly",
                                    limits.write_timeout
                                ));
                            }
                            break;
                        }
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
    if let Some(reason) = limit_reached {
        let _ = inbound.send(Inbound::LimitReached(link, reason)).await;
    }
    let _ = inbound.send(Inbound::Closed(link)).await;
}

/// 파일을 내주는 서버의 접속 작업이 함께 쓰는 것입니다.
struct Web {
    files: WebFiles,
    /// 동시에 보내는 파일 응답의 자리입니다(`max_file_responses`).
    slots: Semaphore,
}

/// 이미 읽은 요청의 머리를 먼저 돌려주고, 그다음부터는 소켓에서 읽는 스트림입니다.
/// tungstenite는 접속을 한 바이트도 읽지 않은 것처럼 요청을 처음부터 읽고 검사합니다.
type Replayed = Join<Chain<Cursor<Vec<u8>>, OwnedReadHalf>, OwnedWriteHalf>;

/// 첫 요청의 머리로 가른 접속입니다.
enum Routed {
    Game(Replayed),
    File(Arc<Web>, TcpStream, Request),
}

/// 파일 요청에 답하는 데 필요한 요청 줄입니다.
struct Request {
    method: String,
    target: String,
}

/// 요청의 머리를 읽은 결과입니다.
enum Head {
    /// 머리가 아직 끝나지 않았습니다.
    Incomplete,
    /// 머리를 끝까지 읽었고, `Upgrade: websocket` 머리가 있었는지를 함께 가집니다.
    Complete(Request, bool),
    /// HTTP 요청이 아니거나 머리 줄의 수가 한도를 넘었습니다.
    Refused,
}

/// 파일을 내주는 서버(`web`)는 요청의 머리를 읽어서 `Upgrade: websocket` 머리가 없으면 파일 요청으로 돌려줍니다.
/// 파일을 내주지 않는 서버는 머리를 읽지 않고, 모든 접속을 게임 접속으로 넘깁니다.
/// 읽을 수 없는 요청과 한도를 넘는 머리는 `None`입니다.
async fn route(mut stream: TcpStream, web: Option<&Arc<Web>>) -> Option<Routed> {
    let Some(web) = web else {
        return Some(Routed::Game(replay(Vec::new(), stream)));
    };
    let mut read = Vec::new();
    loop {
        let mut chunk = [0u8; 2048];
        let room = (MAX_HEAD_BYTES - read.len()).min(chunk.len());
        let n = stream.read(&mut chunk[..room]).await.ok()?;
        if n == 0 {
            return None;
        }
        read.extend_from_slice(&chunk[..n]);
        match parse_head(&read) {
            Head::Complete(_, true) => return Some(Routed::Game(replay(read, stream))),
            Head::Complete(request, false) => {
                return Some(Routed::File(Arc::clone(web), stream, request));
            }
            Head::Incomplete if read.len() < MAX_HEAD_BYTES => {}
            Head::Incomplete | Head::Refused => return None,
        }
    }
}

fn replay(read: Vec<u8>, stream: TcpStream) -> Replayed {
    let (reader, writer) = stream.into_split();
    tokio::io::join(Cursor::new(read).chain(reader), writer)
}

fn parse_head(read: &[u8]) -> Head {
    let mut lines = [httparse::EMPTY_HEADER; MAX_HEAD_LINES];
    let mut parsed = httparse::Request::new(&mut lines);
    match parsed.parse(read) {
        Ok(httparse::Status::Complete(_)) => {
            let (Some(method), Some(target)) = (parsed.method, parsed.path) else {
                return Head::Refused;
            };
            let websocket = parsed.headers.iter().any(|line| {
                line.name.eq_ignore_ascii_case("upgrade")
                    && std::str::from_utf8(line.value).is_ok_and(|value| {
                        value
                            .split(',')
                            .any(|token| token.trim().eq_ignore_ascii_case("websocket"))
                    })
            });
            let request = Request {
                method: method.to_string(),
                target: target.to_string(),
            };
            Head::Complete(request, websocket)
        }
        Ok(httparse::Status::Partial) => Head::Incomplete,
        Err(_) => Head::Refused,
    }
}

/// 파일 요청에 답하고 접속을 닫습니다. 파일 응답의 자리가 없으면 `503`으로 답합니다.
/// 쓰기 하나(머리, 본문의 조각 하나)가 `write_timeout` 안에 끝나지 않거나, 응답 전체가 본문의 크기가 허락하는 시간
/// (`file_response_grace`에 본문의 크기를 `file_min_bytes_per_sec`로 나눈 시간을 더한 값) 안에 끝나지 않으면 그대로 닫습니다.
/// 그래서 읽기를 멈춘 상대도, 조금씩 계속 읽는 상대도 파일 응답의 자리를 정해진 시간보다 오래 차지하지 못합니다.
async fn send_file(mut stream: TcpStream, request: &Request, web: &Web, limits: &ServerLimits) {
    // 자리는 이 함수가 끝날 때(접속을 닫을 때) 돌아갑니다.
    let slot = web.slots.try_acquire();
    let answer = match slot {
        Ok(_) => web.files.answer(&request.method, &request.target),
        Err(_) => Answer::busy(&request.method),
    };
    let allowed = limits.file_response_grace
        + Duration::from_secs_f64(
            answer.body.len() as f64 / limits.file_min_bytes_per_sec.max(1) as f64,
        );
    let write_timeout = limits.write_timeout;
    let send = async {
        for part in std::iter::once(&answer.head[..]).chain(answer.body.chunks(FILE_CHUNK)) {
            let write = tokio::time::timeout(write_timeout, stream.write_all(part));
            if !matches!(write.await, Ok(Ok(()))) {
                return;
            }
        }
        let _ = tokio::time::timeout(write_timeout, stream.shutdown()).await;
    };
    let _ = tokio::time::timeout(allowed, send).await;
}

/// 접속 하나가 지금의 1초 구간에 보낸 메시지의 수를 셉니다.
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

    /// 메시지 하나를 세고, 이번 구간의 한도 안이면 `true`를 돌려줍니다.
    fn allow(&mut self, now: Instant, max_per_window: u32) -> bool {
        if now.duration_since(self.started) >= RATE_WINDOW {
            self.started = now;
            self.count = 0;
        }
        self.count += 1;
        self.count <= max_per_window
    }
}
