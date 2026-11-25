//! 허브는 모든 접속과 모든 방을 소유합니다. 자리(이름, 표식, 유지)는 방이 소유하고, 허브는 접속이 어느 방의 어느 플레이어인지만 압니다.
//! 접속 방식을 직접 호출하지 않고, 해야 할 일을 `Out` 목록으로 돌려줍니다. 그래서 동기 코드 하나가 모든 상태를 소유하고 잠금이 필요 없습니다.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, LinkId, PROTOCOL_VERSION, PlayerName, RejectReason, RoomCode, SeatToken,
    ServerEvent, ServerLimits, ServerMsg, Welcome, WireRoster,
};
use caravan_rules::{JoinError, PlayerId, Seed, Tuning};

use crate::mint::Mint;
use crate::room::{Room, RoomTimes};
use crate::{Notice, RandomSource, RoomSeeds, ServerStats};

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
    /// 서버가 이유를 보낸 접속입니다(입장 거절, 같은 표식의 새 접속에 자리를 내줌). 서버는 이유를 보낸 뒤 접속을 닫지 않고,
    /// 클라이언트가 이유를 읽고 닫기를 `deadline`까지 기다립니다. 서버가 이유를 보내자마자 닫으면,
    /// 브라우저는 닫힘을 그 메시지보다 먼저 알릴 수 있어서 이유가 화면에 나오지 않습니다. 닫히기 전에 도착하는 메시지는 무시합니다.
    Told { deadline: Instant },
    /// 서버가 닫기로 한 접속입니다. 닫히기 전에 도착하는 메시지는 무시하고 `Disconnected`를 기다립니다.
    Closing,
}

/// 서버가 접속을 닫을 때 그 접속의 자리를 어떻게 하는지입니다. 접속이 자리를 쓰고 있지 않으면 어느 쪽이든 같습니다.
#[derive(Clone, Copy)]
enum SeatFate {
    /// 입력이 오지 않는 접속입니다. 끊긴 접속처럼 자리를 유지하고, 사람이 돌아올 수 있습니다.
    Hold,
    /// 약속을 어기거나 한도를 넘은 접속입니다. 자리를 곧바로 내놓습니다.
    GiveUp,
}

/// 입장에 성공한 결과입니다.
struct Admitted {
    code: RoomCode,
    player: PlayerId,
    returned: bool,
    /// 같은 표식으로 자리를 이어받아서 밀려난 이전 접속입니다.
    replaced: Option<LinkId>,
    members: usize,
    welcome: Welcome,
    /// 입장 안내 다음에 입장한 접속에 보낼 지금의 명단입니다.
    roster: WireRoster,
    /// 다른 접속에 보낼 새 명단입니다.
    others: Vec<Out>,
    /// 이 입장이 방을 만들었으면 그 방의 씨앗입니다.
    created_with: Option<Seed>,
}

pub(crate) struct Hub {
    tuning: Tuning,
    join_deadline: Duration,
    idle_timeout: Duration,
    /// 한꺼번에 둘 수 있는 방의 수입니다.
    max_rooms: usize,
    times: RoomTimes,
    seeds: RoomSeeds,
    /// 방 코드를 비우고 입장한 사람의 방 코드와, 새 자리의 표식을 만듭니다.
    mint: Mint,
    /// 지금까지 만든 방의 수입니다. 방이 없어졌다가 같은 코드로 다시 만들어져도 다른 방이므로 센 값이 이어집니다.
    rooms_created: u32,
    links: BTreeMap<LinkId, LinkState>,
    rooms: BTreeMap<RoomCode, Room>,
}

/// 시간을 서버 Tick 수로 바꿉니다. 모자라지 않게 올림합니다.
pub(crate) fn ticks_of(time: Duration, tick_hz: u32) -> u32 {
    (time.as_secs_f64() * f64::from(tick_hz)).ceil() as u32
}

