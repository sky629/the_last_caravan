//! 허브는 모든 접속과 모든 방을 소유합니다.
//! 접속 방식을 직접 호출하지 않고, 해야 할 일을 `Out` 목록으로 돌려줍니다. 그래서 동기 코드 하나가 모든 상태를 소유하고 잠금이 필요 없습니다.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, LinkId, PROTOCOL_VERSION, RejectReason, RoomCode, ServerEvent, ServerLimits,
    ServerMsg, Welcome,
};
use caravan_rules::{JoinError, PlayerId, Tuning};

use crate::Notice;
use crate::room::Room;

/// 허브가 요청하는 일입니다. `GameServer::apply`만 이 값을 실행합니다.
pub(crate) enum Out {
    Send(LinkId, ServerMsg),
    /// 앞서 요청한 `Send`가 모두 전달된 뒤 접속을 닫습니다.
    Close(LinkId),
    Notice(Notice),
}

/// 접속 하나의 상태입니다. 열려 있는 접속은 모두 `deadline`까지 메시지를 보내야 하고, 보내지 않으면 서버가 닫습니다.
enum LinkState {
    /// 접속했지만 아직 `Join`을 보내지 않았습니다.
    AwaitingJoin { deadline: Instant },
    /// 방에 들어왔습니다. 메시지를 받을 때마다 `deadline`이 뒤로 밀립니다.
    InRoom {
        room: RoomCode,
        player: PlayerId,
        deadline: Instant,
    },
    /// 서버가 닫기로 한 접속입니다. 닫히기 전에 도착하는 메시지는 무시하고 `Disconnected`를 기다립니다.
    Closing,
}

pub(crate) struct Hub {
    tuning: Tuning,
    join_deadline: Duration,
    idle_timeout: Duration,
    input_stale_ticks: u32,
    links: BTreeMap<LinkId, LinkState>,
    rooms: BTreeMap<RoomCode, Room>,
}

impl Hub {
    pub fn new(tuning: Tuning, limits: ServerLimits) -> Hub {
        Hub {
            tuning,
            join_deadline: limits.join_deadline,
            idle_timeout: limits.idle_timeout,
            input_stale_ticks: limits.input_stale_ticks,
            links: BTreeMap::new(),
            rooms: BTreeMap::new(),
        }
    }

    pub fn handle_event(&mut self, event: ServerEvent, now: Instant) -> Vec<Out> {
        match event {
            ServerEvent::Connected(link) => {
                let deadline = now + self.join_deadline;
                self.links
                    .insert(link, LinkState::AwaitingJoin { deadline });
                Vec::new()
            }
            ServerEvent::Message(link, msg) => self.handle_message(link, msg, now),
            ServerEvent::Disconnected(link) => match self.links.remove(&link) {
                Some(LinkState::InRoom { room, player, .. }) => {
                    vec![self.remove_player(room, player)]
                }
                // 모르는 접속의 퇴장은 무시하므로 같은 퇴장을 두 번 받아도 안전합니다.
                _ => Vec::new(),
            },
        }
    }

    /// `deadline`이 지나도록 메시지를 보내지 않은 접속을 닫습니다.
    /// 입장하지 않은 접속과, 입장한 뒤 멈춘 접속이 여기에 해당합니다.
    pub fn close_silent_links(&mut self, now: Instant) -> Vec<Out> {
        let silent: Vec<LinkId> = self
            .links
            .iter()
            .filter(|(_, state)| match state {
                LinkState::AwaitingJoin { deadline } | LinkState::InRoom { deadline, .. } => {
                    now >= *deadline
                }
                LinkState::Closing => false,
            })
            .map(|(&link, _)| link)
            .collect();
        silent
            .into_iter()
            .flat_map(|link| self.close_link(link))
            .collect()
    }

    /// 방의 수, 모든 방의 플레이어 수, 모든 방의 몬스터 수입니다.
    pub fn size(&self) -> (usize, usize, usize) {
        let members = self.rooms.values().map(Room::member_count).sum();
        let monsters = self.rooms.values().map(Room::monster_count).sum();
        (self.rooms.len(), members, monsters)
    }

    /// 모든 방을 Tick 하나만큼 진행합니다. 방은 서로의 플레이어와 Tick 번호를 알지 못합니다.
    pub fn tick(&mut self) -> Vec<Out> {
        self.rooms
            .values_mut()
            .flat_map(|room| room.tick())
            .collect()
    }

