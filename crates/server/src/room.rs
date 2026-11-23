//! 방 하나는 게임 하나(`Sim`)와 그 게임을 보는 접속들입니다.
//! 네트워크의 개념(`LinkId`)과 규칙의 개념(`PlayerId`)이 만나는 곳이며, `Sim`은 접속을 알지 못합니다.

use std::collections::BTreeMap;

use caravan_protocol::{LinkId, ServerMsg, Snapshot};
use caravan_rules::{JoinError, PlayerId, PlayerInputs, Sim, Tuning};

use crate::hub::Out;

pub(crate) struct Room {
    sim: Sim,
    members: BTreeMap<PlayerId, LinkId>,
}

impl Room {
    pub fn new(tuning: Tuning) -> Room {
        Room {
            sim: Sim::new(tuning),
            members: BTreeMap::new(),
        }
    }

    pub fn admit(&mut self, link: LinkId) -> Result<PlayerId, JoinError> {
        let player = self.sim.add_player()?;
        self.members.insert(player, link);
        Ok(player)
    }

    pub fn remove(&mut self, player: PlayerId) {
        self.members.remove(&player);
        self.sim.remove_player(player);
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// 게임을 Tick 하나만큼 진행하고, 같은 스냅샷을 모든 접속에 보냅니다.
    pub fn tick(&mut self) -> Vec<Out> {
        self.sim.step(&PlayerInputs::new());
        let snapshot = Snapshot::from_sim(&self.sim);
        self.members
            .values()
            .map(|&link| Out::Send(link, ServerMsg::Snapshot(snapshot.clone())))
            .collect()
    }
}