impl Hub {
    pub fn new(
        tuning: Tuning,
        limits: ServerLimits,
        seeds: RoomSeeds,
        random: Box<dyn RandomSource>,
    ) -> Hub {
        let times = RoomTimes {
            input_stale_ticks: limits.input_stale_ticks,
            away_ticks: ticks_of(limits.away_after, tuning.tick_hz),
            camp_idle_ticks: ticks_of(limits.camp_idle, tuning.tick_hz),
            hold_ticks: ticks_of(limits.seat_hold, tuning.tick_hz),
        };
        Hub {
            tuning,
            join_deadline: limits.join_deadline,
            idle_timeout: limits.idle_timeout,
            max_rooms: limits.max_rooms,
            times,
            seeds,
            mint: Mint::new(random),
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
            // 접속 방식이 한도를 넘은 접속을 닫았습니다. 이 접속의 `Disconnected`가 바로 이어서 오지만, 자리는 지금 내놓습니다.
            ServerEvent::LimitReached(link, reason) => {
                let mut outs = vec![Out::Notice(Notice::LinkClosed { link, reason })];
                let state = self
                    .links
                    .get_mut(&link)
                    .map(|state| std::mem::replace(state, LinkState::Closing));
                if let Some(LinkState::InRoom { room, player, .. }) = state {
                    outs.extend(self.give_up_seat(room, player));
                }
                outs
            }
            ServerEvent::Disconnected(link) => match self.links.remove(&link) {
                Some(LinkState::InRoom { room, player, .. }) => self.hold_seat(room, player, link),
                // 모르는 접속의 퇴장은 무시하므로 같은 퇴장을 두 번 받아도 안전합니다.
                _ => Vec::new(),
            },
        }
    }

    /// `deadline`이 지나도록 메시지를 보내지 않은 접속을 닫습니다.
    /// 입장하지 않은 접속, 입장한 뒤 멈춘 접속, 이유를 보낸 뒤에도 닫지 않은 접속이 여기에 해당합니다.
    pub fn close_silent_links(&mut self, now: Instant) -> Vec<Out> {
        let silent: Vec<(LinkId, String, SeatFate)> = self
            .links
            .iter()
            .filter_map(|(&link, state)| match state {
                LinkState::AwaitingJoin { deadline } if now >= *deadline => Some((
                    link,
                    format!("no Join within {:?} of connecting", self.join_deadline),
                    SeatFate::GiveUp,
                )),
                // 입력이 오지 않는 접속의 자리는 유지합니다. 창이 멈춘 사람이 돌아올 수 있습니다.
                LinkState::InRoom { deadline, .. } if now >= *deadline => Some((
                    link,
                    format!("no message for {:?}", self.idle_timeout),
                    SeatFate::Hold,
                )),
                LinkState::Told { deadline } if now >= *deadline => Some((
                    link,
                    format!(
                        "still open {:?} after it was told why it cannot stay",
                        self.join_deadline
                    ),
                    SeatFate::GiveUp,
                )),
                _ => None,
            })
            .collect();
        silent
            .into_iter()
            .flat_map(|(link, reason, fate)| self.close_link(link, reason, fate))
            .collect()
    }

    /// 방, 플레이어, 몬스터의 수를 셉니다. 플레이어는 유지 중인 자리를 포함합니다.
    /// `ticks`는 서버가 실행한 Tick의 수이며 허브는 알지 못하므로 호출하는 쪽이 넘깁니다.
    pub fn stats(&self, ticks: u64) -> ServerStats {
        ServerStats {
            rooms: self.rooms.len(),
            members: self.rooms.values().map(Room::member_count).sum(),
            monsters: self.rooms.values().map(Room::monster_count).sum(),
            ticks,
        }
    }

    /// 모든 방을 Tick 하나만큼 진행합니다. 방은 서로의 플레이어와 Tick 번호를 알지 못합니다.
    /// 시간이 다 된 자리를 알리고, 그 자리에 아직 열린 접속이 있으면 닫고, 자리가 하나도 남지 않은 방을 없앱니다.
    pub fn tick(&mut self) -> Vec<Out> {
        let mut outs = Vec::new();
        let mut ended: Vec<(RoomCode, PlayerId, Option<LinkId>, usize)> = Vec::new();
        for (code, room) in &mut self.rooms {
            let tick = room.tick();
            outs.extend(tick.outs);
            let mut remaining = room.member_count() + tick.ended.len();
            for seat in tick.ended {
                remaining -= 1;
                ended.push((code.clone(), seat.player, seat.link, remaining));
            }
        }
        for (room, player, link, members) in ended {
            if let Some(link) = link {
                self.links.insert(link, LinkState::Closing);
                outs.push(Out::Close(link));
            }
            outs.push(Out::Notice(Notice::Left {
                room,
                player,
                members,
            }));
        }
        outs.extend(self.remove_empty_rooms());
        outs
    }

