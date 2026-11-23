//! 방 하나는 게임 하나(`Sim`)와 그 게임을 보는 접속들입니다.
//! 네트워크의 개념(`LinkId`)과 규칙의 개념(`PlayerId`)이 만나는 곳이며, `Sim`은 접속을 알지 못합니다.

use std::collections::BTreeMap;

use caravan_protocol::{InputMsg, LinkId, ServerMsg, Snapshot, Welcome};
use caravan_rules::{JoinError, PlayerId, PlayerInputs, Sim, Tuning};

use crate::hub::Out;
use crate::input::MemberInput;

/// 방에 들어온 접속 하나입니다.
struct Member {
    link: LinkId,
    input: MemberInput,
}

pub(crate) struct Room {
    sim: Sim,
    members: BTreeMap<PlayerId, Member>,
    /// 입력 없이 이 Tick 수가 지나면 플레이어를 아무것도 누르지 않은 상태로 움직입니다.
    input_stale_ticks: u32,
}

impl Room {
    pub fn new(tuning: Tuning, input_stale_ticks: u32) -> Room {
        Room {
            sim: Sim::new(tuning),
            members: BTreeMap::new(),
            input_stale_ticks,
        }
    }

    /// 플레이어를 추가하고, 그 플레이어에게 보낼 입장 안내를 함께 돌려줍니다.
    pub fn admit(&mut self, link: LinkId) -> Result<(PlayerId, Welcome), JoinError> {
        let player = self.sim.add_player()?;
        self.members.insert(
            player,
            Member {
                link,
                input: MemberInput::new(),
            },
        );
        // `add_player`가 방금 이 번호의 플레이어를 넣었으므로 항상 있습니다.
        let me = self.sim.player(player).expect("the player was just added");
        Ok((player, Welcome::from_sim(&self.sim, me)))
    }

    pub fn remove(&mut self, player: PlayerId) {
        self.members.remove(&player);
        self.sim.remove_player(player);
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    pub fn monster_count(&self) -> usize {
        self.sim.monsters().count()
    }

    /// 플레이어가 보낸 입력을 받습니다. 이미 방을 떠난 플레이어의 입력은 무시합니다.
    pub fn submit_input(&mut self, player: PlayerId, msg: &InputMsg) {
        if let Some(member) = self.members.get_mut(&player) {
            member.input.accept(msg);
        }
    }

    /// 플레이어마다 입력을 하나씩 정해서 게임을 Tick 하나만큼 진행하고, 같은 스냅샷을 모든 접속에 보냅니다.
    pub fn tick(&mut self) -> Vec<Out> {
        let stale_ticks = self.input_stale_ticks;
        let inputs: PlayerInputs = self
            .members
            .iter_mut()
            .map(|(&player, member)| (player, member.input.for_tick(stale_ticks)))
            .collect();
        self.sim.step(&inputs);
        let snapshot = Snapshot::from_sim(&self.sim);
        self.members
            .values()
            .map(|member| Out::Send(member.link, ServerMsg::Snapshot(snapshot.clone())))
            .collect()
    }
}
