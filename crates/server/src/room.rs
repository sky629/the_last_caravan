//! 방 하나는 게임 하나(`Sim`)와 그 게임의 자리들입니다.
//! 네트워크의 개념(`LinkId`, 이름, 표식)과 규칙의 개념(`PlayerId`, `Presence`)이 만나는 곳이며, `Sim`은 접속을 알지 못합니다.
//!
//! 「자리에 있는 사람이 없으면 방이 진행되지 않음」은 이 파일이 정합니다. 규칙은 Tick이 오지 않는 것만 겪습니다.
//! 자리의 시간(`Seat::quiet_ticks`)은 멈춘 동안에도 흐르므로, 혼자 하던 사람이 돌아오지 않으면 자리가 끝나고 빈 방이 사라집니다.

use std::collections::BTreeMap;

use caravan_protocol::{
    InputMsg, LinkId, PlayerName, RoomCode, SeatToken, ServerMsg, Snapshot, Welcome, WireMember,
    WireRoster,
};
use caravan_rules::{JoinError, PlayerId, PlayerInputs, Presence, Seed, Sim, Tuning};

use crate::hub::Out;
use crate::seat::Seat;

/// 방의 시간 한도입니다. 허브가 `ServerLimits`의 시간을 서버 Tick으로 바꿔서 넘깁니다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RoomTimes {
    /// 입력 없이 이 Tick 수가 지나면 플레이어를 아무것도 누르지 않은 상태로 움직입니다.
    pub input_stale_ticks: u32,
    /// 입력 없이 이 Tick 수가 지나면 자리를 비운 것으로 봅니다.
    pub away_ticks: u32,
    /// 입력 없이 이 Tick 수가 지나면 자리를 내놓습니다.
    pub hold_ticks: u32,
}

/// 입장한 결과입니다.
pub(crate) struct Entered {
    pub player: PlayerId,
    /// 유지 중이던 자리로 돌아왔으면 참입니다.
    pub returned: bool,
    /// 같은 표식의 이전 접속입니다. 아직 열려 있던 접속이 있으면 허브가 `SeatTakenOver`를 보내고 닫기를 기다립니다.
    pub replaced: Option<LinkId>,
    /// 입장한 접속에 가장 먼저 보낼 입장 안내입니다.
    pub welcome: Welcome,
    /// 입장 안내 다음에 입장한 접속에 보낼 지금의 명단입니다.
    pub roster: WireRoster,
    /// 다른 접속에 보낼 새 명단입니다. 명단이 바뀌지 않았으면 비어 있습니다.
    pub others: Vec<Out>,
}

/// 시간이 다 되어 끝난 자리입니다.
pub(crate) struct Ended {
    pub player: PlayerId,
    /// 끝난 자리에 아직 열린 접속이 있으면 그 접속입니다. 허브가 닫습니다.
    pub link: Option<LinkId>,
}

/// Tick 하나의 결과입니다.
pub(crate) struct RoomTick {
    pub outs: Vec<Out>,
    pub ended: Vec<Ended>,
}

pub(crate) struct Room {
    code: RoomCode,
    sim: Sim,
    seats: BTreeMap<PlayerId, Seat>,
    times: RoomTimes,
    /// 마지막으로 보낸 명단입니다. 명단이 이 값과 달라졌을 때만 다시 보냅니다.
    sent_roster: Vec<WireMember>,
    /// 지금까지 보낸 명단의 수입니다. 명단마다 이 번호를 붙여서 받는 쪽이 더 새로운 명단을 알아봅니다.
    roster_revision: u32,
}

impl Room {
    pub fn new(code: RoomCode, tuning: Tuning, seed: Seed, times: RoomTimes) -> Room {
        Room {
            code,
            sim: Sim::new(tuning, seed),
            seats: BTreeMap::new(),
            times,
            sent_roster: Vec::new(),
            roster_revision: 0,
        }
    }

    /// 표식이 이 방의 자리와 맞으면 그 자리로 돌아가고, 아니면 새 플레이어를 추가합니다. 새 자리는 `fresh`를 표식으로 씁니다.
    /// 유지 중인 자리도 정원에 들어가므로, 돌아오는 입장은 방이 가득 차 있어도 거절되지 않습니다.
    /// 입장한 접속은 입장 안내와 지금의 명단을 받고, 다른 접속은 명단이 바뀌었을 때만 새 명단을 받습니다.
    pub fn enter(
        &mut self,
        link: LinkId,
        name: PlayerName,
        token: Option<SeatToken>,
        fresh: SeatToken,
    ) -> Result<Entered, JoinError> {
        let held = token.and_then(|token| {
            self.seats
                .iter()
                .find_map(|(&player, seat)| (seat.token == token).then_some(player))
        });
        let (player, replaced) = match held {
            Some(player) => {
                // `held`는 이 방의 자리에서 찾은 번호이므로 자리가 항상 있습니다.
                let seat = self
                    .seats
                    .get_mut(&player)
                    .expect("the seat was just found");
                (player, seat.take_over(link, name))
            }
            None => {
                let player = self.sim.add_player()?;
                self.seats.insert(player, Seat::new(name, fresh, link));
                (player, None)
            }
        };
        let others = self.publish_roster(Some(link));
        // 위에서 자리를 넣었거나 찾았고, `add_player`가 이 번호의 플레이어를 넣었으므로 항상 있습니다.
        let seat = &self.seats[&player];
        let me = self.sim.player(player).expect("the player is in the room");
        Ok(Entered {
            player,
            returned: held.is_some(),
            replaced,
            welcome: Welcome::from_sim(&self.sim, me, &self.code, seat.token),
            roster: self.current_roster(),
            others,
        })
    }

