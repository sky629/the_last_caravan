//! 게임 서버 라이브러리입니다. 방을 관리하고, 방마다 Tick을 실행하고, 스냅샷을 보냅니다.
//!
//! 이 크레이트는 소켓을 열지 않고, 스레드를 만들지 않고, 현재 시각을 직접 읽지 않습니다.
//! 호출하는 쪽이 접속 방식과 현재 시각을 넘기고 `GameServer::pump`를 반복해서 호출합니다.
//! 그래서 전용 서버 실행 파일과 테스트가 같은 코드를 실행하고, 다른 게임 프로세스에 내장할 수도 있습니다.
//!
//! 읽는 순서: `input`(플레이어 한 명의 입력), `seat`(자리 하나), `room`(방 하나), `random`(무작위 바이트의 원천), `mint`(새 방의 코드와 자리의 표식), `hub`(접속과 방), `clock`(Tick 일정), 이 파일.

mod clock;
mod hub;
mod input;
mod mint;
mod random;
mod room;
mod seat;

use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

use caravan_protocol::{
    LinkId, PlayerName, RejectReason, RoomCode, SeatToken, ServerLimits, ServerTransport, Welcome,
    check_welcome,
};
use caravan_rules::{PlayerId, Seed, Tuning};

use clock::TickClock;
use hub::{Hub, Out, ticks_of};
pub use random::{PredictableRandom, RandomSource};
use room::{Room, RoomTimes};

/// 서버에서 일어난 일 가운데 운영자에게 알릴 것입니다. 라이브러리는 직접 로그를 쓰지 않고, 이 값을 돌려줍니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// 방이 만들어졌습니다. 그 방의 첫 `Joined` 바로 앞에 옵니다.
    RoomCreated {
        room: RoomCode,
        /// 방이 쓰는 씨앗입니다. 전용 서버를 `--seed`로 이 값을 주고 실행하면 방의 첫 원정이 같은 상태로 시작합니다.
        seed: Seed,
    },
    /// 자리가 하나도 남지 않아서 방이 없어졌습니다. 마지막 `Left` 바로 뒤에 옵니다.
    RoomRemoved {
        room: RoomCode,
    },
    Joined {
        room: RoomCode,
        player: PlayerId,
        members: usize,
    },
    /// 플레이어가 방을 떠났습니다. 스스로 나갔거나(`Leave`), 끊긴 자리를 유지하는 시간이 다 되었습니다.
    Left {
        room: RoomCode,
        player: PlayerId,
        members: usize,
    },
    /// 접속이 끊긴 플레이어의 자리를 유지합니다. 플레이어는 방에 남아 있고, 시간이 다 되면 `Left`가 옵니다.
    Held {
        room: RoomCode,
        player: PlayerId,
    },
    /// 유지 중이던 자리로, 또는 아직 접속 중으로 보던 자리로 같은 표식의 새 접속이 돌아왔습니다.
    Returned {
        room: RoomCode,
        player: PlayerId,
    },
    Rejected {
        reason: RejectReason,
    },
    /// 서버가 접속을 닫았습니다. `reason`은 한도를 넘었거나 약속을 어긴 이유를 설명하는 문장입니다.
    LinkClosed {
        link: LinkId,
        reason: String,
    },
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Notice::Joined {
                room,
                player,
                members,
            } => write!(
                f,
                "player {} joined room {room} ({members} members)",
                player.0
            ),
            Notice::Left {
                room,
                player,
                members,
            } => write!(
                f,
                "player {} left room {room} ({members} members)",
                player.0
            ),
            Notice::Held { room, player } => {
                write!(
                    f,
                    "player {} lost the link in room {room}; the seat is held",
                    player.0
                )
            }
            Notice::Returned { room, player } => {
                write!(f, "player {} returned to room {room}", player.0)
            }
            Notice::Rejected { reason } => write!(f, "join rejected: {reason:?}"),
            Notice::RoomCreated { room, seed } => {
                write!(f, "room {room} created (seed {})", seed.0)
            }
            Notice::RoomRemoved { room } => write!(f, "room {room} removed"),
            Notice::LinkClosed { link, reason } => write!(f, "link {} closed: {reason}", link.0),
        }
    }
}

