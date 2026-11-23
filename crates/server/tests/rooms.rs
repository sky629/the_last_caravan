//! 방의 입장, 퇴장, 정원 초과, Tick 일정을 공개 API(`GameServer`)로만 확인합니다.
//! 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.

use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, LoopbackClient, LoopbackConnector, PROTOCOL_VERSION,
    RejectReason, ServerMsg, Snapshot, loopback,
};
use caravan_rules::{PlayerId, Tuning};
use caravan_server::{GameServer, Notice};

struct Rig {
    server: GameServer,
    connector: LoopbackConnector,
    now: Instant,
    period: Duration,
}

fn rig() -> Rig {
    rig_with(Tuning::default())
}

fn rig_with(tuning: Tuning) -> Rig {
    let (transport, connector) = loopback();
    let now = Instant::now();
    let period = Duration::from_secs(1) / tuning.tick_hz;
    Rig {
        server: GameServer::new(Box::new(transport), tuning, now),
        connector,
        now,
        period,
    }
}

impl Rig {
    /// 시각을 바꾸지 않고 서버를 한 번 진행합니다. 입장과 퇴장은 처리되지만 Tick은 실행되지 않습니다.
    fn pump(&mut self) -> Vec<Notice> {
        self.server.pump(self.now)
    }

    /// Tick 간격만큼씩 `ticks`번 시각을 옮기면서 서버를 진행합니다.
    fn run_ticks(&mut self, ticks: u32) -> Vec<Notice> {
        let mut notices = Vec::new();
        for _ in 0..ticks {
            self.now += self.period;
            notices.extend(self.server.pump(self.now));
        }
        notices
    }

    /// 접속하고 `Join`을 보낸 뒤, 서버가 보낸 첫 답을 돌려줍니다.
    fn join(&mut self, room: &str) -> (LoopbackClient, ServerMsg) {
        self.join_with_version(room, PROTOCOL_VERSION)
    }

    fn join_with_version(&mut self, room: &str, version: u32) -> (LoopbackClient, ServerMsg) {
        let mut client = self.connector.connect();
        assert!(matches!(client.poll(), Some(ClientEvent::Connected)));
        client.send(&ClientMsg::Join {
            version,
            room: room.to_string(),
        });
        self.pump();
        match client.poll() {
            Some(ClientEvent::Message(answer)) => (client, answer),
            other => panic!("expected an answer to Join, got {other:?}"),
        }
    }

    fn join_ok(&mut self, room: &str) -> (LoopbackClient, u32) {
        match self.join(room) {
            (client, ServerMsg::Welcome { you, .. }) => (client, you),
            (_, other) => panic!("expected Welcome, got {other:?}"),
        }
    }
}

fn drain(client: &mut LoopbackClient) -> Vec<ClientEvent> {
    std::iter::from_fn(|| client.poll()).collect()
}

fn snapshots(client: &mut LoopbackClient) -> Vec<Snapshot> {
    drain(client)
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Message(ServerMsg::Snapshot(snapshot)) => Some(snapshot),
            _ => None,
        })
        .collect()
}

fn ticks(snapshots: &[Snapshot]) -> Vec<u64> {
    snapshots.iter().map(|s| s.tick).collect()
}

#[test]
fn the_first_joiner_is_welcomed_and_then_gets_one_snapshot_per_tick() {
    let mut rig = rig();
    let (mut a, welcome) = rig.join("test");
    assert_eq!(
        welcome,
        ServerMsg::Welcome {
            you: 1,
            tick_hz: 20
        }
    );

    rig.run_ticks(3);
    let received = snapshots(&mut a);
    assert_eq!(ticks(&received), [1, 2, 3]);
    assert!(received.iter().all(|s| s.players == [1]));
}

#[test]
fn two_clients_in_one_room_see_the_same_ticks_and_both_players() {
    let mut rig = rig();
    let (mut a, _) = rig.join_ok("test");
    let (mut b, b_id) = rig.join_ok("test");
    assert_eq!(b_id, 2);

    rig.run_ticks(2);
    let seen_by_a = snapshots(&mut a);
    let seen_by_b = snapshots(&mut b);
    assert_eq!(seen_by_a, seen_by_b);
    assert_eq!(ticks(&seen_by_a), [1, 2]);
    assert_eq!(seen_by_a[1].players, [1, 2]);
    assert_ne!(
        seen_by_a[0].marker, seen_by_a[1].marker,
        "the marker moves each tick"
    );
}

#[test]
fn clients_in_different_rooms_never_see_each_other() {
    let mut rig = rig();
    let (mut red, _) = rig.join_ok("red");
    let (mut blue, blue_id) = rig.join_ok("blue");
    assert_eq!(blue_id, 1, "each room numbers its own players");

    rig.run_ticks(1);
    assert_eq!(snapshots(&mut red)[0].players, [1]);
    assert_eq!(snapshots(&mut blue)[0].players, [1]);
}

