//! 클라이언트의 접속 상태입니다. 상태 하나를 열거형 하나로 표현하므로 화면에는 항상 상태 하나만 표시됩니다.
//!
//! ```text
//! Connecting --Connected--> Joining --Welcome--> InRoom
//!     |                        |
//!     |                        +--JoinRejected--> Rejected
//!     |                        +--(상한을 넘은 Welcome)--> Failed
//!     +--Disconnected--> Unreachable   (접속이 한 번도 열리지 않았습니다)
//! (그 밖의 모든 상태) --Disconnected--> Disconnected   (Rejected, Failed, Unreachable는 사유를 유지합니다)
//! ```
//!
//! 방에 들어간 뒤에 필요한 값(입장 안내, 스냅샷, 입력 순서 번호)은 `InRoom`에만 있습니다.
//! 그래서 입장하기 전에 스냅샷을 그리거나 입력을 보내는 일은 값이 없어서 일어날 수 없습니다.

use caravan_protocol::{
    ClientEvent, ClientMsg, PROTOCOL_VERSION, RejectReason, RoomCode, ServerMsg, Snapshot, Welcome,
    check_welcome,
};
use caravan_rules::Vec2;

use crate::input::{Controls, InputSender};
use crate::interp::{Frame, SnapshotBuffer};

#[derive(Clone, Debug, PartialEq)]
pub enum Session {
    /// 접속이 열리기를 기다립니다. 열리지 않고 끝나면 어느 서버에 접속하려 했는지 알려야 하므로 `server`를 가집니다.
    Connecting {
        room: RoomCode,
        server: String,
    },
    /// `Join`을 보내고 `Welcome`이나 `JoinRejected`를 기다립니다.
    Joining {
        room: RoomCode,
    },
    InRoom(Joined),
    Rejected {
        reason: RejectReason,
    },
    /// 열려 있던 접속이 끝났습니다. 1단계에는 자동 재접속이 없습니다.
    Disconnected {
        reason: String,
    },
    /// 접속이 한 번도 열리지 않고 끝났습니다. 서버가 꺼져 있거나 주소가 틀린 경우도 있고, 서버가 응답했지만 접속을 받지 않은 경우도 있습니다.
    /// 접속 방식이 알려 준 이유는 `reason`에 보관해서 시도한 서버 주소와 함께 보여 줍니다.
    /// 브라우저는 이유를 알려 주지 않고 접속 방식이 일반적인 문구를 채우므로, 이 경우 `reason`은 `None`입니다.
    Unreachable {
        server: String,
        reason: Option<String>,
    },
    /// 실행 인자가 잘못되었거나 접속을 시작하지 못했습니다.
    Failed {
        reason: String,
    },
}

/// 방에 들어간 뒤의 상태입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Joined {
    pub room: RoomCode,
    /// 서버가 입장할 때 알려 준 값입니다. 내 플레이어 번호, 진영, Tick 속도, 고정된 세계가 들어 있습니다.
    pub welcome: Welcome,
    snapshots: SnapshotBuffer,
    inputs: InputSender,
}

impl Joined {
    fn new(room: RoomCode, welcome: Welcome) -> Joined {
        Joined {
            room,
            snapshots: SnapshotBuffer::new(welcome.tick_hz),
            inputs: InputSender::new(welcome.tick_hz),
            welcome,
        }
    }
}

impl Session {
    pub fn new(room: RoomCode, server: String) -> Session {
        Session::Connecting { room, server }
    }

    /// 이벤트 하나를 처리합니다. 서버에 답으로 보낼 메시지가 있으면 돌려줍니다.
    pub fn on_event(&mut self, event: ClientEvent) -> Option<ClientMsg> {
        match (&mut *self, event) {
            (Session::Connecting { room, .. }, ClientEvent::Connected) => {
                let join = ClientMsg::Join {
                    version: PROTOCOL_VERSION,
                    room: room.as_str().to_string(),
                };
                *self = Session::Joining { room: room.clone() };
                Some(join)
            }
            (Session::Joining { room }, ClientEvent::Message(ServerMsg::Welcome(welcome))) => {
                // 서버가 보낸 값을 처음 믿기 전에 확인합니다. 여기를 통과한 값은 화면 코드가 다시 확인하지 않습니다.
                *self = match check_welcome(&welcome) {
                    Ok(()) => Session::InRoom(Joined::new(room.clone(), welcome)),
                    Err(reason) => Session::Failed {
                        reason: format!("the server sent an unusable welcome: {reason}"),
                    },
                };
                None
            }
            (Session::Joining { .. }, ClientEvent::Message(ServerMsg::JoinRejected { reason })) => {
                *self = Session::Rejected { reason };
                None
            }
            (Session::InRoom(joined), ClientEvent::Message(ServerMsg::Snapshot(snapshot))) => {
                joined.snapshots.push(snapshot);
                None
            }
            (Session::Connecting { server, .. }, ClientEvent::Disconnected { reason }) => {
                *self = Session::Unreachable {
                    server: server.clone(),
                    reason: transport_reason(reason),
                };
                None
            }
            (
                Session::Rejected { .. } | Session::Failed { .. } | Session::Unreachable { .. },
                ClientEvent::Disconnected { .. },
            ) => None,
            (_, ClientEvent::Disconnected { reason }) => {
                *self = Session::Disconnected { reason };
                None
            }
            // 순서에 맞지 않는 메시지(입장 전의 스냅샷 등)는 무시합니다.
            _ => None,
        }
    }