/// 방마다 씨앗을 정하는 방법입니다. 서버는 시계를 읽지 않으므로 호출하는 쪽이 씨앗을 넘깁니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomSeeds {
    /// 모든 방이 같은 씨앗으로 시작합니다. 같은 씨앗은 같은 결과를 만들므로 버그를 다시 만드는 데 씁니다. 전용 서버의 `--seed N`입니다.
    Fixed(Seed),
    /// 방을 만든 순서마다 `base`에서 이어 간 씨앗입니다. 방마다 다른 씨앗이 됩니다. `base`는 서버를 실행할 때마다 달라야 하므로 호출하는 쪽이 정합니다.
    Fresh(Seed),
}

impl RoomSeeds {
    /// `serial`번째로 만든 방(1부터 셉니다)이 쓸 씨앗입니다.
    fn for_room(self, serial: u32) -> Seed {
        match self {
            RoomSeeds::Fixed(seed) => seed,
            RoomSeeds::Fresh(base) => base.derive(u64::from(serial)),
        }
    }
}

/// 서버를 만들지 못한 이유입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerError {
    /// `Tuning::validate`가 거절했거나, 클라이언트가 받아들이지 않을 입장 안내를 만드는 수치입니다.
    /// 값은 어긋난 첫 항목을 설명하는 문장입니다.
    InvalidTuning(String),
    /// 접속의 한도가 `Tuning`과 맞지 않습니다. 값은 어긋난 항목을 설명하는 문장입니다.
    InvalidLimits(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::InvalidTuning(why) => write!(f, "invalid tuning: {why}"),
            ServerError::InvalidLimits(why) => write!(f, "invalid limits: {why}"),
        }
    }
}

impl Error for ServerError {}

/// 서버의 현재 규모입니다. 실행 파일이 주기적으로 기록하는 용도입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerStats {
    pub rooms: usize,
    /// 모든 방의 플레이어 수 합계입니다.
    pub members: usize,
    /// 모든 방에 살아 있는 몬스터 수 합계입니다.
    pub monsters: usize,
    /// 서버가 시작된 뒤 시계가 실행한 Tick의 수입니다. 한 Tick은 모든 방을 한 번씩 진행합니다.
    pub ticks: u64,
}

/// 접속 하나가 1초에 보낼 수 있는 메시지 수(`ServerLimits::max_messages_per_sec`)는 `tick_hz`의 이 배수 이상이어야 합니다.
/// 클라이언트는 Tick마다 입력을 하나씩 보내고, 네트워크가 멈췄다가 풀리면 밀린 입력을 한꺼번에 보냅니다.
/// 한도가 세는 1초 구간에는 밀린 입력 전부와 그 뒤 1초 분량의 입력이 함께 들어올 수 있습니다.
/// 그래서 Tick마다 입력을 보내는 클라이언트가 한도에 닿지 않고 멈출 수 있는 시간은, 이 배수에서 1을 뺀 초에서 Tick 한 번의 시간을 뺀 값까지입니다.
/// 지금의 5배에서는 4초에서 Tick 한 번을 뺀 시간이고, 20 Hz에서는 3.95초입니다. 이보다 오래 멈췄던 클라이언트는 밀린 입력이 몰려오는 순간 한도를 넘어 접속이 끊길 수 있습니다.
const LINK_RATE_HEADROOM: u32 = 5;

pub struct GameServer {
    transport: Box<dyn ServerTransport>,
    hub: Hub,
    clock: TickClock,
    events_per_pump: usize,
    ticks_run: u64,
}