#[test]
fn room_codes_ignore_case() {
    let mut rig = rig();
    let (_a, _) = rig.join_ok("Test");
    let (mut b, _) = rig.join_ok("TEST");
    rig.run_ticks(1);
    assert_eq!(snapshots(&mut b)[0].players, [1, 2]);
}

#[test]
fn a_full_room_rejects_with_its_capacity_and_closes_the_link() {
    let mut rig = rig_with(Tuning {
        max_players: 2,
        ..Tuning::default()
    });
    let _a = rig.join_ok("test");
    let _b = rig.join_ok("test");

    let (mut c, answer) = rig.join("test");
    assert_eq!(
        answer,
        ServerMsg::JoinRejected {
            reason: RejectReason::RoomFull { capacity: 2 }
        }
    );
    assert!(matches!(
        drain(&mut c)[..],
        [ClientEvent::Disconnected { .. }]
    ));
}

#[test]
fn a_bad_room_code_or_another_protocol_version_is_rejected() {
    let mut rig = rig();
    let (mut bad_code, answer) = rig.join("no spaces");
    assert_eq!(
        answer,
        ServerMsg::JoinRejected {
            reason: RejectReason::BadRoomCode
        }
    );
    assert!(matches!(
        drain(&mut bad_code)[..],
        [ClientEvent::Disconnected { .. }]
    ));

    let (_old_build, answer) = rig.join_with_version("test", PROTOCOL_VERSION + 1);
    assert_eq!(
        answer,
        ServerMsg::JoinRejected {
            reason: RejectReason::VersionMismatch {
                server: PROTOCOL_VERSION
            }
        }
    );
}

#[test]
fn input_before_join_closes_the_link_without_a_welcome() {
    let mut rig = rig();
    let mut client = rig.connector.connect();
    client.poll();
    client.send(&ClientMsg::Input { seq: 1 });
    client.send(&ClientMsg::Join {
        version: PROTOCOL_VERSION,
        room: "test".to_string(),
    });
    let notices = rig.pump();
    assert!(
        notices.is_empty(),
        "the Join after the violation is ignored: {notices:?}"
    );
    assert!(matches!(
        drain(&mut client)[..],
        [ClientEvent::Disconnected { .. }]
    ));
}

#[test]
fn a_leaver_disappears_from_the_snapshots_of_the_others() {
    let mut rig = rig();
    let (a, _) = rig.join_ok("test");
    let (mut b, _) = rig.join_ok("test");
    drop(a);
    rig.run_ticks(1);
    assert_eq!(snapshots(&mut b)[0].players, [2]);
}

#[test]
fn the_last_leaver_removes_the_room_and_the_next_joiner_starts_fresh() {
    let mut rig = rig();
    let (a, _) = rig.join_ok("test");
    rig.run_ticks(5);
    drop(a);
    rig.pump();

    let (mut c, c_id) = rig.join_ok("test");
    assert_eq!(c_id, 1);
    rig.run_ticks(1);
    assert_eq!(ticks(&snapshots(&mut c)), [1]);
}

#[test]
fn pump_reports_joins_and_leaves_with_member_counts() {
    let mut rig = rig();
    let mut first = rig.connector.connect();
    let mut second = rig.connector.connect();
    for client in [&mut first, &mut second] {
        client.poll();
        client.send(&ClientMsg::Join {
            version: PROTOCOL_VERSION,
            room: "test".to_string(),
        });
    }
    let room = caravan_protocol::RoomCode::parse("test").unwrap();
    assert_eq!(
        rig.pump(),
        [
            Notice::Joined {
                room: room.clone(),
                player: PlayerId(1),
                members: 1
            },
            Notice::Joined {
                room: room.clone(),
                player: PlayerId(2),
                members: 2
            },
        ]
    );
    drop(first);
    assert_eq!(
        rig.pump(),
        [Notice::Left {
            room,
            player: PlayerId(1),
            members: 1
        }]
    );
}

#[test]
fn the_snapshot_rate_follows_tuning_tick_hz() {
    for tick_hz in [10, 20] {
        let mut rig = rig_with(Tuning {
            tick_hz,
            ..Tuning::default()
        });
        let (mut a, _) = rig.join_ok("test");
        for _ in 0..100 {
            rig.now += Duration::from_millis(10);
            rig.server.pump(rig.now);
        }
        assert_eq!(
            snapshots(&mut a).len(),
            tick_hz as usize,
            "one second at {tick_hz} Hz"
        );
    }
}

#[test]
fn after_a_long_stall_the_server_runs_at_most_five_catch_up_ticks() {
    let mut rig = rig();
    let (mut a, _) = rig.join_ok("test");
    rig.now += Duration::from_secs(10);
    rig.pump();
    assert_eq!(ticks(&snapshots(&mut a)), [1, 2, 3, 4, 5]);

    rig.run_ticks(1);
    assert_eq!(
        ticks(&snapshots(&mut a)),
        [6],
        "the schedule restarts from the stall"
    );
}
