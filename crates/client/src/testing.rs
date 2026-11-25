//! Bevy 시스템 테스트가 함께 쓰는 도구입니다. 화면과 GPU 없이 입장을 마친 세션을 만듭니다.

use caravan_client_core::{DEFAULT_SERVER, Session};
use caravan_protocol::{ClientEvent, RoomCode, ServerMsg, Welcome};
use caravan_rules::{Seed, Sim, Tuning};

use crate::net::ClientSession;

/// 테스트의 플레이어 번호입니다.
pub const YOU: u32 = 1;

/// 기본 수치의 방에 `YOU`번 플레이어로 입장을 마친 세션입니다.
pub fn joined_session() -> ClientSession {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    let first = sim.add_player().unwrap();
    let mut welcome = Welcome::from_sim(&sim, sim.player(first).unwrap());
    welcome.you = YOU;
    let mut session = Session::new(RoomCode::parse("test").unwrap(), DEFAULT_SERVER.to_string());
    session.on_event(ClientEvent::Connected);
    session.on_event(ClientEvent::Message(ServerMsg::Welcome(welcome)));
    ClientSession(session)
}