/// `Tuning::validate`가 검사하는 규칙의 전제에 더해, 이 수치로 보낼 입장 안내를 클라이언트가 받아들이는지와 접속 한도에 맞는지 검사합니다.
fn check_config(tuning: &Tuning, limits: &ServerLimits) -> Result<(), ServerError> {
    tuning.validate().map_err(ServerError::InvalidTuning)?;

    // 클라이언트는 상한을 넘는 입장 안내를 받으면 접속을 포기하므로, 보낼 입장 안내에 같은 확인을 미리 해서
    // 어떤 클라이언트도 들어올 수 없는 서버가 시작되지 않게 합니다. 이 상한은 맵 좌표가 전송 범위(`WirePos`)를 넘는 설정도 거절합니다.
    check_welcome(&first_welcome(tuning, limits)).map_err(ServerError::InvalidTuning)?;

    let needed = tuning.tick_hz.saturating_mul(LINK_RATE_HEADROOM);
    if limits.max_messages_per_sec < needed {
        return Err(ServerError::InvalidLimits(format!(
            "max_messages_per_sec {} must be at least {needed} (tick_hz {} x {LINK_RATE_HEADROOM})",
            limits.max_messages_per_sec, tuning.tick_hz
        )));
    }

    if ticks_of(limits.camp_idle, tuning.tick_hz) == 0 {
        return Err(ServerError::InvalidLimits(format!(
            "camp_idle {:?} must be at least one tick",
            limits.camp_idle
        )));
    }
    // 한 집에서 방 하나의 사람들이 함께 끊겨도 모두 유지되어야 합니다.
    if limits.max_held_seats_per_ip < tuning.max_players {
        return Err(ServerError::InvalidLimits(format!(
            "max_held_seats_per_ip {} must be at least max_players {}",
            limits.max_held_seats_per_ip, tuning.max_players
        )));
    }
    // 한 상대가 열린 접속과 유지시킨 자리로 방을 모두 차지하지 못하려면, 방의 수가 그 상대가 차지할 수 있는 방의 수보다 커야 합니다.
    let one_peer_can_hold = limits
        .max_links_per_ip
        .saturating_add(limits.max_held_seats_per_ip);
    // 서버 전체의 한도는 한 상대의 한도 이상이어야 그 한도가 의미가 있고, 방의 한도보다 작아야 끊긴 자리만으로는 방의 한도를 채우지 못합니다.
    if limits.max_held_seats < limits.max_held_seats_per_ip
        || limits.max_held_seats >= limits.max_rooms
    {
        return Err(ServerError::InvalidLimits(format!(
            "max_held_seats {} must be at least max_held_seats_per_ip {} and smaller than max_rooms {}",
            limits.max_held_seats, limits.max_held_seats_per_ip, limits.max_rooms
        )));
    }
    if limits.max_rooms <= one_peer_can_hold {
        return Err(ServerError::InvalidLimits(format!(
            "max_rooms {} must be larger than max_links_per_ip {} plus max_held_seats_per_ip {}",
            limits.max_rooms, limits.max_links_per_ip, limits.max_held_seats_per_ip
        )));
    }

    // 자리를 비운 것으로 보는 시간이 자리를 내놓는 시간보다 짧아야, 자리가 끝나기 전에 보호가 시작됩니다.
    let (away, hold) = (
        ticks_of(limits.away_after, tuning.tick_hz),
        ticks_of(limits.seat_hold, tuning.tick_hz),
    );
    if away == 0 || away >= hold {
        return Err(ServerError::InvalidLimits(format!(
            "away_after {:?} must be at least one tick and shorter than seat_hold {:?}",
            limits.away_after, limits.seat_hold
        )));
    }
    // 자리를 내놓는 시간은 마지막 메시지부터 셉니다. 조용한 접속을 닫는 시간이 그보다 길면, 자리가 끝난 뒤에도 접속이 열려 있게 됩니다.
    if limits.idle_timeout > limits.seat_hold {
        return Err(ServerError::InvalidLimits(format!(
            "idle_timeout {:?} must not be longer than seat_hold {:?}",
            limits.idle_timeout, limits.seat_hold
        )));
    }
    Ok(())
}

