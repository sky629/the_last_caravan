//! 허브는 모든 접속과 모든 방을 소유합니다.
//! 접속 방식을 직접 호출하지 않고, 해야 할 일을 `Out` 목록으로 돌려줍니다. 그래서 동기 코드 하나가 모든 상태를 소유하고 잠금이 필요 없습니다.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, LinkId, PROTOCOL_VERSION, PlayerName, RejectReason, RoomCode, ServerEvent,
    ServerLimits, ServerMsg, Welcome,
};
use caravan_rules::{JoinError, PlayerId, Seed, Tuning};

use crate::mint::Mint;
use crate::room::Room;
use crate::{Notice, RoomSeeds, ServerSecret, ServerStats};

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

/// 입장에 성공한 결과입니다.
struct Admitted {
    code: RoomCode,
    player: PlayerId,
    members: usize,
    welcome: Welcome,
    /// 입장 안내 다음에 보낼 명단입니다.
    roster: Vec<Out>,
    /// 이 입장이 방을 만들었으면 그 방의 씨앗입니다.
    created_with: Option<Seed>,
}

pub(crate) struct Hub {
    tuning: Tuning,
    join_deadline: Duration,
    idle_timeout: Duration,
    input_stale_ticks: u32,
    seeds: RoomSeeds,
    /// 방 코드를 비우고 입장한 사람의 방 코드를 만듭니다.
    mint: Mint,
    /// 지금까지 만든 방의 수입니다. 방이 없어졌다가 같은 코드로 다시 만들어져도 다른 방이므로 센 값이 이어집니다.
    rooms_created: u32,
    links: BTreeMap<LinkId, LinkState>,
    rooms: BTreeMap<RoomCode, Room>,
}

impl Hub {
    pub fn new(
        tuning: Tuning,
        limits: ServerLimits,
        seeds: RoomSeeds,
        secret: ServerSecret,
    ) -> Hub {
        Hub {
            tuning,
            join_deadline: limits.join_deadline,
            idle_timeout: limits.idle_timeout,
            input_stale_ticks: limits.input_stale_ticks,
            seeds,
            mint: Mint::new(secret),
            rooms_created: 0,
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
            ServerEvent::LimitReached(link, reason) => {
                vec![Out::Notice(Notice::LinkClosed { link, reason })]
            }
            ServerEvent::Disconnected(link) => match self.links.remove(&link) {
                Some(LinkState::InRoom { room, player, .. }) => self.remove_player(room, player),
                // 모르는 접속의 퇴장은 무시하므로 같은 퇴장을 두 번 받아도 안전합니다.
                _ => Vec::new(),
            },
        }
    }

    /// `deadline`이 지나도록 메시지를 보내지 않은 접속을 닫습니다.
    /// 입장하지 않은 접속과, 입장한 뒤 멈춘 접속이 여기에 해당합니다.
    pub fn close_silent_links(&mut self, now: Instant) -> Vec<Out> {
        let silent: Vec<(LinkId, String)> = self
            .links
            .iter()
            .filter_map(|(&link, state)| match state {
                LinkState::AwaitingJoin { deadline } if now >= *deadline => Some((
                    link,
                    format!("no Join within {:?} of connecting", self.join_deadline),
                )),
                LinkState::InRoom { deadline, .. } if now >= *deadline => {
                    Some((link, format!("no message for {:?}", self.idle_timeout)))
                }
                _ => None,
            })
            .collect();
        silent
            .into_iter()
            .flat_map(|(link, reason)| self.close_link(link, reason))
            .collect()
    }

