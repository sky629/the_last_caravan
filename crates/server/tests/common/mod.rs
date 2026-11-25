//! 서버 테스트가 함께 쓰는 도구입니다. 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.
//!
//! 테스트 파일마다 이 모듈을 따로 컴파일하므로, 한 파일이 쓰지 않는 함수는 사용하지 않는 코드로 경고됩니다.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, InputMsg, LoopbackClient, LoopbackConnector,
    PROTOCOL_VERSION, SeatToken, ServerLimits, ServerMsg, Snapshot, Welcome, WireMember,
    WirePlayer, WireRoster, WireVec2, loopback,
};
use caravan_rules::{Seed, Start, Tuning, Vec2};
use caravan_server::{GameServer, Notice, PredictableRandom, RandomSource, RoomSeeds};

/// 새 방의 코드와 자리의 표식을 만드는 무작위 원천입니다. 서버를 다시 만들어도 같은 순서로 같은 값이 나옵니다.
pub fn random() -> Box<dyn RandomSource> {
    Box::new(PredictableRandom::new(1))
}

/// 시험용 이름입니다.
pub const NAME: &str = "TESTER";

pub struct Rig {
    pub server: GameServer,
    pub connector: LoopbackConnector,
    pub now: Instant,
    pub period: Duration,
}

pub fn rig() -> Rig {
    rig_with(Tuning::default())
}

pub fn rig_with(tuning: Tuning) -> Rig {
    rig_with_limits(tuning, ServerLimits::default())
}

/// 방이 만들어지자마자 첫날이 시작되는 서버입니다. 야영지를 거치지 않고, 원정 중인 방에서 일어나는 일(낮과 밤, 몬스터, 원정 중의 입장)을 시험할 때 씁니다.
/// 기본 서버는 방이 야영지에서 시작합니다.
pub fn rig_in_expedition(mut tuning: Tuning) -> Rig {
    tuning.camp.start = Start::AtOnce;
    rig_with(tuning)
}

pub fn rig_with_limits(tuning: Tuning, limits: ServerLimits) -> Rig {
    rig_with_seeds(tuning, limits, RoomSeeds::Fixed(Seed(1)))
}

pub fn rig_with_seeds(tuning: Tuning, limits: ServerLimits, seeds: RoomSeeds) -> Rig {
    rig_with_random(tuning, limits, seeds, random())
}

pub fn rig_with_random(
    tuning: Tuning,
    limits: ServerLimits,
    seeds: RoomSeeds,
    random: Box<dyn RandomSource>,
) -> Rig {
    let (transport, connector) = loopback();
    let now = Instant::now();
    let period = Duration::from_secs(1) / tuning.tick_hz;
    Rig {
        server: GameServer::new(Box::new(transport), tuning, limits, seeds, random, now)
            .expect("the test tuning is valid"),
        connector,
        now,
        period,
    }
}

impl Rig {
    /// 시각을 바꾸지 않고 서버를 한 번 진행합니다. 입장과 퇴장은 처리되지만 Tick은 실행되지 않습니다.
    pub fn pump(&mut self) -> Vec<Notice> {
        self.server.pump(self.now)
    }

    /// Tick 간격만큼씩 `ticks`번 시각을 옮기면서 서버를 진행합니다.
    pub fn run_ticks(&mut self, ticks: u32) -> Vec<Notice> {
        let mut notices = Vec::new();
        for _ in 0..ticks {
            self.now += self.period;
            notices.extend(self.server.pump(self.now));
        }
        notices
    }

    /// 클라이언트처럼 Tick마다 아무것도 누르지 않은 입력을 하나씩 보내면서 `ticks`번 진행합니다. 입력이 3초 넘게 없으면 서버가 자리를 비운 것으로 보기 때문입니다.
    /// `first_seq`는 첫 입력의 순서 번호이고, 다음에 쓸 순서 번호를 돌려줍니다.
    pub fn run_ticks_talking(
        &mut self,
        clients: &mut [&mut LoopbackClient],
        first_seq: u32,
        ticks: u32,
    ) -> u32 {
        let mut seq = first_seq;
        for _ in 0..ticks {
            for client in clients.iter_mut() {
                client.send(&input(seq));
            }
            seq += 1;
            self.run_ticks(1);
        }
        seq
    }

