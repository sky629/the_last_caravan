//! Bevy 시스템 테스트가 함께 쓰는 도구입니다. 화면과 GPU 없이 입장을 마친 화면 흐름을 만듭니다.

use caravan_client_core::{Flow, LaunchOptions, Memory};
use caravan_protocol::{ClientEvent, RoomCode, ServerMsg, Welcome};
use caravan_rules::{Seed, Sim, Tuning};

use crate::net::ClientFlow;

/// 테스트의 플레이어 번호입니다.
pub const YOU: u32 = 1;

/// 기본 수치의 방 `test`에 이름 `KANG`, 플레이어 번호 `YOU`로 입장을 마친 화면 흐름입니다.
pub fn joined_flow() -> ClientFlow {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    let first = sim.add_player().unwrap();
    let room = RoomCode::parse("test").unwrap();
    let mut welcome = Welcome::from_sim(&sim, sim.player(first).unwrap(), &room);
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
    flow.on_event(ClientEvent::Message(ServerMsg::Welcome(welcome)));
    ClientFlow(flow)
}
