//! 허브는 모든 접속과 모든 방을 소유합니다.
//! 접속 방식을 직접 호출하지 않고, 해야 할 일을 `Out` 목록으로 돌려줍니다. 그래서 동기 코드 하나가 모든 상태를 소유하고 잠금이 필요 없습니다.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, LinkId, PROTOCOL_VERSION, RejectReason, RoomCode, ServerEvent, ServerMsg,
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

enum LinkState {
    /// 접속했지만 아직 `Join`을 보내지 않았습니다. `deadline`까지 보내지 않으면 닫습니다.
    AwaitingJoin {
        deadline: Instant,
    },
    InRoom {
        room: RoomCode,
        player: PlayerId,
    },
    /// 서버가 닫기로 한 접속입니다. 닫히기 전에 도착하는 메시지는 무시하고 `Disconnected`를 기다립니다.
    Closing,
}

pub(crate) struct Hub {
    tuning: Tuning,
    join_deadline: Duration,
    links: BTreeMap<LinkId, LinkState>,
    rooms: BTreeMap<RoomCode, Room>,
}

impl Hub {
    pub fn new(tuning: Tuning, join_deadline: Duration) -> Hub {
        Hub {
            tuning,
            join_deadline,
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
            ServerEvent::Message(link, msg) => self.handle_message(link, msg),
            ServerEvent::Disconnected(link) => self.handle_leave(link),
        }
    }

    /// `Join`을 보내야 하는 시간이 지난 접속을 닫습니다.
    pub fn close_late_joiners(&mut self, now: Instant) -> Vec<Out> {
        let mut outs = Vec::new();
        for (&link, state) in self.links.iter_mut() {
            if let LinkState::AwaitingJoin { deadline } = *state
                && now >= deadline
            {
                *state = LinkState::Closing;
                outs.push(Out::Close(link));
            }
        }
        outs
    }

    /// 모든 방을 Tick 하나만큼 진행합니다. 방은 서로의 플레이어와 Tick 번호를 알지 못합니다.
    pub fn tick(&mut self) -> Vec<Out> {
        self.rooms
            .values_mut()
            .flat_map(|room| room.tick())
            .collect()
    }

    fn handle_message(&mut self, link: LinkId, msg: ClientMsg) -> Vec<Out> {
        match (self.links.get(&link), msg) {
            (Some(LinkState::AwaitingJoin { .. }), ClientMsg::Join { version, room }) => {
                match self.admit(link, version, &room) {
                    Ok((code, player, members)) => vec![
                        Out::Send(
                            link,
                            ServerMsg::Welcome {
                                you: player.0,
                                tick_hz: self.tuning.tick_hz,
                            },
                        ),
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
            // 규칙이 아직 플레이어의 입력을 사용하지 않으므로 받은 입력은 버립니다.
            (Some(LinkState::InRoom { .. }), ClientMsg::Input { .. }) => Vec::new(),
            // 입장하기 전의 입력은 약속 위반입니다.
            (Some(LinkState::AwaitingJoin { .. }), ClientMsg::Input { .. }) => {
                self.links.insert(link, LinkState::Closing);
                vec![Out::Close(link)]
            }
            // 두 번째 `Join`도 약속 위반입니다. 플레이어는 `Disconnected`를 받을 때 방에서 제거합니다.
            (Some(LinkState::InRoom { .. }), ClientMsg::Join { .. }) => vec![Out::Close(link)],
            (Some(LinkState::Closing) | None, _) => Vec::new(),
        }
    }

    /// 전송 형식의 문자열이 `RoomCode`가 되는 경계입니다. 처음 입장하는 사람이 방을 만듭니다.
    fn admit(
        &mut self,
        link: LinkId,
        version: u32,
        raw_room: &str,
    ) -> Result<(RoomCode, PlayerId, usize), RejectReason> {
        if version != PROTOCOL_VERSION {
            return Err(RejectReason::VersionMismatch {
                server: PROTOCOL_VERSION,
            });
        }
        let code = RoomCode::parse(raw_room).map_err(|_| RejectReason::BadRoomCode)?;
        let tuning = &self.tuning;
        let room = self
            .rooms
            .entry(code.clone())
            .or_insert_with(|| Room::new(tuning.clone()));
        let player = room
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
            },
        );
        Ok((code, player, members))
    }

    /// 끊긴 접속의 플레이어를 즉시 제거합니다. 마지막 플레이어가 나가면 방도 없앱니다.
    /// 모르는 접속의 퇴장은 무시하므로 같은 퇴장을 두 번 받아도 안전합니다.
    fn handle_leave(&mut self, link: LinkId) -> Vec<Out> {
        let Some(LinkState::InRoom { room: code, player }) = self.links.remove(&link) else {
            return Vec::new();
        };
        let Some(room) = self.rooms.get_mut(&code) else {
            return Vec::new();
        };
        room.remove(player);
        let members = room.member_count();
        if members == 0 {
            self.rooms.remove(&code);
        }
        vec![Out::Notice(Notice::Left {
            room: code,
            player,
            members,
        })]
    }
}