    /// 입장이 거절되었거나, 접속이 끊겼거나, 접속하지 못했거나, 시작하지 못해서 더 할 일이 없는 상태입니다. 이때 접속 방식을 닫아도 됩니다.
    pub fn is_over(&self) -> bool {
        matches!(
            self,
            Session::Rejected { .. }
                | Session::Disconnected { .. }
                | Session::Unreachable { .. }
                | Session::Failed { .. }
        )
    }

    /// 방에 들어갔을 때의 상태입니다.
    pub fn joined(&self) -> Option<&Joined> {
        match self {
            Session::InRoom(joined) => Some(joined),
            _ => None,
        }
    }

    /// 프레임마다 부릅니다. 재생 시각을 프레임 시간만큼 앞으로 보냅니다.
    pub fn advance(&mut self, dt_secs: f64) {
        if let Session::InRoom(joined) = self {
            joined.snapshots.advance(dt_secs);
        }
    }

    /// 지금 화면에 그릴 방의 모습입니다. 첫 스냅샷을 받고 `advance`를 부르기 전에는 없습니다.
    pub fn frame(&self) -> Option<Frame> {
        self.joined()?.snapshots.frame()
    }

    /// 서버가 가장 최근에 보낸 스냅샷입니다. 화면에 그리는 것보다 100ms 앞서 있습니다.
    pub fn newest_snapshot(&self) -> Option<&Snapshot> {
        self.joined()?.snapshots.newest()
    }

    /// 프레임마다 부릅니다. 입력을 보낼 때가 되었으면 입력 메시지를 돌려줍니다.
    /// `now_secs`는 앱이 시작된 뒤 지난 시간(초)이고, `me`는 내 캐릭터가 화면에 그려진 지면 위치입니다.
    pub fn input_due(
        &mut self,
        now_secs: f64,
        controls: &Controls,
        me: Option<Vec2>,
    ) -> Option<ClientMsg> {
        let Session::InRoom(joined) = self else {
            return None;
        };
        joined
            .inputs
            .poll(now_secs, controls, me)
            .map(ClientMsg::Input)
    }

    /// 입장하지 못했거나 접속하지 못했거나 접속이 끊겼을 때, 그 이유를 화면에 크게 보여 주는 데 쓰는 값입니다. 그 밖의 상태에서는 없습니다.
    pub fn problem(&self) -> Option<Problem> {
        match self {
            Session::Rejected { reason } => Some(Problem {
                heading: "Join rejected",
                reason: match reason {
                    RejectReason::RoomFull { capacity } => {
                        format!("the room is full ({capacity} players)")
                    }
                    RejectReason::BadRoomCode => "bad room code".to_string(),
                    RejectReason::VersionMismatch { server } => format!(
                        "server protocol {server}, client protocol {PROTOCOL_VERSION}. Update or reload."
                    ),
                },
            }),
            Session::Disconnected { reason } => Some(Problem {
                heading: "Disconnected",
                reason: reason.clone(),
            }),
            Session::Unreachable { server, reason } => Some(Problem {
                heading: "Cannot reach the server",
                reason: match reason {
                    Some(reason) => format!(
                        "Could not open {server} ({reason}). Check the address and that the server is running."
                    ),
                    None => format!(
                        "Could not open {server}. Check the address and that the server is running."
                    ),
                },
            }),
            Session::Failed { reason } => Some(Problem {
                heading: "Cannot start",
                reason: reason.clone(),
            }),
            _ => None,
        }
    }

