//! 실제 소켓 위에서 조립한 서버를 확인합니다.
//!
//! 서버의 규칙은 `caravan_server`의 테스트가 가짜 시계와 프로세스 내부 전달로 모두 확인합니다.
//! 이 파일은 같은 서버가 WebSocket 위에서도 같은 결과를 내는지, 즉 접속 두 개가 한 방에 들어가서
//! 한쪽의 이동 입력이 다른 쪽이 받은 스냅샷에서 그 플레이어의 움직임으로 보이는지를 확인합니다.
//!
//! 이 테스트는 서버(`caravan_server`)와 WebSocket 어댑터(`caravan_ws_server`)를 함께 쓰므로, 둘을 모두 아는 조립 크레이트에 둡니다.
//! 어댑터 크레이트는 게임 서버에 의존하지 않기 때문에 어댑터의 `tests`에는 둘 수 없습니다.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, InputMsg, PROTOCOL_VERSION, ServerLimits, ServerMsg, Snapshot, WireVec2,
    decode_server, encode_client,
};
use caravan_rules::Tuning;
use caravan_server::GameServer;
use caravan_ws_server::WsServer;
use tokio_tungstenite::tungstenite::stream::MaybeTlsStream;
use tokio_tungstenite::tungstenite::{Error, Message, WebSocket, connect};

/// 실행 파일(`main.rs`)과 같은 방식으로 스레드 하나에서 서버를 돌립니다. 버려지면 스레드를 멈춥니다.
struct RunningServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl RunningServer {
    fn start() -> RunningServer {
        let limits = ServerLimits::default();
        let ws = WsServer::bind("127.0.0.1:0", limits).expect("bind a free port");
        let addr = ws.local_addr();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut server =
                GameServer::new(Box::new(ws), Tuning::default(), limits, Instant::now())
                    .expect("the default tuning is valid");
            while !stopped.load(Ordering::Relaxed) {
                let now = Instant::now();
                server.pump(now);
                thread::sleep(
                    server
                        .time_until_next_tick(now)
                        .min(Duration::from_millis(5)),
                );
            }
        });
        RunningServer {
            addr,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// 블로킹 WebSocket 클라이언트입니다.
struct TestClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl TestClient {
    fn connect(addr: SocketAddr) -> TestClient {
        let (mut socket, _) = connect(format!("ws://{addr}")).expect("connect to the server");
        // 읽기가 영원히 기다리지 않도록 짧게 끊어서 마감 시각을 확인합니다.
        if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .expect("set a read timeout");
        }
        TestClient { socket }
    }

    fn send(&mut self, msg: &ClientMsg) {
        self.socket
            .send(Message::binary(encode_client(msg)))
            .expect("send");
    }

    /// 서버 메시지 하나를 기다립니다. `within` 안에 오지 않으면 테스트를 실패시킵니다.
    fn next(&mut self, within: Duration) -> ServerMsg {
        let deadline = Instant::now() + within;
        loop {
            match self.socket.read() {
                Ok(Message::Binary(bytes)) => {
                    return decode_server(&bytes).expect("the server sends valid messages");
                }
                Ok(_) => {}
                Err(Error::Io(err))
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(err) => panic!("the connection failed: {err}"),
            }
            assert!(
                Instant::now() < deadline,
                "no server message within {within:?}"
            );
        }
    }

    /// `ready`가 참이 되는 스냅샷이 올 때까지 읽고, 그 스냅샷을 돌려줍니다.
    fn snapshot_where(&mut self, within: Duration, ready: impl Fn(&Snapshot) -> bool) -> Snapshot {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if let ServerMsg::Snapshot(snapshot) = self.next(left.max(Duration::from_millis(1)))
                && ready(&snapshot)
            {
                return snapshot;
            }
        }
    }

    fn join(&mut self, room: &str) -> u32 {
        self.send(&ClientMsg::Join {
            version: PROTOCOL_VERSION,
            room: room.to_string(),
        });
        match self.next(Duration::from_secs(5)) {
            ServerMsg::Welcome(welcome) => welcome.you,
            other => panic!("expected Welcome, got {other:?}"),
        }
    }
}

#[test]
fn input_from_one_socket_moves_that_player_in_the_snapshots_of_the_other_socket() {
    let server = RunningServer::start();
    let mut a = TestClient::connect(server.addr);
    let mut b = TestClient::connect(server.addr);
    let a_id = a.join("real");
    let b_id = b.join("real");
    assert_eq!((a_id, b_id), (1, 2));

    let both_in_room = |snapshot: &Snapshot| snapshot.players.len() == 2;
    let before = b.snapshot_where(Duration::from_secs(5), both_in_room);
    let start_x = before.players[0].pos.to_tiles().x;

    a.send(&ClientMsg::Input(InputMsg {
        seq: 1,
        move_dir: WireVec2::new(1.0, 0.0),
        ..InputMsg::default()
    }));

    let moved = b.snapshot_where(Duration::from_secs(5), |snapshot| {
        snapshot
            .players
            .iter()
            .any(|p| p.id == a_id && p.pos.to_tiles().x > start_x + 0.5)
    });
    let seen_a = moved.players.iter().find(|p| p.id == a_id).unwrap();
    let seen_b = moved.players.iter().find(|p| p.id == b_id).unwrap();
    assert_eq!(seen_a.last_input_seq, 1, "the server applied A's input");
    assert_eq!(
        seen_b.pos.to_tiles().x,
        before.players[1].pos.to_tiles().x,
        "B, who sent nothing, did not move"
    );
}