    fn handle_message(&mut self, link: LinkId, msg: ClientMsg, now: Instant) -> Vec<Out> {
        match (self.links.get_mut(&link), msg) {
            (Some(LinkState::AwaitingJoin { .. }), ClientMsg::Join { version, room }) => {
                self.handle_join(link, version, &room, now)
            }
            // 입력은 접속이 살아 있다는 표시이기도 하므로, 버려지는 입력도 기한을 미룹니다.
            (
                Some(LinkState::InRoom {
                    room,
                    player,
                    deadline,
                }),
                ClientMsg::Input(input),
            ) => {
                *deadline = now + self.idle_timeout;
                if let Some(room) = self.rooms.get_mut(room) {
                    room.submit_input(*player, &input);
                }
                Vec::new()
            }
            // 입장하기 전의 입력과 두 번째 `Join`은 약속 위반입니다.
            (Some(LinkState::AwaitingJoin { .. }), ClientMsg::Input(_))
            | (Some(LinkState::InRoom { .. }), ClientMsg::Join { .. }) => self.close_link(link),
            (Some(LinkState::Closing) | None, _) => Vec::new(),
        }
    }

    fn handle_join(
        &mut self,
        link: LinkId,
        version: u32,
        raw_room: &str,
        now: Instant,
    ) -> Vec<Out> {
        match self.admit(link, version, raw_room, now) {
            Ok((code, player, members, welcome)) => vec![
                Out::Send(link, ServerMsg::Welcome(welcome)),
                Out::Notice(Notice::Joined {
                    room: code,
                    player,
                    members,
                }),
            ],
            Err(reason) => {
                self.links.insert(link, LinkState::Closing);
                vec![
                    Out::Send(link, ServerMsg::JoinRejected { reason }),
                    Out::Close(link),
                    Out::Notice(Notice::Rejected { reason }),
                ]
            }
        }
    }

    /// 전송 형식의 문자열이 `RoomCode`가 되는 경계입니다. 처음 입장하는 사람이 방을 만듭니다.
    fn admit(
        &mut self,
        link: LinkId,
        version: u32,
        raw_room: &str,
        now: Instant,
    ) -> Result<(RoomCode, PlayerId, usize, Welcome), RejectReason> {
        if version != PROTOCOL_VERSION {
            return Err(RejectReason::VersionMismatch {
                server: PROTOCOL_VERSION,
            });
        }
        let code = RoomCode::parse(raw_room).map_err(|_| RejectReason::BadRoomCode)?;
        let (tuning, stale_ticks) = (&self.tuning, self.input_stale_ticks);
        let room = self
            .rooms
            .entry(code.clone())
            .or_insert_with(|| Room::new(tuning.clone(), stale_ticks));
        let (player, welcome) = room
            .admit(link)
            .map_err(|JoinError::RoomFull { capacity }| RejectReason::RoomFull {
                capacity: capacity as u32,
            })?;
        let members = room.member_count();
        self.links.insert(
            link,
            LinkState::InRoom {
                room: code.clone(),
                player,
                deadline: now + self.idle_timeout,
            },
        );
        Ok((code, player, members, welcome))
    }

    /// 서버가 접속을 닫습니다. 방에 있던 플레이어는 `Disconnected`를 기다리지 않고 바로 제거하므로,
    /// 다음 스냅샷부터 다른 플레이어의 화면에서 사라집니다.
    fn close_link(&mut self, link: LinkId) -> Vec<Out> {
        let mut outs = Vec::new();
        if let Some(LinkState::InRoom { room, player, .. }) =
            self.links.insert(link, LinkState::Closing)
        {
            outs.push(self.remove_player(room, player));
        }
        outs.push(Out::Close(link));
        outs
    }

    /// 플레이어를 방에서 제거합니다. 마지막 플레이어가 나가면 방도 없앱니다.
    fn remove_player(&mut self, code: RoomCode, player: PlayerId) -> Out {
        let mut members = 0;
        if let Some(room) = self.rooms.get_mut(&code) {
            room.remove(player);
            members = room.member_count();
            if members == 0 {
                self.rooms.remove(&code);
            }
        }
        Out::Notice(Notice::Left {
            room: code,
            player,
            members,
        })
    }
}
