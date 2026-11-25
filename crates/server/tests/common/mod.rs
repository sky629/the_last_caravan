//! 서버 테스트가 함께 쓰는 도구입니다. 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.
//!
//! 테스트 파일마다 이 모듈을 따로 컴파일하므로, 한 파일이 쓰지 않는 함수는 사용하지 않는 코드로 경고됩니다.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, InputMsg, LoopbackClient, LoopbackConnector,
    PROTOCOL_VERSION, ServerLimits, ServerMsg, Snapshot, WirePlayer, WireVec2, loopback,
};
use caravan_rules::{Seed, Start, Tuning, Vec2};
use caravan_server::{GameServer, Notice, RoomSeeds, ServerSecret};

/// 새 방의 코드를 정하는 고정된 비밀 값입니다. 서버를 다시 만들어도 같은 순서로 같은 코드가 나옵니다.
pub const SECRET: ServerSecret = ServerSecret { k0: 1, k1: 2 };

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
    let (transport, connector) = loopback();
    let now = Instant::now();
    let period = Duration::from_secs(1) / tuning.tick_hz;
    Rig {
        server: GameServer::new(Box::new(transport), tuning, limits, seeds, SECRET, now)
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
}

/// 입장 요청입니다.
pub fn join_msg(version: u32, room: Option<&str>, name: &str) -> ClientMsg {
    ClientMsg::Join {
        version,
        room: room.map(str::to_string),
        name: name.to_string(),
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
