//! 클라이언트의 접속 상태입니다. 상태 하나를 열거형 하나로 표현하므로 화면에는 항상 상태 하나만 표시됩니다.
//!
//! ```text
//! Connecting --Connected--> Joining --Welcome--> InRoom
//!     |                        |
//!     |                        +--JoinRejected--> Rejected
//!     |                        +--(상한을 넘은 Welcome)--> Failed
//!     +--Disconnected--> Unreachable   (접속이 한 번도 열리지 않았습니다)
//! InRoom --SeatTakenOver--> TakenOver   (같은 표식의 다른 접속이 자리를 이어받았습니다)
//! (그 밖의 모든 상태) --Disconnected--> Disconnected   (Rejected, Failed, Unreachable, TakenOver는 사유를 유지합니다)
//! ```
//!
//! 방에 들어간 뒤에 필요한 값(입장 안내, 명단, 스냅샷, 입력 순서 번호, 나가기 확인 창)은 `InRoom`에만 있습니다.
//! 그래서 입장하기 전에 스냅샷을 그리거나 입력을 보내는 일은 값이 없어서 일어날 수 없습니다.
//! 끝난 상태(Rejected, Disconnected, Unreachable, Failed, TakenOver)가 되면 `Flow`가 접속을 버리고 시작 화면으로 돌아갑니다.

use caravan_protocol::{
    ClientEvent, ClientMsg, PROTOCOL_VERSION, PlayerName, RejectReason, RoomCode, SeatToken,
    ServerMsg, Snapshot, Welcome, WireMember, WireReport, WireRoster, check_welcome,
};
use caravan_rules::Vec2;

use crate::hud::reject_text;
use crate::input::{Controls, InputSender};
use crate::interp::{Frame, SnapshotBuffer};
use crate::leave::LeavePrompt;
use crate::start::JoinRequest;

#[derive(Clone, Debug, PartialEq)]
pub enum Session {
    /// 접속이 열리기를 기다립니다. 열리지 않고 끝나면 어느 서버에 접속하려 했는지 알려야 하므로 `server`를 가집니다.
    /// `token`은 이 기기가 기억하는 그 방의 내 자리의 표식이고, 있으면 `Join`에 실어서 같은 자리로 돌아갑니다.
    Connecting {
        request: JoinRequest,
        server: String,
        token: Option<SeatToken>,
    },
    /// `Join`을 보내고 `Welcome`이나 `JoinRejected`를 기다립니다.
    Joining {
        request: JoinRequest,
        /// 입장 안내보다 먼저 도착한 명단입니다. 브라우저는 메시지를 보낸 순서와 다르게 넘겨줄 수 있으므로, 버리지 않고 입장한 뒤에 씁니다.
        early_roster: Option<WireRoster>,
        /// 입장 안내보다 먼저 도착한 결과입니다. 결과 화면 중에 입장하면 결과가 입장 안내 다음에 오는데, 명단과 같은 까닭으로 입장한 뒤에 씁니다.
        early_report: Option<WireReport>,
    },
    /// 방에 들어간 뒤의 값은 다른 상태보다 훨씬 크므로 `Box`에 둡니다. 상태 하나를 옮길 때마다 가장 큰 상태의 크기를 복사하지 않게 합니다.
    InRoom(Box<Joined>),
    Rejected {
        reason: RejectReason,
    },
    /// 열려 있던 접속이 끝났습니다. 자동 재접속은 없습니다.
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
    /// 같은 표식의 새 접속이 이 접속의 자리를 이어받았습니다. 서버는 이 접속을 닫지 않으므로, 이유를 읽은 쪽이 닫습니다.
    TakenOver,
}