    /// 방, 플레이어, 몬스터의 수를 셉니다. `ticks`는 서버가 실행한 Tick의 수이며 허브는 알지 못하므로 호출하는 쪽이 넘깁니다.
    pub fn stats(&self, ticks: u64) -> ServerStats {
        ServerStats {
            rooms: self.rooms.len(),
            members: self.rooms.values().map(Room::member_count).sum(),
            monsters: self.rooms.values().map(Room::monster_count).sum(),
            ticks,
        }
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
            (
                Some(LinkState::AwaitingJoin { .. }),
                ClientMsg::Join {
                    version,
                    room,
                    name,
                },
            ) => self.handle_join(link, version, room.as_deref(), &name, now),
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
            (Some(LinkState::AwaitingJoin { .. }), ClientMsg::Input(_)) => {
                self.close_link(link, "sent an input before Join".to_string())
            }
            (Some(LinkState::InRoom { .. }), ClientMsg::Join { .. }) => {
                self.close_link(link, "sent Join again after joining".to_string())
            }
            (Some(LinkState::Closing) | None, _) => Vec::new(),
        }
    }

    fn handle_join(
        &mut self,
        link: LinkId,
        version: u32,
        raw_room: Option<&str>,
        raw_name: &str,
        now: Instant,
    ) -> Vec<Out> {
        match self.admit(link, version, raw_room, raw_name, now) {
            Ok(admitted) => {
                let mut outs = vec![Out::Send(link, ServerMsg::Welcome(admitted.welcome))];
                outs.extend(admitted.roster);
                if let Some(seed) = admitted.created_with {
                    outs.push(Out::Notice(Notice::RoomCreated {
                        room: admitted.code.clone(),
                        seed,
                    }));
                }
                outs.push(Out::Notice(Notice::Joined {
                    room: admitted.code,
                    player: admitted.player,
                    members: admitted.members,
                }));
                outs
            }
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

    /// 전송 형식의 문자열이 `PlayerName`과 `RoomCode`가 되는 경계입니다.
    /// 방 코드가 없으면 서버가 새 코드를 만들고, 방 코드가 있는데 그 방이 없으면 그 코드로 방을 만듭니다.
    fn admit(
        &mut self,
        link: LinkId,
        version: u32,
        raw_room: Option<&str>,
        raw_name: &str,
        now: Instant,
    ) -> Result<Admitted, RejectReason> {
        if version != PROTOCOL_VERSION {
            return Err(RejectReason::VersionMismatch {
                server: PROTOCOL_VERSION,
            });
        }
        let name = PlayerName::parse(raw_name).map_err(|_| RejectReason::BadName)?;
        let code = match raw_room {
            Some(raw) => RoomCode::parse(raw).map_err(|_| RejectReason::BadRoomCode)?,
            None => self.mint.room_code(&self.rooms),
        };
        let mut created_with = None;
        let room = match self.rooms.entry(code.clone()) {
            Entry::Occupied(occupied) => occupied.into_mut(),
            Entry::Vacant(vacant) => {
                self.rooms_created += 1;
                let seed = self.seeds.for_room(self.rooms_created);
                created_with = Some(seed);
                vacant.insert(Room::new(
                    code.clone(),
                    self.tuning.clone(),
                    seed,
                    self.input_stale_ticks,
                ))
            }
        };
        let entered = room
            .enter(link, name)
            .map_err(|JoinError::RoomFull { capacity }| RejectReason::RoomFull {
                capacity: capacity as u32,
            })?;
        let members = room.member_count();
        self.links.insert(
            link,
            LinkState::InRoom {
                room: code.clone(),
                player: entered.player,
                deadline: now + self.idle_timeout,
            },
        );
        Ok(Admitted {
            code,
            player: entered.player,
            members,
            welcome: entered.welcome,
            roster: entered.roster,
            created_with,
        })
    }

    /// 서버가 `reason` 때문에 접속을 닫습니다. 방에 있던 플레이어는 `Disconnected`를 기다리지 않고 바로 제거하므로,
    /// 다음 스냅샷부터 다른 플레이어의 화면에서 사라집니다.
    fn close_link(&mut self, link: LinkId, reason: String) -> Vec<Out> {
        let mut outs = vec![Out::Notice(Notice::LinkClosed { link, reason })];
        if let Some(LinkState::InRoom { room, player, .. }) =
            self.links.insert(link, LinkState::Closing)
        {
            outs.extend(self.remove_player(room, player));
        }
        outs.push(Out::Close(link));
        outs
    }

    /// 플레이어를 방에서 제거합니다. 남은 사람에게 새 명단을 보내고, 마지막 플레이어가 나가면 방도 없앱니다.
    fn remove_player(&mut self, code: RoomCode, player: PlayerId) -> Vec<Out> {
        let mut members = 0;
        let mut room_removed = false;
        let mut outs = Vec::new();
        if let Some(room) = self.rooms.get_mut(&code) {
            outs.extend(room.remove(player));
            members = room.member_count();
            if members == 0 {
                self.rooms.remove(&code);
                room_removed = true;
            }
        }
        outs.push(Out::Notice(Notice::Left {
            room: code.clone(),
            player,
            members,
        }));
        if room_removed {
            outs.push(Out::Notice(Notice::RoomRemoved { room: code }));
        }
        outs
    }
}
