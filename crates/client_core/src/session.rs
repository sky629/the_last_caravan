//! 클라이언트의 접속 상태입니다. 상태 하나를 열거형 하나로 표현하므로 화면에는 항상 상태 하나만 표시됩니다.
//!
//! ```text
//! Connecting --Connected--> Joining --Welcome--> InRoom
//!                              |
//!                              +--JoinRejected--> Rejected
//! (모든 상태) --Disconnected--> Disconnected   (Rejected는 거절 사유를 유지합니다)
//! ```

use caravan_protocol::{
    ClientEvent, ClientMsg, InputMsg, PROTOCOL_VERSION, RejectReason, RoomCode, ServerMsg, Snapshot,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Session {
    Connecting {
        room: RoomCode,
    },
    /// `Join`을 보내고 `Welcome`이나 `JoinRejected`를 기다립니다.
    Joining {
        room: RoomCode,
    },
    InRoom {
        room: RoomCode,
        you: u32,
        tick_hz: u32,
        /// 가장 최근에 받은 스냅샷의 Tick입니다. 이보다 오래된 스냅샷은 버립니다.
        latest_tick: Option<u64>,
        /// 다음에 보낼 입력의 순서 번호입니다.
        next_seq: u32,
        /// 다음 입력을 보낼 시각입니다 (앱이 시작된 뒤 지난 초).
        next_input_at: f64,
    },
    Rejected {
        reason: RejectReason,
    },
    /// 접속이 끝났습니다. 1단계에는 자동 재접속이 없습니다.
    Disconnected {
        reason: String,
    },
    /// 실행 인자가 잘못되었거나 접속을 시작하지 못했습니다.
    Failed {
        reason: String,
    },
}

/// 이벤트 하나를 처리한 뒤 호출한 쪽(Bevy 앱)이 할 일입니다.
#[derive(Clone, Debug, PartialEq)]
pub enum Reaction {
    Nothing,
    Send(ClientMsg),
    Show(Snapshot),
}

impl Session {
    pub fn new(room: RoomCode) -> Session {
        Session::Connecting { room }
    }

    pub fn on_event(&mut self, event: ClientEvent) -> Reaction {
        match (&mut *self, event) {
            (Session::Connecting { room }, ClientEvent::Connected) => {
                let join = ClientMsg::Join {
                    version: PROTOCOL_VERSION,
                    room: room.as_str().to_string(),
                };
                *self = Session::Joining { room: room.clone() };
                Reaction::Send(join)
            }
            (Session::Joining { room }, ClientEvent::Message(ServerMsg::Welcome(welcome))) => {
                *self = Session::InRoom {
                    room: room.clone(),
                    you: welcome.you,
                    tick_hz: welcome.tick_hz,
                    latest_tick: None,
                    next_seq: 1,
                    next_input_at: 0.0,
                };
                Reaction::Nothing
            }
            (Session::Joining { .. }, ClientEvent::Message(ServerMsg::JoinRejected { reason })) => {
                *self = Session::Rejected { reason };
                Reaction::Nothing
            }
            (
                Session::InRoom { latest_tick, .. },
                ClientEvent::Message(ServerMsg::Snapshot(snapshot)),
            ) => {
                // 접속 방식에 따라 메시지의 순서가 바뀔 수 있으므로, 이미 그린 것보다 오래된 상태는 그리지 않습니다.
                if latest_tick.is_some_and(|latest| snapshot.tick <= latest) {
                    return Reaction::Nothing;
                }
                *latest_tick = Some(snapshot.tick);
                Reaction::Show(snapshot)
            }
            (
                Session::Rejected { .. } | Session::Failed { .. },
                ClientEvent::Disconnected { .. },
            ) => Reaction::Nothing,
            (_, ClientEvent::Disconnected { reason }) => {
                *self = Session::Disconnected { reason };
                Reaction::Nothing
            }
            // 순서에 맞지 않는 메시지(입장 전의 스냅샷 등)는 무시합니다.
            _ => Reaction::Nothing,
        }
    }

    /// 입장한 뒤 서버의 Tick 간격마다 입력 하나를 돌려줍니다. `now_secs`는 앱이 시작된 뒤 지난 시간(초)입니다.
    /// 서버는 일정 시간 동안 메시지가 없는 접속을 닫으므로, 입력은 접속이 살아 있다는 표시도 됩니다.
    pub fn input_due(&mut self, now_secs: f64) -> Option<ClientMsg> {
        let Session::InRoom {
            tick_hz,
            next_seq,
            next_input_at,
            ..
        } = self
        else {
            return None;
        };
        if now_secs < *next_input_at {
            return None;
        }
        let period = 1.0 / f64::from(*tick_hz);
        // 프레임이 오래 멈췄다가 다시 돌아도 밀린 입력을 몰아서 보내지 않습니다.
        *next_input_at = if *next_input_at + period > now_secs {
            *next_input_at + period
        } else {
            now_secs + period
        };
        let seq = *next_seq;
        *next_seq += 1;
        Some(ClientMsg::Input(InputMsg {
            seq,
            ..InputMsg::default()
        }))
    }

    /// 화면의 상태 표시줄에 쓰는 문장입니다. Bevy의 기본 글꼴은 영문만 표시하므로 영어로 씁니다.
    pub fn status_text(&self) -> String {
        match self {
            Session::Connecting { room } => format!("Connecting... (room {room})"),
            Session::Joining { room } => format!("Joining room {room}..."),
            Session::InRoom {
                room,
                you,
                latest_tick,
                ..
            } => match latest_tick {
                Some(tick) => format!("Room {room} | player {you} | tick {tick}"),
                None => format!("Room {room} | player {you} | waiting for the first snapshot"),
            },
            Session::Rejected { reason } => match reason {
                RejectReason::RoomFull { capacity } => {
                    format!("Join rejected: the room is full ({capacity} players)")
                }
                RejectReason::BadRoomCode => "Join rejected: bad room code".to_string(),
                RejectReason::VersionMismatch { server } => format!(
                    "Join rejected: server protocol {server}, client protocol {PROTOCOL_VERSION}. Update or reload."
                ),
            },
            Session::Disconnected { reason } => format!("Disconnected: {reason}"),
            Session::Failed { reason } => format!("Cannot start: {reason}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::Welcome;
    use caravan_rules::{Sim, Tuning};

    fn room() -> RoomCode {
        RoomCode::parse("test").unwrap()
    }

    fn snapshot(tick: u64) -> Snapshot {
        let mut snapshot = Snapshot::from_sim(&Sim::new(Tuning::default()));
        snapshot.tick = tick;
        snapshot
    }

    /// 플레이어 번호가 `you`인 입장 안내입니다.
    fn welcome(you: u32) -> ServerMsg {
        let mut sim = Sim::new(Tuning::default());
        let first = sim.add_player().unwrap();
        let mut welcome = Welcome::from_sim(&sim, sim.player(first).unwrap());
        welcome.you = you;
        ServerMsg::Welcome(welcome)
    }

    fn input(seq: u32) -> ClientMsg {
        ClientMsg::Input(InputMsg {
            seq,
            ..InputMsg::default()
        })
    }

    fn message(msg: ServerMsg) -> ClientEvent {
        ClientEvent::Message(msg)
    }

    fn in_room() -> Session {
        let mut session = Session::new(room());
        session.on_event(ClientEvent::Connected);
        session.on_event(message(welcome(3)));
        session
    }

    #[test]
    fn connecting_sends_join_with_the_room_code_and_protocol_version() {
        let mut session = Session::new(room());
        assert_eq!(
            session.on_event(ClientEvent::Connected),
            Reaction::Send(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: "test".to_string()
            })
        );
        assert_eq!(session, Session::Joining { room: room() });
    }

    #[test]
    fn welcome_enters_the_room_and_snapshots_are_shown_in_tick_order() {
        let mut session = in_room();
        assert!(matches!(
            session,
            Session::InRoom {
                you: 3,
                tick_hz: 20,
                ..
            }
        ));
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(snapshot(5)))),
            Reaction::Show(snapshot(5))
        );
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(snapshot(4)))),
            Reaction::Nothing,
            "an older snapshot is dropped"
        );
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(snapshot(6)))),
            Reaction::Show(snapshot(6))
        );
    }

    #[test]
    fn in_the_room_one_input_is_due_per_server_tick_with_increasing_seq() {
        let mut session = Session::new(room());
        assert_eq!(session.input_due(0.0), None, "no input before joining");
        session.on_event(ClientEvent::Connected);
        session.on_event(message(welcome(3)));

        assert_eq!(session.input_due(1.000), Some(input(1)));
        assert_eq!(session.input_due(1.020), None);
        assert_eq!(session.input_due(1.051), Some(input(2)));
        assert_eq!(session.input_due(1.060), None);
        assert_eq!(session.input_due(9.000), Some(input(3)), "after a stall");
        assert_eq!(session.input_due(9.010), None, "no burst of missed inputs");
        assert_eq!(session.input_due(9.051), Some(input(4)));
    }

    #[test]
    fn a_snapshot_before_welcome_is_ignored() {
        let mut session = Session::new(room());
        session.on_event(ClientEvent::Connected);
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(snapshot(1)))),
            Reaction::Nothing
        );
        assert_eq!(session, Session::Joining { room: room() });
    }

    #[test]
    fn a_rejection_stays_on_screen_after_the_server_closes() {
        let mut session = Session::new(room());
        session.on_event(ClientEvent::Connected);
        let reason = RejectReason::RoomFull { capacity: 4 };
        session.on_event(message(ServerMsg::JoinRejected { reason }));
        session.on_event(ClientEvent::Disconnected {
            reason: "closed".to_string(),
        });
        assert_eq!(session, Session::Rejected { reason });
    }

    #[test]
    fn losing_the_connection_in_the_room_ends_the_session() {
        let mut session = in_room();
        session.on_event(ClientEvent::Disconnected {
            reason: "connection closed".to_string(),
        });
        assert_eq!(
            session,
            Session::Disconnected {
                reason: "connection closed".to_string()
            }
        );
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(snapshot(9)))),
            Reaction::Nothing
        );
    }
}