/// 방에 들어간 뒤의 상태입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Joined {
    /// 들어간 방의 코드입니다. 방 코드를 비우고 입장했으면 서버가 만든 코드이므로, 입장 안내에서 읽습니다.
    pub room: RoomCode,
    pub name: PlayerName,
    /// 서버가 입장할 때 알려 준 값입니다. 내 플레이어 번호, 진영, Tick 속도, 고정된 세계가 들어 있습니다.
    pub welcome: Welcome,
    /// 방에 있는 사람입니다. 서버가 `Roster`를 보낼 때마다 바뀝니다.
    pub roster: Vec<WireMember>,
    /// 서버가 가장 최근에 보낸 결과입니다. 결과 화면이 열릴 때 오고, 결과에는 몇 번째 원정의 것인지가 있어서
    /// 지금 그리는 원정(`Frame::expedition`)의 것인지 화면이 알아봅니다. 결과 화면이 아닐 때에도 마지막 결과가 남아 있지만 그리지 않습니다.
    pub report: Option<WireReport>,
    /// 방을 나가기 전에 묻는 확인 창입니다. 방에 들어간 뒤의 상태에만 있으므로, 방을 나가거나 접속이 끊겨서 `Joined`가 사라지면 창도 사라집니다.
    pub(crate) leave_prompt: LeavePrompt,
    /// 받아들인 명단의 번호입니다. 아직 받아들인 명단이 없으면 0입니다.
    roster_revision: u32,
    snapshots: SnapshotBuffer,
    inputs: InputSender,
}

impl Joined {
    fn new(room: RoomCode, name: PlayerName, welcome: Welcome) -> Joined {
        Joined {
            room,
            name,
            roster: Vec::new(),
            report: None,
            leave_prompt: LeavePrompt::default(),
            roster_revision: 0,
            snapshots: SnapshotBuffer::new(welcome.tick_hz),
            inputs: InputSender::new(welcome.tick_hz),
            welcome,
        }
    }
}

impl Joined {
    /// 서버가 보낸 명단을 받아들입니다. 이미 받아들인 것보다 오래된 명단과, 이름이 규칙에 맞지 않거나 방의 정원보다 긴 명단은 버립니다.
    /// 이름은 화면에 그대로 그리므로, 서버가 보낸 값이라도 믿기 전에 확인합니다.
    fn take_roster(&mut self, roster: WireRoster) {
        let usable = roster.members.len() <= self.welcome.capacity as usize
            && roster
                .members
                .iter()
                .all(|member| PlayerName::parse(&member.name).is_ok());
        if usable && roster.revision > self.roster_revision {
            self.roster_revision = roster.revision;
            self.roster = roster.members;
        }
    }

    /// 서버가 보낸 결과를 받아들입니다. 이름이 규칙에 맞지 않거나 방의 정원보다 긴 결과는 버립니다. 이름은 화면에 그대로 그리기 때문입니다.
    fn take_report(&mut self, report: WireReport) {
        let usable = report.rows.len() <= self.welcome.capacity as usize
            && report
                .rows
                .iter()
                .all(|row| PlayerName::parse(&row.name).is_ok());
        if usable {
            self.report = Some(report);
        }
    }
}

impl Session {
    pub fn new(request: JoinRequest, server: String, token: Option<SeatToken>) -> Session {
        Session::Connecting {
            request,
            server,
            token,
        }
    }

