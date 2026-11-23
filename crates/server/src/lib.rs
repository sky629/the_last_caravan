//! 게임 서버 라이브러리입니다. 방을 관리하고, 방마다 Tick을 실행하고, 스냅샷을 보냅니다.
//!
//! 이 크레이트는 소켓을 열지 않고, 스레드를 만들지 않고, 현재 시각을 직접 읽지 않습니다.
//! 호출하는 쪽이 접속 방식과 현재 시각을 넘기고 `GameServer::pump`를 반복해서 호출합니다.
//! 그래서 전용 서버 실행 파일과 테스트가 같은 코드를 실행하고, 다른 게임 프로세스에 내장할 수도 있습니다.
//!
//! 읽는 순서: `input`(플레이어 한 명의 입력), `room`(방 하나), `hub`(접속과 방), `clock`(Tick 일정), 이 파일.

mod clock;
mod hub;
mod input;
mod room;

use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

use caravan_protocol::{RejectReason, RoomCode, ServerLimits, ServerTransport};
use caravan_rules::{PlayerId, Tuning};

use clock::TickClock;
use hub::{Hub, Out};

/// 서버에서 일어난 일 가운데 운영자에게 알릴 것입니다. 라이브러리는 직접 로그를 쓰지 않고, 이 값을 돌려줍니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Joined {
        room: RoomCode,
        player: PlayerId,
        members: usize,
    },
    Left {
        room: RoomCode,
        player: PlayerId,
        members: usize,
    },
    Rejected {
        reason: RejectReason,
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
            Notice::Rejected { reason } => write!(f, "join rejected: {reason:?}"),
        }
    }
}

/// 서버를 만들지 못한 이유입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerError {
    /// `Tuning::validate`가 거절한 수치입니다. 값은 어긋난 첫 항목을 설명하는 문장입니다.
    InvalidTuning(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::InvalidTuning(why) => write!(f, "invalid tuning: {why}"),
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

pub struct GameServer {
    transport: Box<dyn ServerTransport>,
    hub: Hub,
    clock: TickClock,
    events_per_pump: usize,
    ticks_run: u64,
}

impl GameServer {
    /// 수치가 규칙의 전제를 지키는지 한 번 검사하고, 어긋나면 오류를 돌려줍니다.
    /// 이 검사를 통과한 수치로 만든 방은 개체가 벽을 통과하는 것 같은 오동작을 일으키지 않습니다.
    pub fn new(
        transport: Box<dyn ServerTransport>,
        tuning: Tuning,
        limits: ServerLimits,
        now: Instant,
    ) -> Result<GameServer, ServerError> {
        tuning.validate().map_err(ServerError::InvalidTuning)?;
        Ok(GameServer {
            transport,
            clock: TickClock::new(tuning.tick_hz, now),
            hub: Hub::new(tuning, limits),
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
