//! 방의 입장, 퇴장, 정원 초과, Tick 일정을 공개 API(`GameServer`)로만 확인합니다.
//! 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.

mod common;

use std::time::Duration;

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, LoopbackClient, PROTOCOL_VERSION, RejectReason,
    ServerLimits, ServerMsg, Welcome, WireLife, WireNightStep, WirePhase, check_welcome, loopback,
};
use caravan_rules::{PlayerId, Sim, Tuning};
use caravan_server::{GameServer, Notice, ServerError, ServerStats};
use common::{
    assert_near, drain, input, input_with, last_snapshot, player, player_ids, position, rig,
    rig_with, rig_with_limits, snapshots, ticks,
};

#[test]
fn the_first_joiner_is_welcomed_and_then_gets_one_snapshot_per_tick() {
    let mut rig = rig();
    let (mut a, answer) = rig.join("test");
    let ServerMsg::Welcome(welcome) = answer else {
        panic!("expected Welcome, got {answer:?}");
    };
    assert_eq!((welcome.you, welcome.tick_hz, welcome.capacity), (1, 20, 4));

    rig.run_ticks(3);
    let received = snapshots(&mut a);
    assert_eq!(ticks(&received), [1, 2, 3]);
    assert!(received.iter().all(|s| player_ids(s) == [1]));
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
    assert_eq!(player_ids(&seen_by_a[1]), [1, 2]);
}

#[test]
fn clients_in_different_rooms_never_see_each_other() {
    let mut rig = rig();
    let (mut red, _) = rig.join_ok("red");
    let (mut blue, blue_id) = rig.join_ok("blue");
    assert_eq!(blue_id, 1, "each room numbers its own players");

    rig.run_ticks(1);
    assert_eq!(player_ids(&snapshots(&mut red)[0]), [1]);
    assert_eq!(player_ids(&snapshots(&mut blue)[0]), [1]);
}

#[test]
fn room_codes_ignore_case() {
    let mut rig = rig();
    let (_a, _) = rig.join_ok("Test");
    let (mut b, _) = rig.join_ok("TEST");
    rig.run_ticks(1);
    assert_eq!(player_ids(&snapshots(&mut b)[0]), [1, 2]);
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
    client.send(&input(1));
    client.send(&ClientMsg::Join {
        version: PROTOCOL_VERSION,
        room: "test".to_string(),
    });
    let notices = rig.pump();
    assert!(
        matches!(&notices[..], [Notice::LinkClosed { reason, .. }] if reason.contains("before Join")),
        "the closing is reported and the Join after the violation is ignored: {notices:?}"
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
    assert_eq!(player_ids(&snapshots(&mut b)[0]), [2]);
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
fn pump_reports_rooms_created_and_removed_with_the_joins_and_leaves() {
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
            Notice::RoomCreated { room: room.clone() },
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
        ],
        "only the first joiner creates the room"
    );
    drop(first);
    assert_eq!(
        rig.pump(),
        [Notice::Left {
            room: room.clone(),
            player: PlayerId(1),
            members: 1
        }]
    );
    drop(second);
    assert_eq!(
        rig.pump(),
        [
            Notice::Left {
                room: room.clone(),
                player: PlayerId(2),
                members: 0
            },
            Notice::RoomRemoved { room }
        ],
        "the last leaver removes the room"
    );
}