    fn handle_message(&mut self, link: LinkId, msg: ClientMsg, now: Instant) -> Vec<Out> {
        match (self.links.get_mut(&link), msg) {
            (
                Some(LinkState::AwaitingJoin { .. }),
                ClientMsg::Join {
                    version,
                    room,
                    name,
                    token,
                },
            ) => self.handle_join(link, version, room.as_deref(), &name, token, now),
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
            (Some(LinkState::InRoom { .. }), ClientMsg::Leave) => self.leave(link),
            // 입장하기 전의 입력과 나가기, 그리고 두 번째 `Join`은 약속 위반입니다.
            (Some(LinkState::AwaitingJoin { .. }), ClientMsg::Input(_)) => self.close_link(
                link,
                "sent an input before Join".to_string(),
                SeatFate::GiveUp,
            ),
            (Some(LinkState::AwaitingJoin { .. }), ClientMsg::Leave) => {
                self.close_link(link, "sent Leave before Join".to_string(), SeatFate::GiveUp)
            }
            (Some(LinkState::InRoom { .. }), ClientMsg::Join { .. }) => self.close_link(
                link,
                "sent Join again after joining".to_string(),
                SeatFate::GiveUp,
            ),
            (Some(LinkState::Told { .. } | LinkState::Closing) | None, _) => Vec::new(),
        }
    }

    fn handle_join(
        &mut self,
        link: LinkId,
        version: u32,
        raw_room: Option<&str>,
        raw_name: &str,
        token: Option<SeatToken>,
        now: Instant,
    ) -> Vec<Out> {
        match self.admit(link, version, raw_room, raw_name, token, now) {
            Ok(admitted) => {
                let mut outs = vec![
                    Out::Send(link, ServerMsg::Welcome(Box::new(admitted.welcome))),
                    Out::Send(link, ServerMsg::Roster(admitted.roster)),
                ];
                outs.extend(admitted.others);
                if let Some(old) = admitted.replaced {
                    // 이전 접속은 닫지 않습니다. 이유를 읽은 클라이언트가 닫기를 기다립니다(`LinkState::Told`).
                    self.links.insert(
                        old,
                        LinkState::Told {
                            deadline: now + self.join_deadline,
                        },
                    );
                    outs.push(Out::Send(old, ServerMsg::SeatTakenOver));
                }
                if let Some(seed) = admitted.created_with {
                    outs.push(Out::Notice(Notice::RoomCreated {
                        room: admitted.code.clone(),
                        seed,
                    }));
                }
                outs.push(Out::Notice(if admitted.returned {
                    Notice::Returned {
                        room: admitted.code,
                        player: admitted.player,
                    }
                } else {
                    Notice::Joined {
                        room: admitted.code,
                        player: admitted.player,
                        members: admitted.members,
                    }
                }));
                outs
            }
            Err(reason) => {
                let deadline = now + self.join_deadline;
                self.links.insert(link, LinkState::Told { deadline });
                vec![
                    Out::Send(link, ServerMsg::JoinRejected { reason }),
                    Out::Notice(Notice::Rejected { reason }),
                ]
            }
        }
    }