    /// 접속하고 이름 `NAME`으로 `Join`을 보낸 뒤, 서버가 보낸 첫 답을 돌려줍니다.
    pub fn join(&mut self, room: &str) -> (LoopbackClient, ServerMsg) {
        self.join_as(Some(room), NAME)
    }

    /// `join`과 같지만 이름과 방 코드를 고릅니다. 방 코드가 없으면 서버가 새 방을 만듭니다.
    pub fn join_as(&mut self, room: Option<&str>, name: &str) -> (LoopbackClient, ServerMsg) {
        self.join_msg(&join_msg(PROTOCOL_VERSION, room, name))
    }

    pub fn join_with_version(&mut self, room: &str, version: u32) -> (LoopbackClient, ServerMsg) {
        self.join_msg(&join_msg(version, Some(room), NAME))
    }

    /// 접속하고 `msg`를 보낸 뒤, 서버가 보낸 첫 답을 돌려줍니다.
    pub fn join_msg(&mut self, msg: &ClientMsg) -> (LoopbackClient, ServerMsg) {
        let mut client = self.connector.connect();
        assert!(matches!(client.poll(), Some(ClientEvent::Connected)));
        client.send(msg);
        self.pump();
        match client.poll() {
            Some(ClientEvent::Message(answer)) => (client, answer),
            other => panic!("expected an answer to Join, got {other:?}"),
        }
    }

    /// `join_ok`와 같지만, 입장을 처리하면서 서버가 알린 일도 함께 돌려줍니다.
    pub fn join_noticed(&mut self, room: &str) -> (LoopbackClient, Vec<Notice>) {
        let mut client = self.connector.connect();
        assert!(matches!(client.poll(), Some(ClientEvent::Connected)));
        client.send(&join_msg(PROTOCOL_VERSION, Some(room), NAME));
        let notices = self.pump();
        assert!(matches!(
            client.poll(),
            Some(ClientEvent::Message(ServerMsg::Welcome(_)))
        ));
        (client, notices)
    }

    pub fn join_ok(&mut self, room: &str) -> (LoopbackClient, u32) {
        match self.join(room) {
            (client, ServerMsg::Welcome(welcome)) => (client, welcome.you),
            (_, other) => panic!("expected Welcome, got {other:?}"),
        }
    }

    /// 이름과 표식을 정해 입장하고 입장 안내를 돌려줍니다. 입장하지 못하면 테스트가 실패합니다.
    pub fn join_welcomed(
        &mut self,
        room: &str,
        name: &str,
        token: Option<SeatToken>,
    ) -> (LoopbackClient, Welcome) {
        match self.join_msg(&join_msg_with_token(
            PROTOCOL_VERSION,
            Some(room),
            name,
            token,
        )) {
            (client, ServerMsg::Welcome(welcome)) => (client, *welcome),
            (_, other) => panic!("expected Welcome, got {other:?}"),
        }
    }

    /// 클라이언트의 접속이 끊긴 것으로 하고, 서버가 그것을 처리하게 합니다.
    pub fn drop_link(&mut self, client: LoopbackClient) {
        drop(client);
        self.pump();
    }

    /// 이 서버의 Tick 속도에서 `secs`초가 몇 Tick인지 돌려줍니다.
    pub fn ticks(&self, secs: f32) -> u32 {
        (secs / self.period.as_secs_f32()).round() as u32
    }
}

/// 클라이언트가 지금까지 받은 것을 한 번에 꺼낸 것입니다. 스냅샷과 명단을 따로 꺼내면 앞서 꺼낸 쪽이 나머지를 버리므로, 둘 다 볼 때 씁니다.
#[derive(Debug, Default)]
pub struct Heard {
    pub snapshots: Vec<Snapshot>,
    pub rosters: Vec<WireRoster>,
    /// `SeatTakenOver`를 받았습니다.
    pub taken_over: bool,
    /// 서버가 접속을 닫았습니다.
    pub closed: bool,
}

impl Heard {
    pub fn last_snapshot(&self) -> &Snapshot {
        self.snapshots
            .last()
            .expect("at least one snapshot arrived")
    }

    pub fn last_roster(&self) -> &[WireMember] {
        &self
            .rosters
            .last()
            .expect("at least one roster arrived")
            .members
    }
}