#[test]
fn the_snapshot_rate_follows_tuning_tick_hz() {
    // 기본 접속 한도는 20 Hz까지만 실을 수 있으므로, 더 빠른 Tick은 한도를 함께 올립니다.
    let limits = ServerLimits {
        max_messages_per_sec: 1000,
        ..ServerLimits::default()
    };
    for tick_hz in [20, 40] {
        let tuning = Tuning {
            tick_hz,
            ..Tuning::default()
        };
        let mut rig = rig_with_limits(tuning, limits);
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

#[test]
fn a_link_that_does_not_join_within_the_deadline_is_closed() {
    let mut rig = rig_with_limits(
        Tuning::default(),
        ServerLimits {
            join_deadline: Duration::from_secs(5),
            ..ServerLimits::default()
        },
    );
    let mut idle = rig.connector.connect();
    idle.poll();
    rig.pump();
    let (mut player, _) = rig.join_ok("test");

    rig.now += Duration::from_millis(4_900);
    rig.pump();
    assert!(
        drain(&mut idle).is_empty(),
        "still open just before the deadline"
    );

    rig.now += Duration::from_millis(100);
    let notices = rig.pump();
    assert!(matches!(
        drain(&mut idle)[..],
        [ClientEvent::Disconnected { .. }]
    ));
    assert!(
        matches!(&notices[..], [Notice::LinkClosed { reason, .. }] if reason.contains("no Join within 5s")),
        "the closing is reported with its reason: {notices:?}"
    );
    assert!(
        !drain(&mut player)
            .iter()
            .any(|e| matches!(e, ClientEvent::Disconnected { .. })),
        "a player who joined in time stays connected"
    );
}

#[test]
fn a_flood_of_events_is_spread_over_pumps_and_the_due_tick_still_runs() {
    let mut rig = rig_with_limits(
        Tuning::default(),
        ServerLimits {
            events_per_pump: 3,
            ..ServerLimits::default()
        },
    );
    let (mut player, _) = rig.join_ok("home");

    // 접속마다 이벤트가 두 개(Connected, Join)이므로 20개의 이벤트가 쌓입니다.
    let mut flood: Vec<LoopbackClient> = (0..10)
        .map(|i| {
            let mut client = rig.connector.connect();
            client.poll();
            client.send(&ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: format!("flood-{i}"),
            });
            client
        })
        .collect();

    rig.run_ticks(1);
    assert_eq!(
        ticks(&snapshots(&mut player)),
        [1],
        "the due tick ran despite the backlog"
    );
    let answered = flood
        .iter_mut()
        .map(drain)
        .filter(|events| !events.is_empty())
        .count();
    assert!(
        answered < 10,
        "one pump handled only part of the backlog ({answered} answered)"
    );

    for _ in 0..10 {
        rig.pump();
    }
    let welcomed_later = flood
        .iter_mut()
        .map(drain)
        .filter(|events| {
            events
                .iter()
                .any(|e| matches!(e, ClientEvent::Message(ServerMsg::Welcome { .. })))
        })
        .count();
    assert_eq!(answered + welcomed_later, 10, "no event was lost");
}

#[test]
fn a_joined_link_that_goes_silent_is_closed_and_its_player_leaves_the_room() {
    let mut rig = rig_with_limits(
        Tuning::default(),
        ServerLimits {
            idle_timeout: Duration::from_secs(30),
            ..ServerLimits::default()
        },
    );
    let (mut talker, _) = rig.join_ok("test");
    let (mut silent, _) = rig.join_ok("test");

    // 말하는 클라이언트는 1초마다 입력을 보내고, 조용한 클라이언트는 입장한 뒤 아무것도 보내지 않습니다.
    for seq in 1..=29 {
        talker.send(&input(seq));
        rig.now += Duration::from_secs(1);
        rig.pump();
    }
    assert!(
        !drain(&mut silent)
            .iter()
            .any(|e| matches!(e, ClientEvent::Disconnected { .. })),
        "still connected after 29 s"
    );

    talker.send(&input(30));
    rig.now += Duration::from_secs(1);
    let notices = rig.pump();
    let room = caravan_protocol::RoomCode::parse("test").unwrap();
    assert!(
        matches!(&notices[0], Notice::LinkClosed { reason, .. } if reason.contains("no message for 30s")),
        "the closing is reported with its reason: {notices:?}"
    );
    assert_eq!(
        notices[1..],
        [Notice::Left {
            room,
            player: PlayerId(2),
            members: 1
        }]
    );
    assert!(matches!(
        drain(&mut silent)[..],
        [ClientEvent::Disconnected { .. }]
    ));

    rig.run_ticks(1);
    let last = snapshots(&mut talker).pop().unwrap();
    assert_eq!(
        player_ids(&last),
        [1],
        "the silent player is gone from the room"
    );
}

#[test]
fn the_welcome_reports_the_players_id_the_capacity_and_the_tick_rate_from_the_tuning() {
    let tuning = Tuning {
        max_players: 3,
        tick_hz: 30,
        ..Tuning::default()
    };
    let limits = ServerLimits {
        max_messages_per_sec: 1000,
        ..ServerLimits::default()
    };
    let mut rig = rig_with_limits(tuning, limits);
    let (_a, _) = rig.join("test");
    let (_b, answer) = rig.join("test");
    let ServerMsg::Welcome(welcome) = answer else {
        panic!("expected Welcome, got {answer:?}");
    };
    assert_eq!((welcome.you, welcome.capacity, welcome.tick_hz), (2, 3, 30));
    assert_eq!(
        welcome.world.tower_limit,
        Tuning::default().village.tower_limit
    );
}

#[test]
fn a_player_who_joins_during_a_fight_enters_the_running_expedition_alive() {
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 0.1;
    let mut rig = rig_with(tuning);
    let (mut a, _) = rig.join_ok("test");
    // 낮은 0.1초(2 Tick), 첫 무리는 그 뒤 3초(60 Tick)에 나타나므로 3.2초를 기다리면 싸우는 중입니다.
    rig.run_ticks(64);
    let fighting = last_snapshot(&mut a);
    assert!(
        matches!(
            fighting.phase,
            WirePhase::Night {
                day: 1,
                step: WireNightStep::Fighting { wave: 1 },
                ..
            }
        ),
        "got {:?}",
        fighting.phase
    );

    let (mut b, b_id) = rig.join_ok("test");
    rig.run_ticks(1);
    let first_seen = last_snapshot(&mut b);
    assert_eq!(first_seen.expedition, 1, "no new expedition starts");
    assert!(matches!(first_seen.phase, WirePhase::Night { day: 1, .. }));
    assert!(
        !first_seen.monsters.is_empty(),
        "the newcomer sees the monsters already on the field"
    );
    assert_eq!(
        player(&first_seen, b_id).life,
        WireLife::Alive {
            hp: 100,
            max_hp: 100,
            carried: 0,
            interaction: None,
            build_refusal: None
        }
    );
}

#[test]
fn a_second_join_on_one_link_closes_it_and_removes_its_player() {
    let mut rig = rig();
    let (mut a, _) = rig.join_ok("test");
    let (mut b, _) = rig.join_ok("test");

    a.send(&ClientMsg::Join {
        version: PROTOCOL_VERSION,
        room: "elsewhere".to_string(),
    });
    let notices = rig.pump();
    assert!(
        matches!(drain(&mut a)[..], [ClientEvent::Disconnected { .. }]),
        "the link that broke the protocol is closed"
    );
    assert!(
        matches!(&notices[..], [Notice::LinkClosed { reason, .. }, Notice::Left { members: 1, .. }] if reason.contains("Join again")),
        "the closing is reported and its player leaves the room: {notices:?}"
    );

    rig.run_ticks(1);
    assert_eq!(player_ids(&last_snapshot(&mut b)), [2]);
}

#[test]
fn every_snapshot_carries_a_tick_larger_than_the_one_before() {
    let mut rig = rig();
    let (mut a, _) = rig.join_ok("test");
    let mut seen = Vec::new();
    let mut visitor = None;
    for step in 0..60 {
        // 중간에 사람이 들어오고 나가도 방의 Tick은 끊기지 않습니다.
        if step == 20 {
            visitor = Some(rig.join_ok("test"));
        }
        if step == 40 {
            visitor = None;
        }
        rig.run_ticks(1);
        seen.extend(snapshots(&mut a));
    }
    drop(visitor);
    assert_eq!(ticks(&seen), (1..=60).collect::<Vec<u64>>());
}

#[test]
fn a_players_movement_in_one_room_is_not_visible_in_another_room() {
    let mut rig = rig();
    let (mut red, red_id) = rig.join_ok("red");
    let (mut blue, blue_id) = rig.join_ok("blue");
    assert_eq!(red_id, blue_id, "both rooms call their first player 1");

    red.send(&input_with(1, (1.0, 0.0), false));
    rig.run_ticks(5);

    assert_near(
        position(&last_snapshot(&mut red), red_id).x,
        33.0,
        "the red player moved",
    );
    assert_near(
        position(&last_snapshot(&mut blue), blue_id).x,
        32.0,
        "the blue player did not move",
    );
}

#[test]
fn after_a_player_leaves_the_room_keeps_running_for_the_one_who_stays() {
    let mut rig = rig();
    let (a, a_id) = rig.join_ok("test");
    let (mut b, b_id) = rig.join_ok("test");
    rig.run_ticks(2);
    drop(a);

    b.send(&input_with(1, (0.0, -1.0), false));
    rig.run_ticks(3);
    let snapshot = last_snapshot(&mut b);
    assert_eq!(snapshot.tick, 5, "the room never stopped");
    assert!(
        snapshot.players.iter().all(|p| p.id != a_id),
        "the leaver is gone"
    );
    assert_near(
        position(&snapshot, b_id).y,
        34.5 - 0.6,
        "the one who stays still answers to input",
    );
}

fn try_start(tuning: Tuning, limits: ServerLimits) -> Result<(), ServerError> {
    let (transport, _connector) = loopback();
    GameServer::new(
        Box::new(transport),
        tuning,
        limits,
        std::time::Instant::now(),
    )
    .map(|_| ())
}

#[test]
fn a_server_refuses_a_tick_rate_that_the_link_message_limit_cannot_carry() {
    let limits = ServerLimits::default();
    assert_eq!(try_start(Tuning::default(), limits), Ok(()));

    // 클라이언트는 Tick마다 입력을 하나씩 보내므로, 한도와 같은 Tick 속도는 모든 접속을 끊습니다.
    for tick_hz in [
        limits.max_messages_per_sec + 20,
        limits.max_messages_per_sec,
    ] {
        let tuning = Tuning {
            tick_hz,
            ..Tuning::default()
        };
        assert!(
            matches!(
                try_start(tuning, limits),
                Err(ServerError::InvalidLimits(_))
            ),
            "{tick_hz} Hz against {} messages per second",
            limits.max_messages_per_sec
        );
    }
    let barely_under = Tuning {
        tick_hz: limits.max_messages_per_sec - 1,
        ..Tuning::default()
    };
    assert!(
        matches!(
            try_start(barely_under, limits),
            Err(ServerError::InvalidLimits(_))
        ),
        "a limit just above the tick rate leaves no room for a late burst"
    );

    let roomy = ServerLimits {
        max_messages_per_sec: 1000,
        ..limits
    };
    let fast = Tuning {
        tick_hz: 120,
        ..Tuning::default()
    };
    assert_eq!(try_start(fast, roomy), Ok(()));
}

#[test]
fn the_default_limits_carry_the_default_tick_rate_and_nothing_faster() {
    let limits = ServerLimits::default();
    let default_hz = Tuning::default().tick_hz;
    let at = |tick_hz| Tuning {
        tick_hz,
        ..Tuning::default()
    };
    assert_eq!(try_start(at(default_hz), limits), Ok(()));
    assert!(
        matches!(
            try_start(at(default_hz + 1), limits),
            Err(ServerError::InvalidLimits(_))
        ),
        "a faster tick leaves too little room for inputs that arrive late"
    );
}

/// 이 수치로 입장한 클라이언트가 입장 안내를 확인한 결과입니다.
fn client_verdict(tuning: &Tuning) -> Result<(), String> {
    let mut sim = Sim::new(tuning.clone());
    let first = sim.add_player().expect("an empty room admits a player");
    let me = sim.player(first).expect("the player was just added");
    check_welcome(&Welcome::from_sim(&sim, me))
}

#[test]
fn a_server_starts_exactly_when_a_client_would_accept_its_welcome() {
    let with_map_side = |side: f32| {
        let mut tuning = Tuning::default();
        tuning.map.width = side;
        tuning.map.height = side;
        tuning
    };
    let with_capacity = |max_players: usize| Tuning {
        max_players,
        ..Tuning::default()
    };
    let cases = [
        ("the default numbers", Tuning::default()),
        ("a 128 x 128 map", with_map_side(128.0)),
        ("a 129 x 129 map", with_map_side(129.0)),
        ("a 200 x 200 map", with_map_side(200.0)),
        // 전송할 수 있는 좌표 범위보다 넓은 맵입니다.
        ("a 600 x 600 map", with_map_side(600.0)),
        ("a capacity of 16", with_capacity(16)),
        ("a capacity of 17", with_capacity(17)),
        ("a capacity of 20", with_capacity(20)),
    ];

    let (mut accepted, mut refused) = (0, 0);
    for (name, tuning) in cases {
        let verdict = client_verdict(&tuning);
        let started = try_start(tuning, ServerLimits::default());
        match verdict {
            Ok(()) => {
                accepted += 1;
                assert_eq!(started, Ok(()), "{name}: the clients accept it");
            }
            Err(reason) => {
                refused += 1;
                assert_eq!(
                    started,
                    Err(ServerError::InvalidTuning(reason)),
                    "{name}: the clients refuse it, so the server says why"
                );
            }
        }
    }
    assert!(
        accepted > 1 && refused > 1,
        "the cases cover both answers ({accepted} accepted, {refused} refused)"
    );
}

#[test]
fn a_server_refuses_to_start_with_numbers_that_break_the_rules() {
    let build = |tuning: Tuning| {
        let (transport, _connector) = loopback();
        GameServer::new(
            Box::new(transport),
            tuning,
            ServerLimits::default(),
            std::time::Instant::now(),
        )
        .map(|_| ())
    };
    assert_eq!(build(Tuning::default()), Ok(()));

    let mut too_fast = Tuning::default();
    too_fast.player.speed = 100.0;
    assert!(matches!(
        build(too_fast),
        Err(ServerError::InvalidTuning(why)) if why.contains("player")
    ));
    assert!(matches!(
        build(Tuning {
            tick_hz: 0,
            ..Tuning::default()
        }),
        Err(ServerError::InvalidTuning(_))
    ));
}

#[test]
fn the_stats_count_rooms_members_monsters_and_ticks() {
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 0.1;
    let mut rig = rig_with(tuning);
    let (_a1, _) = rig.join_ok("a");
    let (_a2, _) = rig.join_ok("a");
    let (b, _) = rig.join_ok("b");
    assert_eq!(
        rig.server.stats(),
        ServerStats {
            rooms: 2,
            members: 3,
            monsters: 32,
            ticks: 0
        }
    );

    // 낮이 시작될 때 방마다 낮의 몬스터가 열여섯 마리(채집 지점 여덟 곳에 둘씩) 놓입니다.
    // 낮이 끝나고 첫 무리가 나타난 뒤에는 방마다 밤의 몬스터가 더해집니다.
    rig.run_ticks(64);
    let busy = rig.server.stats();
    assert_eq!((busy.rooms, busy.members, busy.ticks), (2, 3, 64));
    assert!(
        busy.monsters >= 34,
        "the guards and at least one night monster per room, got {}",
        busy.monsters
    );

    drop(b);
    rig.pump();
    let after = rig.server.stats();
    assert_eq!((after.rooms, after.members), (1, 2));
    assert!(after.monsters < busy.monsters, "the empty room is gone");
}