    /// 이벤트 하나를 처리합니다. 서버에 답으로 보낼 메시지가 있으면 돌려줍니다.
    pub fn on_event(&mut self, event: ClientEvent) -> Option<ClientMsg> {
        match (&mut *self, event) {
            (Session::Connecting { request, token, .. }, ClientEvent::Connected) => {
                let join = ClientMsg::Join {
                    version: PROTOCOL_VERSION,
                    room: request.room.as_ref().map(|room| room.as_str().to_string()),
                    name: request.name.as_str().to_string(),
                    token: *token,
                };
                *self = Session::Joining {
                    request: request.clone(),
                    early_roster: None,
                    early_report: None,
                };
                Some(join)
            }
            (
                Session::Joining {
                    request,
                    early_roster,
                    early_report,
                },
                ClientEvent::Message(ServerMsg::Welcome(welcome)),
            ) => {
                // 서버가 보낸 값을 처음 믿기 전에 확인합니다. 여기를 통과한 값은 화면 코드가 다시 확인하지 않습니다.
                *self = match checked_room(&welcome) {
                    Ok(room) => {
                        let mut joined = Joined::new(room, request.name.clone(), *welcome);
                        if let Some(roster) = early_roster.take() {
                            joined.take_roster(roster);
                        }
                        if let Some(report) = early_report.take() {
                            joined.take_report(report);
                        }
                        Session::InRoom(Box::new(joined))
                    }
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
                joined.snapshots.push(*snapshot);
                None
            }
            (Session::InRoom(joined), ClientEvent::Message(ServerMsg::Roster(roster))) => {
                joined.take_roster(roster);
                None
            }
            (Session::InRoom(joined), ClientEvent::Message(ServerMsg::Debrief(report))) => {
                joined.take_report(report);
                None
            }
            (Session::InRoom(_), ClientEvent::Message(ServerMsg::SeatTakenOver)) => {
                *self = Session::TakenOver;
                None
            }
            (
                Session::Joining { early_roster, .. },
                ClientEvent::Message(ServerMsg::Roster(roster)),
            ) => {
                let newer = early_roster
                    .as_ref()
                    .is_none_or(|held| roster.revision > held.revision);
                if newer {
                    *early_roster = Some(roster);
                }
                None
            }
            (
                Session::Joining { early_report, .. },
                ClientEvent::Message(ServerMsg::Debrief(report)),
            ) => {
                *early_report = Some(report);
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
                Session::Rejected { .. }
                | Session::Failed { .. }
                | Session::Unreachable { .. }
                | Session::TakenOver,
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

    /// 입장이 거절되었거나, 접속이 끊겼거나, 접속하지 못했거나, 시작하지 못했거나, 자리를 다른 접속에 내주어서 더 할 일이 없는 상태입니다.
    /// 이때 접속 방식을 닫아도 됩니다. `Flow`가 접속을 버리고 시작 화면으로 돌아갑니다.
    pub fn is_over(&self) -> bool {
        matches!(
            self,
            Session::Rejected { .. }
                | Session::Disconnected { .. }
                | Session::Unreachable { .. }
                | Session::Failed { .. }
                | Session::TakenOver
        )
    }

    /// 방에 들어갔을 때의 상태입니다.
    pub fn joined(&self) -> Option<&Joined> {
        match self {
            Session::InRoom(joined) => Some(joined.as_ref()),
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

    /// 끝난 상태(`is_over`)의 이유입니다. `Flow`가 시작 화면으로 돌아가면서 안내 한 줄로 보여 주고, 접속 상태 문장(`status_text`)도 이 값으로 만듭니다.
    /// 그 밖의 상태에서는 없습니다.
    pub fn problem(&self) -> Option<Problem> {
        match self {
            Session::Rejected { reason } => Some(Problem {
                heading: "Join rejected",
                reason: reject_text(*reason),
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
            Session::TakenOver => Some(Problem {
                heading: "Seat taken over",
                reason: "Another window took over this seat".to_string(),
            }),
            _ => None,
        }
    }

    /// 화면의 접속 상태 표시줄에 쓰는 문장입니다. Bevy의 기본 글꼴은 영문만 표시하므로 영어로 씁니다.
    pub fn status_text(&self) -> String {
        match (self, self.problem()) {
            (_, Some(Problem { heading, reason })) => format!("{heading}: {reason}"),
            (Session::Connecting { request, .. }, None) => match &request.room {
                Some(room) => format!("Connecting... (room {room})"),
                None => "Connecting... (new room)".to_string(),
            },
            (Session::Joining { request, .. }, None) => match &request.room {
                Some(room) => format!("Joining room {room}..."),
                None => "Creating a room...".to_string(),
            },
            _ => "Connected".to_string(),
        }
    }
}

/// 입장 안내를 처음 믿기 전에 하는 확인입니다. 통과하면 서버가 알려 준 방 코드를 돌려줍니다.
fn checked_room(welcome: &Welcome) -> Result<RoomCode, String> {
    check_welcome(welcome)?;
    RoomCode::parse(&welcome.room).map_err(|err| format!("room code: {err}"))
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

/// 끝난 상태의 이유입니다. `heading`은 짧은 제목이고 `reason`은 서버나 접속 방식이 알려 준 이유, 또는 사용자가 확인할 내용입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub heading: &'static str,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::{InputMsg, WireRecordRow};
    use caravan_rules::{Seed, Sim, Tuning};

    fn room() -> RoomCode {
        RoomCode::parse("test").unwrap()
    }

    const SERVER: &str = "ws://127.0.0.1:7777";

    fn request() -> JoinRequest {
        JoinRequest {
            name: PlayerName::parse("KANG").unwrap(),
            room: Some(room()),
        }
    }

    fn connecting() -> Session {
        Session::new(request(), SERVER.to_string(), None)
    }

    fn snapshot(tick: u64) -> Snapshot {
        let mut snapshot = Snapshot::from_sim(&Sim::new(Tuning::default(), Seed(1)));
        snapshot.tick = tick;
        snapshot
    }

    /// 플레이어 번호가 `you`인 입장 안내입니다.
    fn welcome(you: u32) -> ServerMsg {
        let mut sim = Sim::new(Tuning::default(), Seed(1));
        let first = sim.add_player().unwrap();
        let mut welcome = Welcome::from_sim(
            &sim,
            sim.player(first).unwrap(),
            &room(),
            SeatToken([7; 16]),
        );
        welcome.you = you;
        ServerMsg::Welcome(Box::new(welcome))
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
                room: Some("test".to_string()),
                name: "KANG".to_string(),
                token: None,
            })
        );
        assert_eq!(
            session,
            Session::Joining {
                request: request(),
                early_roster: None,
                early_report: None
            }
        );
    }

    #[test]
    fn the_remembered_seat_token_goes_with_the_join() {
        let token = SeatToken([5; 16]);
        let mut session = Session::new(request(), SERVER.to_string(), Some(token));

        assert_eq!(
            session.on_event(ClientEvent::Connected),
            Some(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: Some("test".to_string()),
                name: "KANG".to_string(),
                token: Some(token),
            })
        );
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
        session.on_event(message(ServerMsg::Snapshot(Box::new(snapshot(1)))));
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

        session.on_event(message(ServerMsg::Snapshot(Box::new(snapshot(5)))));
        session.on_event(message(ServerMsg::Snapshot(Box::new(snapshot(4)))));
        session.on_event(message(ServerMsg::Snapshot(Box::new(snapshot(6)))));
        session.advance(0.016);

        assert_eq!(session.newest_snapshot().map(|s| s.tick), Some(6));
        assert!(session.frame().is_some());
    }

    fn roster(revision: u32, names: &[&str]) -> ClientEvent {
        let members = names
            .iter()
            .enumerate()
            .map(|(index, name)| WireMember {
                id: index as u32 + 1,
                name: name.to_string(),
                here: true,
            })
            .collect();
        message(ServerMsg::Roster(WireRoster { revision, members }))
    }

    fn names(session: &Session) -> Vec<&str> {
        let Session::InRoom(joined) = session else {
            panic!("not in a room: {session:?}");
        };
        joined.roster.iter().map(|m| m.name.as_str()).collect()
    }

    fn report(expedition: u32, names: &[&str]) -> WireReport {
        WireReport {
            expedition,
            day: 3,
            elapsed_secs: 872,
            tower_kills: 41,
            rows: names
                .iter()
                .map(|name| WireRecordRow {
                    name: name.to_string(),
                    kills: 88,
                    revives: 2,
                    downs: 1,
                    deposited: 34,
                })
                .collect(),
        }
    }

    fn report_of(session: &Session) -> Option<&WireReport> {
        let Session::InRoom(joined) = session else {
            panic!("not in a room: {session:?}");
        };
        joined.report.as_ref()
    }

    #[test]
    fn the_result_the_server_sends_is_kept_until_the_next_one_replaces_it() {
        let mut session = in_room();
        assert_eq!(
            report_of(&session),
            None,
            "no result before an expedition ends"
        );

        session.on_event(message(ServerMsg::Debrief(report(1, &["KANG", "MIRO"]))));
        assert_eq!(report_of(&session), Some(&report(1, &["KANG", "MIRO"])));

        session.on_event(message(ServerMsg::Debrief(report(2, &["KANG"]))));
        assert_eq!(report_of(&session), Some(&report(2, &["KANG"])));
    }

    #[test]
    fn a_result_that_arrives_before_the_welcome_is_kept_for_the_room() {
        let mut session = connecting();
        session.on_event(ClientEvent::Connected);
        session.on_event(message(ServerMsg::Debrief(report(3, &["KANG", "MIRO"]))));

        session.on_event(message(welcome(1)));

        assert_eq!(report_of(&session), Some(&report(3, &["KANG", "MIRO"])));
    }

    #[test]
    fn a_result_with_a_name_against_the_rules_or_more_rows_than_the_room_holds_is_dropped() {
        let mut session = in_room();
        session.on_event(message(ServerMsg::Debrief(report(1, &["KANG"]))));

        session.on_event(message(ServerMsg::Debrief(report(
            2,
            &["KANG", "not a name!"],
        ))));
        session.on_event(message(ServerMsg::Debrief(report(
            3,
            &["A", "B", "C", "D", "E"],
        ))));

        assert_eq!(report_of(&session), Some(&report(1, &["KANG"])));
    }

    #[test]
    fn a_roster_that_arrives_before_the_welcome_is_kept_for_the_room() {
        let mut session = connecting();
        session.on_event(ClientEvent::Connected);
        session.on_event(roster(1, &["KANG"]));
        session.on_event(roster(2, &["KANG", "MIRO"]));
        session.on_event(roster(1, &["KANG"]));

        session.on_event(message(welcome(1)));

        assert_eq!(names(&session), ["KANG", "MIRO"], "the newest one is used");
    }

    #[test]
    fn a_roster_older_than_the_one_in_use_is_dropped() {
        let mut session = in_room();
        session.on_event(roster(2, &["KANG", "MIRO"]));

        session.on_event(roster(1, &["KANG"]));
        assert_eq!(names(&session), ["KANG", "MIRO"]);

        session.on_event(roster(3, &["MIRO"]));
        assert_eq!(names(&session), ["MIRO"]);
    }

    #[test]
    fn a_roster_with_a_name_against_the_rules_or_more_members_than_the_room_holds_is_dropped() {
        let mut session = in_room();
        session.on_event(roster(1, &["KANG"]));

        session.on_event(roster(2, &["KANG", "not a name!"]));
        session.on_event(roster(3, &["A", "B", "C", "D", "E"]));

        assert_eq!(names(&session), ["KANG"]);
    }

    #[test]
    fn a_snapshot_before_welcome_is_ignored() {
        let mut session = connecting();
        session.on_event(ClientEvent::Connected);
        assert_eq!(
            session.on_event(message(ServerMsg::Snapshot(Box::new(snapshot(1))))),
            None
        );
        assert_eq!(
            session,
            Session::Joining {
                request: request(),
                early_roster: None,
                early_report: None
            }
        );
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
        assert_eq!(session.status_text(), "Join rejected: Room is full (4 / 4)");
    }

    #[test]
    fn every_rejection_reason_is_explained_on_screen() {
        for (reason, expected) in [
            (RejectReason::BadRoomCode, "room code is not valid"),
            (RejectReason::BadName, "Name must be"),
            (RejectReason::VersionMismatch { server: 9 }, "protocol 9"),
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
            session.on_event(message(ServerMsg::Snapshot(Box::new(snapshot(9))))),
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

    #[test]
    fn the_prompt_does_not_hold_back_the_other_keys() {
        let mut session = in_room();
        let Session::InRoom(joined) = &mut session else {
            panic!("in the room");
        };
        joined.leave_prompt = LeavePrompt::Open;
        let busy = Controls {
            attack: true,
            switch_weapon: true,
            right: true,
            ..Controls::default()
        };

        let Some(ClientMsg::Input(input)) = session.input_due(1.0, &busy, None) else {
            panic!("an input is due");
        };

        assert!(
            input.attack && input.switch_weapon,
            "the game does not stop"
        );
        assert!(input.move_dir.x != 0.0 || input.move_dir.y != 0.0);
    }
}