    /// `link`가 닫혔습니다. 자리는 남고, 다음 Tick부터 자리를 비운 것으로 보입니다.
    /// 그 접속이 이 자리의 지금 접속이었으면 참입니다. 이미 다른 접속이 이어받은 자리이면 아무것도 하지 않고 거짓입니다.
    pub fn link_closed(&mut self, player: PlayerId, link: LinkId) -> bool {
        self.seats
            .get_mut(&player)
            .is_some_and(|seat| seat.close_link(link))
    }

    /// 플레이어가 스스로 나갔습니다. 자리를 곧바로 내놓고, 원정 중이면 든 자원과 상자에서 얻은 무기가 떨어집니다.
    /// 남은 접속에 보낼 새 명단을 돌려줍니다.
    pub fn leave(&mut self, player: PlayerId) -> Vec<Out> {
        self.seats.remove(&player);
        self.sim.remove_player(player);
        self.publish_roster(None)
    }

    pub fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }

    pub fn member_count(&self) -> usize {
        self.seats.len()
    }

    pub fn monster_count(&self) -> usize {
        self.sim.monsters().count()
    }

    /// 플레이어가 보낸 입력을 받습니다. 자리가 이미 없으면 무시합니다.
    /// 다른 접속이 이어받은 자리의 이전 접속은 허브가 `LinkState::Told`로 바꿔 두므로 이 입력은 방에 오지 않습니다.
    pub fn submit_input(&mut self, player: PlayerId, msg: &InputMsg) {
        if let Some(seat) = self.seats.get_mut(&player) {
            seat.heard();
            seat.input.accept(msg);
        }
    }

    /// 서버 Tick 하나입니다. 순서는 다음과 같습니다.
    /// 1. 모든 자리의 시간이 흐릅니다. 방이 멈춘 Tick에도 흐르므로, 혼자 하던 사람이 돌아오지 않으면 자리가 끝납니다.
    /// 2. 시간이 다 된 자리를 내보냅니다(퇴장과 같습니다).
    /// 3. 자리에 있는지를 규칙에 알리고, 바뀐 명단을 보냅니다.
    /// 4. 자리에 있는 사람이 없으면 여기서 끝냅니다. 게임을 진행하지 않으므로 Tick 번호와 세계가 그대로이고 스냅샷도 보내지 않습니다.
    /// 5. 플레이어마다 입력을 하나씩 정해서 게임을 Tick 하나만큼 진행하고, 같은 스냅샷을 접속이 있는 모든 자리에 보냅니다.
    pub fn tick(&mut self) -> RoomTick {
        for seat in self.seats.values_mut() {
            seat.pass_tick();
        }
        let over: Vec<PlayerId> = self
            .seats
            .iter()
            .filter(|(_, seat)| seat.is_over(self.times.hold_ticks))
            .map(|(&player, _)| player)
            .collect();
        let mut ended = Vec::with_capacity(over.len());
        for player in over {
            if let Some(seat) = self.seats.remove(&player) {
                self.sim.remove_player(player);
                ended.push(Ended {
                    player,
                    link: seat.link,
                });
            }
        }
        self.sync_presence();
        let mut outs = self.publish_roster(None);
        if !self.anyone_here() {
            return RoomTick { outs, ended };
        }
        let stale_ticks = self.times.input_stale_ticks;
        let inputs: PlayerInputs = self
            .seats
            .iter_mut()
            .map(|(&player, seat)| (player, seat.input.for_tick(stale_ticks)))
            .collect();
        self.sim.step(&inputs);
        let snapshot = Snapshot::from_sim(&self.sim);
        outs.extend(self.seats.values().filter_map(|seat| {
            seat.link
                .map(|link| Out::Send(link, ServerMsg::Snapshot(Box::new(snapshot.clone()))))
        }));
        RoomTick { outs, ended }
    }

    fn anyone_here(&self) -> bool {
        self.seats
            .values()
            .any(|seat| seat.presence(self.times.away_ticks) == Presence::Here)
    }

    /// 자리마다 자리에 있는지를 규칙에 알립니다. 규칙이 아는 값과 다른 자리만 알립니다.
    fn sync_presence(&mut self) {
        for (&player, seat) in &self.seats {
            let presence = seat.presence(self.times.away_ticks);
            if self.sim.player(player).map(|p| p.presence) != Some(presence) {
                self.sim.set_presence(player, presence);
            }
        }
    }

    /// 방에 있는 사람입니다. 번호가 작은 플레이어부터입니다.
    fn members(&self) -> Vec<WireMember> {
        self.seats
            .iter()
            .map(|(player, seat)| WireMember {
                id: player.0,
                name: seat.name.as_str().to_string(),
                here: seat.presence(self.times.away_ticks) == Presence::Here,
            })
            .collect()
    }

    /// 지금의 명단입니다. 번호는 마지막으로 보낸 명단의 번호입니다.
    fn current_roster(&self) -> WireRoster {
        WireRoster {
            revision: self.roster_revision,
            members: self.members(),
        }
    }

    /// 명단이 마지막으로 보낸 것과 달라졌으면 접속이 있는 모든 자리에 보낼 일을 돌려주고 보낸 명단을 바꿉니다.
    /// `except`인 접속은 입장 안내 다음에 지금의 명단을 따로 받으므로 빼고 보냅니다.
    fn publish_roster(&mut self, except: Option<LinkId>) -> Vec<Out> {
        let members = self.members();
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
            .filter_map(|seat| seat.link)
            .filter(|&link| Some(link) != except)
            .map(|link| Out::Send(link, ServerMsg::Roster(roster.clone())))
            .collect()
    }
}