    /// 화면의 접속 상태 표시줄에 쓰는 문장입니다. Bevy의 기본 글꼴은 영문만 표시하므로 영어로 씁니다.
    pub fn status_text(&self) -> String {
        match (self, self.problem()) {
            (_, Some(Problem { heading, reason })) => format!("{heading}: {reason}"),
            (Session::Connecting { room, .. }, None) => format!("Connecting... (room {room})"),
            (Session::Joining { room }, None) => format!("Joining room {room}..."),
            _ => "Connected".to_string(),
        }
    }
}

/// 접속 방식이 알려 준 이유 중 사용자에게 도움이 되는 것만 남깁니다.
/// 브라우저가 이유를 모를 때 접속 방식이 채우는 `Unknown error`와 빈 문자열은 정보가 없으므로 버립니다.
fn transport_reason(reason: String) -> Option<String> {
    let reason = reason.trim();
    if reason.is_empty() || reason.eq_ignore_ascii_case("Unknown error") {
        None
    } else {
        Some(reason.to_string())
    }
}

/// 접속하지 못한 이유입니다. `heading`은 짧은 제목이고 `reason`은 서버나 접속 방식이 알려 준 이유, 또는 사용자가 확인할 내용입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub heading: &'static str,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::InputMsg;
    use caravan_rules::{Sim, Tuning};

    fn room() -> RoomCode {
        RoomCode::parse("test").unwrap()
    }

    const SERVER: &str = "ws://127.0.0.1:7777";

    fn connecting() -> Session {
        Session::new(room(), SERVER.to_string())
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

    fn message(msg: ServerMsg) -> ClientEvent {
        ClientEvent::Message(msg)
    }

    fn in_room() -> Session {
        let mut session = connecting();
        session.on_event(ClientEvent::Connected);
        session.on_event(message(welcome(3)));
        session
    }

    fn seq_of(msg: Option<ClientMsg>) -> Option<u32> {
        match msg {
            Some(ClientMsg::Input(InputMsg { seq, .. })) => Some(seq),
            _ => None,
        }
    }

    #[test]
    fn connecting_sends_join_with_the_room_code_and_protocol_version() {
        let mut session = connecting();
        assert_eq!(
            session.on_event(ClientEvent::Connected),
            Some(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: "test".to_string()
            })
        );
        assert_eq!(session, Session::Joining { room: room() });
    }

    #[test]
    fn welcome_enters_the_room_and_keeps_what_the_server_said() {
        let session = in_room();

        let joined = session.joined().expect("in the room");
        assert_eq!(joined.room, room());
        assert_eq!(joined.welcome.you, 3);
        assert_eq!(joined.welcome.tick_hz, 20);
        let tuning = Tuning::default();
        assert_eq!(
            joined.welcome.world.obstacles.len(),
            tuning.map.obstacles.len()
        );
        assert_eq!(joined.welcome.world.tower_limit, tuning.village.tower_limit);
    }

    #[test]
    fn a_welcome_over_a_limit_fails_the_session_with_a_reason() {
        let mut session = connecting();
        session.on_event(ClientEvent::Connected);
        let ServerMsg::Welcome(mut hostile) = welcome(3) else {
            unreachable!("welcome() builds a Welcome");
        };
        hostile.capacity = u32::MAX;

        session.on_event(message(ServerMsg::Welcome(hostile)));

        let Session::Failed { reason } = &session else {
            panic!("expected Failed, got {session:?}");
        };
        assert!(reason.contains("capacity"), "{reason}");
        assert!(session.is_over());
        assert!(session.joined().is_none());
        // 닫히는 접속이 거절 이유를 지우지 않고, 이후의 스냅샷은 그려지지 않습니다.
        session.on_event(ClientEvent::Disconnected {
            reason: "closed".to_string(),
        });
        session.on_event(message(ServerMsg::Snapshot(snapshot(1))));
        assert!(matches!(session, Session::Failed { .. }));
        assert!(session.frame().is_none());
    }

    #[test]
    fn a_link_that_closes_before_it_opens_keeps_the_reason_and_names_the_server() {
        let mut session = connecting();
        session.on_event(ClientEvent::Disconnected {
            reason: "Handshake not finished".to_string(),
        });

        assert_eq!(
            session,
            Session::Unreachable {
                server: SERVER.to_string(),
                reason: Some("Handshake not finished".to_string()),
            }
        );
        assert!(session.is_over());
        assert_eq!(
            session.problem(),
            Some(Problem {
                heading: "Cannot reach the server",
                reason: "Could not open ws://127.0.0.1:7777 (Handshake not finished). Check the address and that the server is running.".to_string(),
            })
        );

        // 이어서 닫힘이 다시 알려져도 이유가 바뀌지 않습니다.
        session.on_event(ClientEvent::Disconnected {
            reason: "closed".to_string(),
        });
        assert_eq!(
            session.status_text(),
            "Cannot reach the server: Could not open ws://127.0.0.1:7777 (Handshake not finished). Check the address and that the server is running."
        );
    }

    #[test]
    fn a_placeholder_or_empty_reason_is_hidden() {
        for reason in ["Unknown error", "unknown error", "", "  "] {
            let mut session = connecting();
            session.on_event(ClientEvent::Disconnected {
                reason: reason.to_string(),
            });

            assert_eq!(
                session,
                Session::Unreachable {
                    server: SERVER.to_string(),
                    reason: None,
                },
                "reason {reason:?}"
            );
            assert_eq!(
                session.status_text(),
                "Cannot reach the server: Could not open ws://127.0.0.1:7777. Check the address and that the server is running.",
                "reason {reason:?}"
            );
        }
    }

    #[test]
    fn a_link_that_closes_after_it_opened_keeps_the_reason_it_gave() {
        let mut waiting_for_welcome = connecting();
        waiting_for_welcome.on_event(ClientEvent::Connected);
        waiting_for_welcome.on_event(ClientEvent::Disconnected {
            reason: "the server closed the connection".to_string(),
        });

        assert_eq!(
            waiting_for_welcome.status_text(),
            "Disconnected: the server closed the connection"
        );
    }

    #[test]
    fn only_a_finished_session_is_over() {
        assert!(!connecting().is_over());
        assert!(!in_room().is_over());
        let mut disconnected = in_room();
        disconnected.on_event(ClientEvent::Disconnected {
            reason: "closed".to_string(),
        });
        assert!(disconnected.is_over());
    }

    #[test]
    fn snapshots_are_buffered_and_an_older_one_is_dropped() {
        let mut session = in_room();
        assert!(
            session.frame().is_none(),
            "nothing to draw before a snapshot"
        );

        session.on_event(message(ServerMsg::Snapshot(snapshot(5))));
        session.on_event(message(ServerMsg::Snapshot(snapshot(4))));
        session.on_event(message(ServerMsg::Snapshot(snapshot(6))));
        session.advance(0.016);

        assert_eq!(session.newest_snapshot().map(|s| s.tick), Some(6));
        assert!(session.frame().is_some());
    }

    #[test]
    fn a_snapshot_before_welcome_is_ignored() {
        let mut session = connecting();
        session.on_event(ClientEvent::Connected);
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(snapshot(1)))),
            None
        );
        assert_eq!(session, Session::Joining { room: room() });
        assert!(session.newest_snapshot().is_none());
    }

    #[test]
    fn a_rejection_stays_on_screen_after_the_server_closes() {
        let mut session = connecting();
        session.on_event(ClientEvent::Connected);
        let reason = RejectReason::RoomFull { capacity: 4 };
        session.on_event(message(ServerMsg::JoinRejected { reason }));
        session.on_event(ClientEvent::Disconnected {
            reason: "closed".to_string(),
        });
        assert_eq!(session, Session::Rejected { reason });
        assert_eq!(
            session.status_text(),
            "Join rejected: the room is full (4 players)"
        );
    }

    #[test]
    fn every_rejection_reason_is_explained_on_screen() {
        for (reason, expected) in [
            (RejectReason::BadRoomCode, "bad room code"),
            (
                RejectReason::VersionMismatch { server: 9 },
                "server protocol 9",
            ),
        ] {
            let text = Session::Rejected { reason }.status_text();
            assert!(text.contains(expected), "{text}");
        }
    }

    #[test]
    fn losing_the_connection_in_the_room_ends_the_session() {
        let mut session = in_room();
        session.on_event(ClientEvent::Disconnected {
            reason: "the server closed the connection".to_string(),
        });
        assert_eq!(
            session,
            Session::Disconnected {
                reason: "the server closed the connection".to_string()
            }
        );
        assert_eq!(
            session.status_text(),
            "Disconnected: the server closed the connection"
        );
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(snapshot(9)))),
            None
        );
        assert!(session.frame().is_none());
    }

    #[test]
    fn inputs_are_sent_only_in_the_room_one_per_server_tick() {
        let idle = Controls::default();
        let mut session = connecting();
        assert_eq!(session.input_due(0.0, &idle, None), None);
        session.on_event(ClientEvent::Connected);
        session.on_event(message(welcome(3)));

        assert_eq!(seq_of(session.input_due(1.000, &idle, None)), Some(1));
        assert_eq!(seq_of(session.input_due(1.020, &idle, None)), None);
        assert_eq!(seq_of(session.input_due(1.051, &idle, None)), Some(2));
    }
}