    /// 전송 형식의 글자가 `PlayerName`과 `RoomCode`가 되는 경계입니다.
    /// 방 코드가 없으면 서버가 새 코드를 만들고, 방 코드가 있는데 그 방이 없으면 그 코드로 방을 만듭니다.
    /// 표식은 그 방의 자리와 맞을 때만 쓰이고, 맞지 않으면 아무 일도 없이 새 플레이어로 들어갑니다.
    fn admit(
        &mut self,
        link: LinkId,
        version: u32,
        raw_room: Option<&str>,
        raw_name: &str,
        token: Option<SeatToken>,
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
        let at_room_limit = self.rooms.len() >= self.max_rooms;
        let mut created_with = None;
        let room = match self.rooms.entry(code.clone()) {
            Entry::Occupied(occupied) => occupied.into_mut(),
            Entry::Vacant(_) if at_room_limit => return Err(RejectReason::ServerFull),
            Entry::Vacant(vacant) => {
                self.rooms_created += 1;
                let seed = self.seeds.for_room(self.rooms_created);
                created_with = Some(seed);
                vacant.insert(Room::new(
                    code.clone(),
                    self.tuning.clone(),
                    seed,
                    self.times,
                ))
            }
        };
        let fresh = self.mint.token();
        let entered =
            room.enter(link, name, token, fresh)
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
            returned: entered.returned,
            replaced: entered.replaced,
            members,
            welcome: entered.welcome,
            roster: entered.roster,
            others: entered.others,
            created_with,
        })
    }

    /// 플레이어가 스스로 나갑니다. 자리를 곧바로 내놓고 접속을 닫습니다. 마지막 자리였으면 방을 없앱니다.
    /// 나간 접속에는 더 보낼 것이 없으므로 닫아도 잃는 메시지가 없습니다.
    fn leave(&mut self, link: LinkId) -> Vec<Out> {
        let mut outs = Vec::new();
        if let Some(LinkState::InRoom { room, player, .. }) =
            self.links.insert(link, LinkState::Closing)
        {
            outs.extend(self.give_up_seat(room, player));
        }
        outs.push(Out::Close(link));
        outs
    }

    /// 서버가 `reason` 때문에 접속을 닫습니다. 방에 있던 플레이어의 자리는 `fate`가 정합니다.
    fn close_link(&mut self, link: LinkId, reason: String, fate: SeatFate) -> Vec<Out> {
        let mut outs = vec![Out::Notice(Notice::LinkClosed { link, reason })];
        if let Some(LinkState::InRoom { room, player, .. }) =
            self.links.insert(link, LinkState::Closing)
        {
            outs.extend(match fate {
                SeatFate::Hold => self.hold_seat(room, player, link),
                SeatFate::GiveUp => self.give_up_seat(room, player),
            });
        }
        outs.push(Out::Close(link));
        outs
    }

    /// `link`가 닫혔으니 자리를 유지합니다. 자리는 방에 남고, 시간이 다 되면 끝납니다.
    /// 이미 다른 접속이 이어받은 자리이면 아무 일도 하지 않습니다.
    fn hold_seat(&mut self, code: RoomCode, player: PlayerId, link: LinkId) -> Vec<Out> {
        let held = self
            .rooms
            .get_mut(&code)
            .is_some_and(|room| room.link_closed(player, link));
        if held {
            vec![Out::Notice(Notice::Held { room: code, player })]
        } else {
            Vec::new()
        }
    }

    /// 자리를 곧바로 내놓습니다. 남은 사람에게 새 명단을 보내고, 마지막 자리였으면 방도 없앱니다.
    fn give_up_seat(&mut self, code: RoomCode, player: PlayerId) -> Vec<Out> {
        let mut outs = Vec::new();
        if let Some(room) = self.rooms.get_mut(&code) {
            outs.extend(room.leave(player));
            outs.push(Out::Notice(Notice::Left {
                room: code,
                player,
                members: room.member_count(),
            }));
        }
        outs.extend(self.remove_empty_rooms());
        outs
    }

    /// 자리가 하나도 남지 않은 방을 없앱니다.
    fn remove_empty_rooms(&mut self) -> Vec<Out> {
        let empty: Vec<RoomCode> = self
            .rooms
            .iter()
            .filter(|(_, room)| room.is_empty())
            .map(|(code, _)| code.clone())
            .collect();
        empty
            .into_iter()
            .map(|room| {
                self.rooms.remove(&room);
                Out::Notice(Notice::RoomRemoved { room })
            })
            .collect()
    }
}