/// 클라이언트에 와 있는 이벤트를 모두 꺼냅니다.
pub fn hear(client: &mut LoopbackClient) -> Heard {
    let mut heard = Heard::default();
    for event in drain(client) {
        match event {
            ClientEvent::Message(ServerMsg::Snapshot(snapshot)) => heard.snapshots.push(*snapshot),
            ClientEvent::Message(ServerMsg::Roster(roster)) => heard.rosters.push(roster),
            ClientEvent::Message(ServerMsg::SeatTakenOver) => heard.taken_over = true,
            ClientEvent::Disconnected { .. } => heard.closed = true,
            ClientEvent::Connected | ClientEvent::Message(_) => {}
        }
    }
    heard
}

/// 지금까지 받은 명단을 받은 순서대로 돌려줍니다.
pub fn rosters(client: &mut LoopbackClient) -> Vec<WireRoster> {
    drain(client)
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Message(ServerMsg::Roster(roster)) => Some(roster),
            _ => None,
        })
        .collect()
}

/// 지금까지 받은 명단 가운데 가장 마지막 것의 사람들입니다.
pub fn last_roster(client: &mut LoopbackClient) -> Vec<WireMember> {
    rosters(client)
        .pop()
        .expect("at least one roster arrived")
        .members
}

/// 입장 요청입니다. 표식은 없습니다.
pub fn join_msg(version: u32, room: Option<&str>, name: &str) -> ClientMsg {
    join_msg_with_token(version, room, name, None)
}

/// 표식을 실은 입장 요청입니다.
pub fn join_msg_with_token(
    version: u32,
    room: Option<&str>,
    name: &str,
    token: Option<SeatToken>,
) -> ClientMsg {
    ClientMsg::Join {
        version,
        room: room.map(str::to_string),
        name: name.to_string(),
        token,
    }
}

/// 준비 키(R)를 한 번 누른 입력입니다.
pub fn press_ready(seq: u32) -> ClientMsg {
    ClientMsg::Input(InputMsg {
        seq,
        toggle_ready: true,
        ..InputMsg::default()
    })
}

pub fn input(seq: u32) -> ClientMsg {
    ClientMsg::Input(InputMsg {
        seq,
        ..InputMsg::default()
    })
}

pub fn drain(client: &mut LoopbackClient) -> Vec<ClientEvent> {
    std::iter::from_fn(|| client.poll()).collect()
}

pub fn snapshots(client: &mut LoopbackClient) -> Vec<Snapshot> {
    drain(client)
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Message(ServerMsg::Snapshot(snapshot)) => Some(*snapshot),
            _ => None,
        })
        .collect()
}

/// 지금까지 받은 스냅샷 가운데 가장 마지막 것입니다.
pub fn last_snapshot(client: &mut LoopbackClient) -> Snapshot {
    snapshots(client)
        .pop()
        .expect("at least one snapshot arrived")
}

pub fn player_ids(snapshot: &Snapshot) -> Vec<u32> {
    snapshot.players.iter().map(|player| player.id).collect()
}

pub fn ticks(snapshots: &[Snapshot]) -> Vec<u64> {
    snapshots.iter().map(|s| s.tick).collect()
}

/// 방향과 버튼을 지정한 입력입니다. 조준은 비워 둡니다.
pub fn input_with(seq: u32, move_dir: (f32, f32), attack: bool) -> ClientMsg {
    ClientMsg::Input(InputMsg {
        seq,
        move_dir: WireVec2::new(move_dir.0, move_dir.1),
        attack,
        ..InputMsg::default()
    })
}

pub fn player(snapshot: &Snapshot, id: u32) -> &WirePlayer {
    snapshot
        .players
        .iter()
        .find(|player| player.id == id)
        .unwrap_or_else(|| panic!("player {id} is not in {snapshot:?}"))
}

/// 스냅샷에 실린 플레이어의 위치(타일)입니다.
pub fn position(snapshot: &Snapshot, id: u32) -> Vec2 {
    player(snapshot, id).pos.to_tiles()
}

/// 위치를 정수로 줄이는 오차보다 크고 한 Tick의 거리보다 훨씬 작은 허용 오차입니다.
const TOLERANCE: f32 = 0.01;

pub fn assert_near(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "{what}: expected {expected}, got {actual}"
    );
}
