//! 방 하나는 게임 하나(`Sim`)와 그 게임의 자리들입니다.
//! 네트워크의 개념(`LinkId`, 이름)과 규칙의 개념(`PlayerId`)이 만나는 곳이며, `Sim`은 접속과 이름을 알지 못합니다.

use std::collections::BTreeMap;

use caravan_protocol::{
    InputMsg, LinkId, PlayerName, RoomCode, ServerMsg, Snapshot, Welcome, WireMember, WireRoster,
};
use caravan_rules::{JoinError, PlayerId, PlayerInputs, Seed, Sim, Tuning};

use crate::hub::Out;
use crate::seat::Seat;

/// 입장한 결과입니다.
pub(crate) struct Entered {
    pub player: PlayerId,
    /// 입장한 접속에 가장 먼저 보낼 입장 안내입니다.
    pub welcome: Welcome,
    /// 입장 안내 다음에 보낼 명단입니다. 방의 모든 접속에 갑니다.
    pub roster: Vec<Out>,
}

pub(crate) struct Room {
    code: RoomCode,
    sim: Sim,
    seats: BTreeMap<PlayerId, Seat>,
    /// 입력 없이 이 Tick 수가 지나면 플레이어를 아무것도 누르지 않은 상태로 움직입니다.
    input_stale_ticks: u32,
    /// 마지막으로 보낸 명단입니다. 명단이 이 값과 달라졌을 때만 다시 보냅니다.
    sent_roster: Vec<WireMember>,
    /// 지금까지 보낸 명단의 수입니다. 명단마다 이 번호를 붙여서 받는 쪽이 더 새로운 명단을 알아봅니다.
    roster_revision: u32,
}

impl Room {
    pub fn new(code: RoomCode, tuning: Tuning, seed: Seed, input_stale_ticks: u32) -> Room {
        Room {
            code,
            sim: Sim::new(tuning, seed),
            seats: BTreeMap::new(),
            input_stale_ticks,
            sent_roster: Vec::new(),
            roster_revision: 0,
        }
    }

    /// 플레이어를 추가하고, 그 플레이어에게 보낼 입장 안내와 모두에게 보낼 새 명단을 함께 돌려줍니다.
    pub fn enter(&mut self, link: LinkId, name: PlayerName) -> Result<Entered, JoinError> {
        let player = self.sim.add_player()?;
        self.seats.insert(player, Seat::new(name, link));
        // `add_player`가 방금 이 번호의 플레이어를 넣었으므로 항상 있습니다.
        let me = self.sim.player(player).expect("the player was just added");
        let welcome = Welcome::from_sim(&self.sim, me, &self.code);
        Ok(Entered {
            player,
            welcome,
            roster: self.roster_if_changed(),
        })
    }

    /// 플레이어를 방에서 뺍니다. 남은 접속에 보낼 새 명단을 돌려줍니다.
    pub fn remove(&mut self, player: PlayerId) -> Vec<Out> {
        self.seats.remove(&player);
        self.sim.remove_player(player);
        self.roster_if_changed()
    }

    pub fn member_count(&self) -> usize {
        self.seats.len()
    }

    pub fn monster_count(&self) -> usize {
        self.sim.monsters().count()
    }

    /// 플레이어가 보낸 입력을 받습니다. 이미 방을 떠난 플레이어의 입력은 무시합니다.
    pub fn submit_input(&mut self, player: PlayerId, msg: &InputMsg) {
        if let Some(seat) = self.seats.get_mut(&player) {
            seat.input.accept(msg);
        }
    }

    /// 플레이어마다 입력을 하나씩 정해서 게임을 Tick 하나만큼 진행하고, 같은 스냅샷을 모든 접속에 보냅니다.
    pub fn tick(&mut self) -> Vec<Out> {
        let stale_ticks = self.input_stale_ticks;
        let inputs: PlayerInputs = self
            .seats
            .iter_mut()
            .map(|(&player, seat)| (player, seat.input.for_tick(stale_ticks)))
            .collect();
        self.sim.step(&inputs);
        let snapshot = Snapshot::from_sim(&self.sim);
        self.seats
            .values()
            .map(|seat| Out::Send(seat.link, ServerMsg::Snapshot(Box::new(snapshot.clone()))))
            .collect()
    }

    /// 방에 있는 사람입니다. 번호가 작은 플레이어부터입니다.
    fn roster(&self) -> Vec<WireMember> {
        self.seats
            .iter()
            .map(|(player, seat)| WireMember {
                id: player.0,
                name: seat.name.as_str().to_string(),
            })
            .collect()
    }

    /// 명단이 마지막으로 보낸 것과 달라졌으면 모든 접속에 보낼 일을 돌려주고 보낸 명단을 바꿉니다.
    fn roster_if_changed(&mut self) -> Vec<Out> {
        let members = self.roster();
        if members == self.sent_roster {
            return Vec::new();
        }
        self.roster_revision += 1;
        let roster = WireRoster {
            revision: self.roster_revision,
            members: members.clone(),
        };
        self.sent_roster = members;
        self.seats
            .values()
            .map(|seat| Out::Send(seat.link, ServerMsg::Roster(roster.clone())))
            .collect()
    }
}
