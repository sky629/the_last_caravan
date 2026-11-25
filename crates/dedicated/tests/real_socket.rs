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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, InputMsg, PROTOCOL_VERSION, SeatToken, ServerLimits, ServerMsg, Snapshot, Welcome,
    WireMember, WireVec2, decode_server, encode_client, loopback,
};
use caravan_rules::{Seed, Tuning};
use caravan_server::{GameServer, Notice, PredictableRandom, RandomSource, RoomSeeds};
use caravan_ws_server::WsServer;
use tokio_tungstenite::tungstenite::stream::MaybeTlsStream;
use tokio_tungstenite::tungstenite::{Error, Message, WebSocket, connect};

/// 새 방의 코드와 자리의 표식을 만드는 무작위 원천입니다. 서버를 다시 만들어도 같은 순서로 같은 값이 나옵니다.
fn random() -> Box<dyn RandomSource> {
    Box::new(PredictableRandom::new(1))
}

/// 실행 파일(`main.rs`)과 같은 방식으로 스레드 하나에서 서버를 돌립니다. 버려지면 스레드를 멈춥니다.
struct RunningServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    /// 서버가 지금까지 알린 일입니다.
    notices: Arc<Mutex<Vec<Notice>>>,
    thread: Option<JoinHandle<()>>,
}

impl RunningServer {
    fn start(tuning: Tuning) -> RunningServer {
        let limits = ServerLimits::default();
        let ws = WsServer::bind("127.0.0.1:0", limits).expect("bind a free port");
        let addr = ws.local_addr();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let notices = Arc::new(Mutex::new(Vec::new()));
        let reported = Arc::clone(&notices);
        let thread = thread::spawn(move || {
            let mut server = GameServer::new(
                Box::new(ws),
                tuning,
                limits,
                RoomSeeds::Fixed(Seed(1)),
                random(),
                Instant::now(),
            )
            .expect("the tuning is valid and fits the limits");
            while !stopped.load(Ordering::Relaxed) {
                let now = Instant::now();
                let news = server.pump(now);
                reported
                    .lock()
                    .expect("the test thread is alive")
                    .extend(news);
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
            notices,
            thread: Some(thread),
        }
    }

    /// 서버가 `wanted`에 맞는 일을 알릴 때까지 기다리고, 그 일을 돌려줍니다.
    fn wait_for_notice(&self, wanted: impl Fn(&Notice) -> bool) -> Notice {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let found = self
                .notices
                .lock()
                .expect("the server thread is alive")
                .iter()
                .find(|notice| wanted(notice))
                .cloned();
            if let Some(notice) = found {
                return notice;
            }
            assert!(Instant::now() < deadline, "the server never reported it");
            thread::sleep(Duration::from_millis(10));
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
                return *snapshot;
            }
        }
    }

    /// 이름 `name`으로 입장하고, 받은 입장 안내를 돌려줍니다. `room`이 없으면 서버가 새 방을 만듭니다.
    fn join_as(&mut self, room: Option<&str>, name: &str) -> Welcome {
        self.join_holding(room, name, None)
    }

    /// `join_as`와 같고, 자리의 표식 `token`을 함께 보냅니다.
    fn join_holding(
        &mut self,
        room: Option<&str>,
        name: &str,
        token: Option<SeatToken>,
    ) -> Welcome {
        self.send(&ClientMsg::Join {
            version: PROTOCOL_VERSION,
            room: room.map(str::to_string),
            name: name.to_string(),
            token,
        });
        match self.next(Duration::from_secs(5)) {
            ServerMsg::Welcome(welcome) => *welcome,
            other => panic!("expected Welcome, got {other:?}"),
        }
    }

    /// 이름 `TESTER`로 방 `room`에 입장하고, 받은 플레이어 번호를 돌려줍니다.
    fn join(&mut self, room: &str) -> u32 {
        self.join_as(Some(room), "TESTER").you
    }

    /// `ready`가 참이 되는 명단이 올 때까지 읽고, 그 명단을 돌려줍니다.
    fn roster_where(
        &mut self,
        within: Duration,
        ready: impl Fn(&[WireMember]) -> bool,
    ) -> Vec<WireMember> {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if let ServerMsg::Roster(roster) = self.next(left.max(Duration::from_millis(1)))
                && ready(&roster.members)
            {
                return roster.members;
            }
        }
    }
}

