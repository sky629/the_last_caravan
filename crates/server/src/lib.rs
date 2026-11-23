//! 게임 서버 라이브러리입니다. 방을 관리하고, 방마다 Tick을 실행하고, 스냅샷을 보냅니다.
//!
//! 이 크레이트는 소켓을 열지 않고, 스레드를 만들지 않고, 현재 시각을 직접 읽지 않습니다.
//! 호출하는 쪽이 접속 방식과 현재 시각을 넘기고 `GameServer::pump`를 반복해서 호출합니다.
//! 그래서 전용 서버 실행 파일과 테스트가 같은 코드를 실행하고, 다른 게임 프로세스에 내장할 수도 있습니다.
//!
//! 읽는 순서: `room`(방 하나), `hub`(접속과 방), `clock`(Tick 일정), 이 파일.

mod clock;
mod hub;
mod room;

use std::fmt;
use std::time::{Duration, Instant};

use caravan_protocol::{RejectReason, RoomCode, ServerTransport};
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

pub struct GameServer {
    transport: Box<dyn ServerTransport>,
    hub: Hub,
    clock: TickClock,
}

impl GameServer {
    pub fn new(transport: Box<dyn ServerTransport>, tuning: Tuning, now: Instant) -> GameServer {
        GameServer {
            transport,
            clock: TickClock::new(tuning.tick_hz, now),
            hub: Hub::new(tuning),
        }
    }

    /// 서버를 한 번 진행합니다. 기다리지 않습니다.
    /// 접속 방식에 도착한 이벤트를 모두 처리하고, `now`까지 실행해야 하는 Tick을 실행한 뒤, 알릴 일을 돌려줍니다.
    pub fn pump(&mut self, now: Instant) -> Vec<Notice> {
        let mut notices = Vec::new();
        while let Some(event) = self.transport.poll() {
            let outs = self.hub.handle_event(event);
            self.apply(outs, &mut notices);
        }
        for _ in 0..self.clock.due(now) {
            let outs = self.hub.tick();
            self.apply(outs, &mut notices);
        }
        notices
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
