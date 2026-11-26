//! Bevy 시스템 테스트가 함께 쓰는 도구입니다. 화면과 GPU 없이 입장을 마친 화면 흐름을 만듭니다.

use caravan_client_core::{Flow, Frame, LaunchOptions, Memory};
use std::sync::atomic::{AtomicU32, Ordering};

use caravan_protocol::{
    ClientEvent, RoomCode, SeatToken, ServerMsg, Welcome, WireMember, WirePhase, WireRoster,
    WireWagon,
};
use caravan_rules::{Seed, Sim, Tuning};

use crate::net::ClientFlow;

/// 테스트의 플레이어 번호입니다.
pub const YOU: u32 = 1;

/// 기본 수치의 방 `test`에 이름 `KANG`, 플레이어 번호 `YOU`로 입장을 마친 화면 흐름입니다.
pub fn joined_flow() -> ClientFlow {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    let first = sim.add_player().unwrap();
    let room = RoomCode::parse("test").unwrap();
    let mut welcome =
        Welcome::from_sim(&sim, sim.player(first).unwrap(), &room, SeatToken([7; 16]));
    welcome.you = YOU;
    let launch = LaunchOptions::from_values(
        Some("test".to_string()),
        Some("KANG".to_string()),
        None,
        false,
    );
    // 이름과 방 코드가 모두 있으므로 시작 화면 없이 접속을 시작합니다.
    let (mut flow, _) = Flow::new(launch, Memory::default(), None);
    flow.on_event(ClientEvent::Connected);
    flow.on_event(ClientEvent::Message(ServerMsg::Welcome(Box::new(welcome))));
    ClientFlow(flow)
}

/// 서버가 보낸 명단이 도착한 것으로 합니다. 각 항목은 (플레이어 번호, 이름)이고, 모두 자리에 있습니다.
/// 부를 때마다 더 큰 번호를 붙이므로, 나중에 부른 명단이 앞의 명단을 대신합니다.
pub fn deliver_roster(flow: &mut ClientFlow, members: &[(u32, &str)]) {
    let members: Vec<_> = members
        .iter()
        .map(|(id, name)| (*id, *name, true))
        .collect();
    deliver_presence(flow, &members);
}

/// `deliver_roster`와 같되, 각 항목이 (플레이어 번호, 이름, 자리에 있는지)입니다.
pub fn deliver_presence(flow: &mut ClientFlow, members: &[(u32, &str, bool)]) {
    static REVISION: AtomicU32 = AtomicU32::new(1);
    let members = members
        .iter()
        .map(|(id, name, here)| WireMember {
            id: *id,
            name: name.to_string(),
            here: *here,
        })
        .collect();
    flow.0
        .on_event(ClientEvent::Message(ServerMsg::Roster(WireRoster {
            revision: REVISION.fetch_add(1, Ordering::Relaxed),
            members,
        })));
}

/// 플레이어와 몬스터가 없는 빈 방의 모습입니다. 단계만 정합니다.
pub fn empty_frame(phase: WirePhase) -> Frame {
    Frame {
        tick: 1.0,
        expedition: 1,
        phase,
        wagon: WireWagon {
            hp: 500,
            max_hp: 500,
        },
        stash: 0,
        towers: Vec::new(),
        nodes: Vec::new(),
        piles: Vec::new(),
        chests: Vec::new(),
        ground_weapons: Vec::new(),
        players: Vec::new(),
        monsters: Vec::new(),
        projectiles: Vec::new(),
    }
}