/// 이 수치로 만든 빈 방에 첫 플레이어가 입장할 때 서버가 보내는 입장 안내입니다.
fn first_welcome(tuning: &Tuning, limits: &ServerLimits) -> Welcome {
    // 입장 안내는 씨앗에서 정해지는 것을 담지 않으므로, 어떤 씨앗으로 만든 방이든 같은 안내를 냅니다.
    // 입장 안내의 방 코드, 이름, 표식은 입장 안내의 크기와 상한에 관계가 없으므로 아무 값이나 씁니다.
    let code = RoomCode::parse("check").expect("the code is valid");
    let name = PlayerName::parse("check").expect("the name is valid");
    let times = RoomTimes {
        input_stale_ticks: limits.input_stale_ticks,
        away_ticks: 1,
        camp_idle_ticks: 1,
        hold_ticks: 2,
    };
    let mut room = Room::new(code, tuning.clone(), Seed(0), times);
    // `Tuning::validate`가 정원이 1 이상임을 확인했으므로 빈 방의 첫 입장은 항상 성공합니다.
    room.enter(LinkId(0), name, None, SeatToken([0; 16]))
        .expect("a validated tuning admits the first player")
        .welcome
}

impl GameServer {
    /// 수치가 규칙의 전제를 지키고, 클라이언트가 받아들이는 입장 안내를 만들고, 접속 한도에 맞는지 한 번 검사하고, 어긋나면 오류를 돌려줍니다.
    /// 이 검사를 통과한 수치로 만든 방은 개체가 벽을 통과하는 것 같은 오동작을 일으키지 않습니다.
    /// 또 Tick마다 입력을 보내는 클라이언트는 `LINK_RATE_HEADROOM`에 적힌 시간까지 멈췄다가 입력을 몰아서 보내도 접속 한도에 닿지 않습니다.
    pub fn new(
        transport: Box<dyn ServerTransport>,
        tuning: Tuning,
        limits: ServerLimits,
        seeds: RoomSeeds,
        random: Box<dyn RandomSource>,
        now: Instant,
    ) -> Result<GameServer, ServerError> {
        check_config(&tuning, &limits)?;
        Ok(GameServer {
            transport,
            clock: TickClock::new(tuning.tick_hz, now),
            hub: Hub::new(tuning, limits, seeds, random),
            events_per_pump: limits.events_per_pump,
            ticks_run: 0,
        })
    }

    /// 서버를 한 번 진행합니다. 기다리지 않습니다.
    /// 접속 방식에 도착한 이벤트를 `events_per_pump`개까지 처리하고, 정해진 시간 동안 메시지가 없는 접속을 닫고,
    /// `now`까지 실행해야 하는 Tick을 실행한 뒤, 알릴 일을 돌려줍니다. 처리하지 못한 이벤트는 다음 호출이 처리합니다.
    pub fn pump(&mut self, now: Instant) -> Vec<Notice> {
        let mut notices = Vec::new();
        for _ in 0..self.events_per_pump {
            let Some(event) = self.transport.poll() else {
                break;
            };
            let outs = self.hub.handle_event(event, now);
            self.apply(outs, &mut notices);
        }
        let outs = self.hub.close_silent_links(now);
        self.apply(outs, &mut notices);
        for _ in 0..self.clock.due(now) {
            let outs = self.hub.tick();
            self.ticks_run += 1;
            self.apply(outs, &mut notices);
        }
        notices
    }

    pub fn stats(&self) -> ServerStats {
        self.hub.stats(self.ticks_run)
    }

    /// 다음 Tick까지 남은 시간입니다. 실행 파일은 이 시간만큼 쉰 뒤 다시 `pump`를 호출합니다.
    pub fn time_until_next_tick(&self, now: Instant) -> Duration {
        self.clock.time_until_next(now)
    }

    /// 허브가 요청한 일을 실행합니다. 접속 방식을 호출하는 곳은 이 함수 하나입니다.
    fn apply(&mut self, outs: Vec<Out>, notices: &mut Vec<Notice>) {
        for out in outs {
            match out {
                Out::Send(link, msg) => self.transport.send(link, &msg),
                Out::Close(link) => self.transport.close(link),
                Out::Notice(notice) => notices.push(notice),
            }
        }
    }
}