#[test]
fn input_from_one_socket_moves_that_player_in_the_snapshots_of_the_other_socket() {
    let server = RunningServer::start(Tuning::default());
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

#[test]
fn a_player_without_a_room_code_gets_a_new_room_and_a_friend_who_knows_the_code_sees_the_same_names()
 {
    let server = RunningServer::start(Tuning::default());
    let mut kang = TestClient::connect(server.addr);
    let mut miro = TestClient::connect(server.addr);

    let first = kang.join_as(None, "KANG");
    assert_eq!(first.room.len(), 5, "the server made the room code");
    let second = miro.join_as(Some(&first.room), "MIRO");
    assert_eq!(second.room, first.room, "the friend is in the same room");

    let both = |roster: &[WireMember]| roster.len() == 2;
    let names = |roster: Vec<WireMember>| -> Vec<(u32, String)> {
        roster.into_iter().map(|m| (m.id, m.name)).collect()
    };
    let expected = vec![(1, "KANG".to_string()), (2, "MIRO".to_string())];
    let seen_by_kang = kang.roster_where(Duration::from_secs(5), both);
    let seen_by_miro = miro.roster_where(Duration::from_secs(5), both);
    assert_eq!(names(seen_by_kang), expected);
    assert_eq!(names(seen_by_miro), expected);
}

#[test]
fn a_player_who_drops_and_returns_with_the_saved_token_gets_the_same_seat() {
    let server = RunningServer::start(Tuning::default());
    let mut first = TestClient::connect(server.addr);
    let welcome = first.join_as(None, "KANG");
    let mut friend = TestClient::connect(server.addr);
    friend.join_as(Some(&welcome.room), "MIRO");
    drop(first);
    server.wait_for_notice(|notice| matches!(notice, Notice::Held { .. }));

    let mut back = TestClient::connect(server.addr);
    let again = back.join_holding(Some(&welcome.room), "KANG", Some(welcome.token));

    assert_eq!(again.you, welcome.you, "the same player");
    assert_eq!(again.token, welcome.token, "the same seat");
    server.wait_for_notice(|notice| matches!(notice, Notice::Returned { .. }));
    let both = |roster: &[WireMember]| roster.len() == 2;
    assert_eq!(back.roster_where(Duration::from_secs(5), both).len(), 2);
    let notices = server.notices.lock().expect("the server thread is alive");
    assert!(
        !notices
            .iter()
            .any(|notice| matches!(notice, Notice::Left { .. })),
        "the seat was never given up"
    );
}

/// 기본 접속 한도에서 서버가 시작을 허락하는 가장 빠른 Tick 속도입니다.
fn fastest_tick_hz_the_default_limits_carry() -> u32 {
    (1..=240)
        .rev()
        .find(|&tick_hz| {
            let (transport, _connector) = loopback();
            let tuning = Tuning {
                tick_hz,
                ..Tuning::default()
            };
            GameServer::new(
                Box::new(transport),
                tuning,
                ServerLimits::default(),
                RoomSeeds::Fixed(Seed(1)),
                random(),
                Instant::now(),
            )
            .is_ok()
        })
        .expect("some tick rate starts")
}

#[test]
fn a_client_whose_inputs_arrive_in_a_burst_after_a_three_second_stall_stays_connected() {
    // 서버가 시작을 허락하는 가장 빠른 Tick 속도가 접속 한도에 가장 가깝습니다.
    let tick_hz = fastest_tick_hz_the_default_limits_carry();
    let server = RunningServer::start(Tuning {
        tick_hz,
        ..Tuning::default()
    });
    let mut client = TestClient::connect(server.addr);
    client.join("burst");

    let mut seq = 0;
    let mut send_input = |client: &mut TestClient| {
        seq += 1;
        client.send(&ClientMsg::Input(InputMsg {
            seq,
            ..InputMsg::default()
        }));
        seq
    };
    // 3초 동안 멈춰 있던 클라이언트의 입력이 한꺼번에 도착하고, 그 뒤로 Tick마다 하나씩 도착합니다.
    for _ in 0..3 * tick_hz {
        send_input(&mut client);
    }
    let mut last = 0;
    for _ in 0..tick_hz {
        client.next(Duration::from_secs(5));
        last = send_input(&mut client);
    }

    let reached = client.snapshot_where(Duration::from_secs(5), |snapshot| {
        snapshot.players[0].last_input_seq == last
    });
    assert_eq!(reached.players[0].last_input_seq, last, "the link survived");
}

#[test]
fn a_link_cut_by_the_message_rate_limit_is_reported_with_the_limit_it_broke() {
    let server = RunningServer::start(Tuning::default());
    let mut client = TestClient::connect(server.addr);
    client.join("flood");

    // 한도를 넘으면 서버가 접속을 닫으므로, 그 뒤의 전송 실패는 무시합니다.
    for seq in 1..=300 {
        let input = encode_client(&ClientMsg::Input(InputMsg {
            seq,
            ..InputMsg::default()
        }));
        let _ = client.socket.send(Message::binary(input));
    }

    let closed = server.wait_for_notice(|notice| matches!(notice, Notice::LinkClosed { .. }));
    let Notice::LinkClosed { reason, .. } = closed else {
        unreachable!("the wanted notice is a LinkClosed");
    };
    assert_eq!(
        reason,
        format!(
            "sent more than {} messages in one second",
            ServerLimits::default().max_messages_per_sec
        )
    );
    server.wait_for_notice(|notice| matches!(notice, Notice::Held { .. }));
}
