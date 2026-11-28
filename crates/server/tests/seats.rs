//! 자리의 유지와 재접속을 공개 API(`GameServer`)로만 확인합니다. 클라이언트가 보는 메시지와 서버가 알리는 일(`Notice`)만 봅니다.
//! 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.

mod common;

use caravan_protocol::{
    ClientEvent, ClientMsg, ClientTransport, SeatToken, ServerMsg, WireLife, WireMember, WirePhase,
};
use caravan_rules::Tuning;
use caravan_server::{Notice, RandomSource};
use common::{
    drain, hear, input, input_with, last_roster, last_snapshot, player, press_ready, rig,
    rig_in_expedition, rig_with_random, rosters,
};

/// 몬스터가 곧 마차 둘레로 몰려오는 원정입니다. 낮이 0.1초이고, 첫 무리가 0.1초 뒤에 나타나서 20마리가 0.05초마다 한 마리씩 나옵니다.
fn raid_tuning() -> Tuning {
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 0.1;
    tuning.nights.first_wave_delay_secs = 0.1;
    tuning.nights.spawn_interval_secs = 0.05;
    tuning.nights.plans = vec![caravan_rules::NightPlan {
        waves: vec![caravan_rules::Wave {
            packs: vec![caravan_rules::Pack::always(
                caravan_rules::MonsterKind::Grunt,
                20,
            )],
        }],
    }];
    tuning
}

fn here_of(roster: &[WireMember], id: u32) -> bool {
    roster
        .iter()
        .find(|member| member.id == id)
        .unwrap_or_else(|| panic!("player {id} is not in {roster:?}"))
        .here
}

/// 접속이 끊긴 자리가 60초 동안 남고, 같은 표식으로 돌아오면 같은 플레이어로 이어집니다.
#[test]
fn a_dropped_player_rejoins_with_the_token_as_the_same_player() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    miro.send(&input_with(1, (1.0, 0.0), false));
    let mut seq = rig.run_ticks_talking(&mut [&mut kang], 1, 10);
    let before = *player(&last_snapshot(&mut miro), miro_welcome.you);

    rig.drop_link(miro);
    seq = rig.run_ticks_talking(&mut [&mut kang], seq, rig.ticks(1.0));

    let heard = hear(&mut kang);
    assert_eq!(
        heard.last_roster()[1],
        WireMember {
            id: miro_welcome.you,
            name: "MIRO".to_string(),
            here: false
        },
        "the others see that MIRO is away"
    );
    let held = *player(heard.last_snapshot(), miro_welcome.you);
    assert_eq!(
        held.life, before.life,
        "the held character is unhurt, still alive, with the same HP"
    );
    assert_eq!(
        (held.loadout, held.pos.x >= before.pos.x),
        (before.loadout, true),
        "the held character keeps its weapons and stays where its last input left it"
    );

    let (mut back, back_welcome) = rig.join_welcomed("night", "MIRO", Some(miro_welcome.token));
    assert_eq!(back_welcome.you, miro_welcome.you, "the same player number");
    assert_eq!(back_welcome.token, miro_welcome.token, "the same token");
    rig.run_ticks_talking(&mut [&mut kang, &mut back], seq, 1);
    let returned = *player(&last_snapshot(&mut back), miro_welcome.you);
    assert_eq!(returned.pos, held.pos, "at the place it was held");
    assert_eq!(returned.loadout, held.loadout);
    assert!(
        hear(&mut kang)
            .last_roster()
            .iter()
            .all(|member| member.here),
        "everybody is here again"
    );
}

/// 몬스터가 몰려와도 끊긴 플레이어는 노려지지 않고 피해를 받지 않으며, 같은 자리에 있는 접속 중인 플레이어는 맞습니다.
#[test]
fn a_held_character_is_not_hurt_while_monsters_hurt_the_one_next_to_it() {
    let mut rig = rig_in_expedition(raid_tuning());
    let (mut kang, kang_welcome) = rig.join_welcomed("raid", "KANG", None);
    let (miro, miro_welcome) = rig.join_welcomed("raid", "MIRO", None);
    let max_hp = Tuning::default().player.max_hp;
    rig.drop_link(miro);

    let mut seq = 1;
    let mut hurt = false;
    for _ in 0..60 {
        seq = rig.run_ticks_talking(&mut [&mut kang], seq, 20);
        let snapshot = last_snapshot(&mut kang);
        let kang_hp = match player(&snapshot, kang_welcome.you).life {
            WireLife::Alive { hp, .. } => hp,
            WireLife::Downed { .. } | WireLife::Dead => 0,
        };
        if kang_hp < max_hp {
            hurt = true;
            assert!(
                matches!(
                    player(&snapshot, miro_welcome.you).life,
                    WireLife::Alive { hp, .. } if hp == max_hp
                ),
                "the held character stays unhurt while the monsters hurt KANG: {snapshot:?}"
            );
            break;
        }
    }
    assert!(hurt, "the monsters reached KANG within a minute");
}

/// 틀린 표식, 표식이 없는 입장, 다른 방의 표식은 유지 중인 자리를 가져가지 못하고 새 플레이어로 들어옵니다.
#[test]
fn a_wrong_token_or_none_joins_as_a_new_player_and_leaves_the_held_seat_alone() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    rig.drop_link(miro);

    let (_wrong, wrong) = rig.join_welcomed("night", "MIRO", Some(SeatToken([9; 16])));
    let (_none, none) = rig.join_welcomed("night", "MIRO", None);
    let (_elsewhere, elsewhere) = rig.join_welcomed("other", "MIRO", Some(miro_welcome.token));

    assert_ne!(wrong.you, miro_welcome.you);
    assert_ne!(none.you, miro_welcome.you);
    assert_ne!(wrong.you, none.you);
    assert_eq!(elsewhere.you, 1, "the first player of another room");
    for welcome in [&wrong, &none, &elsewhere] {
        assert_ne!(
            welcome.token, miro_welcome.token,
            "a new seat gets a token of its own"
        );
    }
    let (mut back, back_welcome) = rig.join_welcomed("night", "MIRO", Some(miro_welcome.token));
    assert_eq!(
        back_welcome.you, miro_welcome.you,
        "the held seat was still there"
    );
    let roster = last_roster(&mut back);
    assert_eq!(roster.len(), 3, "the held seat and two new players");
}

/// 접속한 사람이 한 명도 없으면 방이 진행되지 않고, 돌아오면 멈춘 곳에서 이어집니다.
#[test]
fn a_room_with_nobody_here_does_not_tick_and_resumes_where_it_stopped() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut solo, welcome) = rig.join_welcomed("solo", "KANG", None);
    rig.run_ticks_talking(&mut [&mut solo], 1, 10);
    let stopped = last_snapshot(&mut solo);

    rig.drop_link(solo);
    rig.run_ticks(rig.ticks(30.0));
    assert_eq!(
        rig.server.stats().ticks,
        10 + 600,
        "the server clock ran on, the room did not"
    );

    let (mut back, _) = rig.join_welcomed("solo", "KANG", Some(welcome.token));
    rig.run_ticks(1);
    let resumed = last_snapshot(&mut back);
    assert_eq!(
        resumed.tick,
        stopped.tick + 1,
        "the next tick, not 600 later"
    );
    let (
        WirePhase::Day {
            secs_left: before, ..
        },
        WirePhase::Day {
            secs_left: after, ..
        },
    ) = (&stopped.phase, &resumed.phase)
    else {
        panic!(
            "the room is still in the first day: {:?} {:?}",
            stopped.phase, resumed.phase
        );
    };
    assert!(
        (before - after - 0.05).abs() < 1e-3,
        "the day went on by one tick: {before} then {after}"
    );
}

/// 접속은 열려 있어도 입력이 3초 동안 오지 않으면 자리를 비운 것으로 보고, 같은 접속에서 입력이 다시 오면 이어서 합니다.
#[test]
fn a_silent_open_link_is_away_after_three_seconds_and_back_on_input() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    drain(&mut miro);

    let seq = rig.run_ticks_talking(&mut [&mut kang], 1, rig.ticks(2.9));
    assert!(
        rosters(&mut kang)
            .iter()
            .all(|roster| roster.members.iter().all(|member| member.here)),
        "nobody was away before three seconds"
    );

    let seq = rig.run_ticks_talking(&mut [&mut kang], seq, rig.ticks(0.2));
    let roster = last_roster(&mut kang);
    assert!(
        !here_of(&roster, miro_welcome.you),
        "away after three seconds"
    );
    assert!(
        !drain(&mut miro)
            .iter()
            .any(|event| matches!(event, ClientEvent::Disconnected { .. })),
        "the link stays open"
    );

    miro.send(&input(1));
    rig.run_ticks_talking(&mut [&mut kang], seq, 1);
    assert!(
        here_of(&last_roster(&mut kang), miro_welcome.you),
        "an input on the same link brings the player back"
    );
}

/// 서버가 아직 접속 중으로 보는 자리에 같은 표식으로 들어오면, 새 접속이 자리를 이어받습니다.
/// 이전 접속은 `SeatTakenOver`를 받고, 서버는 그 접속을 닫지 않고 클라이언트가 이유를 읽고 닫기를 기다립니다.
#[test]
fn the_same_token_on_a_live_seat_takes_it_over_and_tells_the_old_link_without_closing_it() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut old, welcome) = rig.join_welcomed("night", "KANG", None);
    let (mut other, _) = rig.join_welcomed("night", "MIRO", None);
    rig.run_ticks_talking(&mut [&mut old, &mut other], 1, 5);
    drain(&mut old);

    let (mut new, taken) = rig.join_welcomed("night", "KANG", Some(welcome.token));

    assert_eq!(taken.you, welcome.you, "the same player");
    let told = hear(&mut old);
    assert!(told.taken_over, "the old link is told why");
    assert!(
        !told.closed,
        "and is left open for the client to read it first"
    );
    let notices = rig.run_ticks(1);
    assert!(
        !notices
            .iter()
            .any(|notice| matches!(notice, Notice::Held { .. } | Notice::Left { .. })),
        "the seat was taken over, not given up: {notices:?}"
    );

    // 이전 접속의 입력은 자리에 들어가지 않고, 그 접속이 닫혀도 새 접속의 자리는 그대로입니다.
    old.send(&input_with(50, (-1.0, 0.0), false));
    let x_before = player(&last_snapshot(&mut new), welcome.you).pos.x;
    rig.run_ticks_talking(&mut [&mut new, &mut other], 1, 10);
    assert_eq!(
        player(&last_snapshot(&mut new), welcome.you).pos.x,
        x_before,
        "the old link's input does not move the character"
    );
    let notices = {
        drop(old);
        rig.pump()
    };
    assert!(
        notices.is_empty(),
        "the old link going away is not news: {notices:?}"
    );
    let roster = {
        rig.run_ticks_talking(&mut [&mut new, &mut other], 11, 5);
        hear(&mut other).rosters
    };
    assert!(
        roster
            .last()
            .is_none_or(|r| r.members.iter().all(|m| m.here)),
        "the player is still here"
    );
    assert_eq!(rig.server.stats().members, 2);
}

/// 이전 접속이 닫지 않으면 서버가 입장 기한에 닫습니다.
#[test]
fn a_taken_over_link_that_stays_open_is_closed_at_the_join_deadline() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut old, welcome) = rig.join_welcomed("night", "KANG", None);
    let (mut new, _) = rig.join_welcomed("night", "KANG", Some(welcome.token));
    assert!(hear(&mut old).taken_over);

    let next_seq = rig.run_ticks_talking(&mut [&mut new], 1, rig.ticks(4.5));
    assert!(!hear(&mut old).closed, "still open before the deadline");

    let mut closed = false;
    let mut reasons = Vec::new();
    for seq in next_seq..next_seq + rig.ticks(1.0) {
        new.send(&input(seq));
        for notice in rig.run_ticks(1) {
            if let Notice::LinkClosed { reason, .. } = notice {
                reasons.push(reason);
            }
        }
        closed |= hear(&mut old).closed;
    }
    assert!(closed, "closed by the server after five seconds");
    assert!(
        reasons
            .iter()
            .any(|reason| reason.contains("told why it cannot stay")),
        "{reasons:?}"
    );
    assert_eq!(
        rig.server.stats().members,
        1,
        "the seat is still the new link's"
    );
}

/// 60초가 지나면 자리가 끝나고(퇴장과 같음), 끝난 자리의 표식은 쓸 수 없습니다.
#[test]
fn a_seat_ends_sixty_seconds_after_the_last_message_and_its_token_is_dead() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    let mut seq = rig.run_ticks_talking(&mut [&mut kang, &mut miro], 1, 5);
    rig.drop_link(miro);

    let mut notices = Vec::new();
    for _ in 0..rig.ticks(58.0) {
        seq = rig.run_ticks_talking(&mut [&mut kang], seq, 1);
        notices.extend(rig.pump());
    }
    assert_eq!(rig.server.stats().members, 2, "held after 58 seconds");
    assert!(
        notices.iter().all(|n| !matches!(n, Notice::Left { .. })),
        "{notices:?}"
    );

    let mut left = None;
    for _ in 0..rig.ticks(4.0) {
        for notice in rig.run_ticks(1) {
            if matches!(notice, Notice::Left { .. }) {
                left = Some(notice);
            }
        }
        rig.run_ticks_talking(&mut [&mut kang], seq, 1);
        seq += 1;
    }
    assert!(
        matches!(left, Some(Notice::Left { player, members: 1, .. }) if player.0 == miro_welcome.you),
        "{left:?}"
    );
    assert_eq!(rig.server.stats().members, 1);
    let heard = hear(&mut kang);
    assert_eq!(heard.last_roster().len(), 1, "MIRO is off the roster");
    assert!(
        !heard
            .last_snapshot()
            .players
            .iter()
            .any(|p| p.id == miro_welcome.you),
        "and out of the world"
    );

    let (_late, late) = rig.join_welcomed("night", "MIRO", Some(miro_welcome.token));
    assert_ne!(
        late.you, miro_welcome.you,
        "the token of an ended seat is no key"
    );
}

/// 서버가 30초 무응답으로 접속을 닫아도 자리는 마지막 입력부터 60초가 지나야 끝납니다.
#[test]
fn the_sixty_seconds_count_from_the_last_input_even_when_the_server_closes_the_silent_link() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, _) = rig.join_welcomed("night", "MIRO", None);
    let mut seq = rig.run_ticks_talking(&mut [&mut kang, &mut miro], 1, 5);

    let mut closed_at = None;
    let mut left_at = None;
    for second in 1..=70 {
        seq = rig.run_ticks_talking(&mut [&mut kang], seq, rig.ticks(1.0));
        if hear(&mut miro).closed && closed_at.is_none() {
            closed_at = Some(second);
        }
        if rig.server.stats().members == 1 && left_at.is_none() {
            left_at = Some(second);
        }
    }

    let closed_at = closed_at.expect("the server closes the silent link");
    let left_at = left_at.expect("the seat ends");
    assert!(
        (29..=31).contains(&closed_at),
        "closed after about 30 s, at {closed_at}"
    );
    assert!(
        (59..=61).contains(&left_at),
        "ended after about 60 s, at {left_at}"
    );
}

/// 한 사람만 있던 방은 그 사람의 자리가 끝나면 사라집니다.
#[test]
fn a_room_whose_only_seat_ended_is_removed() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (solo, _) = rig.join_welcomed("solo", "KANG", None);
    rig.drop_link(solo);
    assert_eq!(rig.server.stats().rooms, 1, "held");

    let notices = rig.run_ticks(rig.ticks(60.0) + 2);

    assert_eq!(rig.server.stats().rooms, 0);
    let room = caravan_protocol::RoomCode::parse("solo").unwrap();
    let ends: Vec<&Notice> = notices
        .iter()
        .filter(|n| matches!(n, Notice::Left { .. } | Notice::RoomRemoved { .. }))
        .collect();
    assert_eq!(
        ends,
        [
            &Notice::Left {
                room: room.clone(),
                player: caravan_rules::PlayerId(1),
                members: 0
            },
            &Notice::RoomRemoved { room }
        ]
    );
}

/// 나가기(`Leave`)는 자리를 곧바로 내놓고, 그 표식은 더 이상 쓸 수 없습니다.
#[test]
fn leave_gives_up_the_seat_at_once_and_closes_the_link() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (mut miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    rig.run_ticks_talking(&mut [&mut kang, &mut miro], 1, 3);
    drain(&mut kang);

    miro.send(&ClientMsg::Leave);
    let notices = rig.pump();

    let room = caravan_protocol::RoomCode::parse("night").unwrap();
    assert_eq!(
        notices,
        [Notice::Left {
            room,
            player: caravan_rules::PlayerId(2),
            members: 1
        }],
        "no waiting for sixty seconds"
    );
    assert!(hear(&mut miro).closed, "the leaver's link is closed");
    assert_eq!(hear(&mut kang).last_roster().len(), 1);
    let (_again, again) = rig.join_welcomed("night", "MIRO", Some(miro_welcome.token));
    assert_ne!(
        again.token, miro_welcome.token,
        "the token of a seat given up opens nothing"
    );
}

/// 유지 중인 자리도 정원에 들어갑니다. 정원이 찼어도 표식이 있는 사람은 돌아올 수 있습니다.
#[test]
fn a_held_seat_counts_against_the_capacity_but_its_owner_can_come_back() {
    let mut rig = rig_in_expedition(Tuning {
        max_players: 2,
        ..Tuning::default()
    });
    let (_kang, _) = rig.join_welcomed("night", "KANG", None);
    let (miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    rig.drop_link(miro);

    let (_third, answer) = rig.join_as(Some("night"), "NEWCOMER");
    assert_eq!(
        answer,
        ServerMsg::JoinRejected {
            reason: caravan_protocol::RejectReason::RoomFull { capacity: 2 }
        },
        "the held seat still takes up a place"
    );

    let (_back, back) = rig.join_welcomed("night", "MIRO", Some(miro_welcome.token));
    assert_eq!(
        back.you, miro_welcome.you,
        "the owner returns to a full room"
    );
}

/// 끊긴 자리는 야영지의 「모두 준비」를 막지 않고, 준비하지 않은 사람이 돌아오면 세던 시간이 취소됩니다.
#[test]
fn a_held_seat_does_not_hold_up_the_ready_and_an_unready_return_cancels_the_countdown() {
    let mut rig = rig();
    let (mut kang, _) = rig.join_welcomed("camp", "KANG", None);
    let (miro, miro_welcome) = rig.join_welcomed("camp", "MIRO", None);
    kang.send(&press_ready(1));
    let mut seq = rig.run_ticks_talking(&mut [&mut kang], 2, 5);
    assert!(
        matches!(
            last_snapshot(&mut kang).phase,
            WirePhase::Camp {
                starting_in: None,
                ..
            }
        ),
        "MIRO is here and not ready"
    );

    rig.drop_link(miro);
    seq = rig.run_ticks_talking(&mut [&mut kang], seq, 5);
    let counting = last_snapshot(&mut kang);
    assert!(
        matches!(
            counting.phase,
            WirePhase::Camp {
                starting_in: Some(_),
                ..
            }
        ),
        "the one who is here is ready: {:?}",
        counting.phase
    );

    let (_back, _) = rig.join_welcomed("camp", "MIRO", Some(miro_welcome.token));
    rig.run_ticks_talking(&mut [&mut kang], seq, 1);
    assert!(
        matches!(
            last_snapshot(&mut kang).phase,
            WirePhase::Camp {
                starting_in: None,
                ..
            }
        ),
        "MIRO is back and not ready, so the countdown is gone"
    );
}

/// 끊긴 자리가 있는 방에서 남은 사람들이 모두 준비하면 원정이 시작됩니다.
#[test]
fn the_expedition_starts_without_the_player_who_is_away() {
    let mut rig = rig();
    let (mut kang, _) = rig.join_welcomed("camp", "KANG", None);
    let (miro, _) = rig.join_welcomed("camp", "MIRO", None);
    rig.drop_link(miro);
    kang.send(&press_ready(1));

    rig.run_ticks_talking(&mut [&mut kang], 2, rig.ticks(3.5));

    assert!(matches!(
        last_snapshot(&mut kang).phase,
        WirePhase::Day { .. }
    ));
}

/// 자리의 시간이 흐르는 동안 명단의 번호는 바뀔 때마다 커지고, 끊김과 돌아옴이 명단으로 알려집니다.
#[test]
fn the_roster_tells_who_is_away_and_its_numbers_only_grow() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (mut kang, _) = rig.join_welcomed("night", "KANG", None);
    let (miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    let mut seq = rig.run_ticks_talking(&mut [&mut kang], 1, 3);

    rig.drop_link(miro);
    seq = rig.run_ticks_talking(&mut [&mut kang], seq, 3);
    let (mut back, back_welcome) = rig.join_welcomed("night", "MIRO", Some(miro_welcome.token));
    rig.run_ticks_talking(&mut [&mut kang, &mut back], seq, 3);

    let all = hear(&mut kang).rosters;
    let shape: Vec<(u32, Vec<bool>)> = all
        .iter()
        .map(|roster| {
            (
                roster.revision,
                roster.members.iter().map(|m| m.here).collect(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (1, vec![true]),
            (2, vec![true, true]),
            (3, vec![true, false]),
            (4, vec![true, true]),
        ],
        "joined, joined, away, back"
    );
    let greeted = hear(&mut back).rosters;
    assert_eq!(
        greeted.first().map(|roster| roster.revision),
        Some(4),
        "the returning link is given the roster as it is, after its welcome"
    );
    assert_eq!(back_welcome.you, miro_welcome.you);
}

/// 접속이 끊기고 돌아오는 것이 알림으로 나옵니다.
#[test]
fn the_server_reports_a_held_seat_and_a_return() {
    let mut rig = rig_in_expedition(Tuning::default());
    let (_kang, _) = rig.join_welcomed("night", "KANG", None);
    let (miro, miro_welcome) = rig.join_welcomed("night", "MIRO", None);
    let room = caravan_protocol::RoomCode::parse("night").unwrap();

    let held = {
        drop(miro);
        rig.pump()
    };
    assert_eq!(
        held,
        [Notice::Held {
            room: room.clone(),
            player: caravan_rules::PlayerId(miro_welcome.you)
        }]
    );

    let mut back = rig.connector.connect();
    back.poll();
    back.send(&common::join_msg_with_token(
        caravan_protocol::PROTOCOL_VERSION,
        Some("night"),
        "MIRO",
        Some(miro_welcome.token),
    ));
    assert_eq!(
        rig.pump(),
        [Notice::Returned {
            room,
            player: caravan_rules::PlayerId(miro_welcome.you)
        }]
    );
}

/// 서버는 시작할 때 보호가 자리의 끝보다 먼저 시작하는지, 조용한 접속이 자리보다 먼저 닫히는지 확인합니다.
#[test]
fn a_server_refuses_limits_that_leave_a_seat_over_before_its_link_or_its_protection() {
    use caravan_protocol::{ServerLimits, loopback};
    use caravan_server::{GameServer, RoomSeeds, ServerError};
    use std::time::Duration;

    let try_start = |limits: ServerLimits| {
        let (transport, _connector) = loopback();
        GameServer::new(
            Box::new(transport),
            Tuning::default(),
            limits,
            RoomSeeds::Fixed(caravan_rules::Seed(1)),
            common::random(),
            std::time::Instant::now(),
        )
        .map(|_| ())
    };
    assert_eq!(try_start(ServerLimits::default()), Ok(()));

    let slow_close = ServerLimits {
        idle_timeout: Duration::from_secs(61),
        ..ServerLimits::default()
    };
    assert!(matches!(
        try_start(slow_close),
        Err(ServerError::InvalidLimits(why)) if why.contains("idle_timeout")
    ));
    let at_the_hold = ServerLimits {
        idle_timeout: Duration::from_secs(60),
        ..ServerLimits::default()
    };
    assert_eq!(try_start(at_the_hold), Ok(()), "equal is allowed");

    for (away_after, seat_hold) in [(60, 60), (61, 60), (0, 60)] {
        let limits = ServerLimits {
            away_after: Duration::from_secs(away_after),
            seat_hold: Duration::from_secs(seat_hold),
            ..ServerLimits::default()
        };
        assert!(
            matches!(
                try_start(limits),
                Err(ServerError::InvalidLimits(why)) if why.contains("away_after")
            ),
            "away_after {away_after} s, seat_hold {seat_hold} s"
        );
    }
}

/// 같은 바이트만 주는 원천입니다.
struct Constant(u8);

impl RandomSource for Constant {
    fn fill(&mut self, bytes: &mut [u8]) {
        bytes.fill(self.0);
    }
}

/// 자리의 표식과 새 방의 코드는 서버를 만들 때 넘긴 무작위 원천의 바이트로 만들어집니다. 서버가 따로 만들어 내는 값이 없습니다.
#[test]
fn the_token_and_the_room_code_are_made_from_the_bytes_the_random_source_gives() {
    let mut rig = rig_with_random(
        Tuning::default(),
        caravan_protocol::ServerLimits::default(),
        caravan_server::RoomSeeds::Fixed(caravan_rules::Seed(1)),
        Box::new(Constant(171)),
    );

    let (_kang, welcome) = rig.join_welcomed("night", "KANG", None);
    assert_eq!(welcome.token, SeatToken([171; 16]));

    let (_miro, answer) = rig.join_as(None, "MIRO");
    let ServerMsg::Welcome(made) = answer else {
        panic!("expected Welcome, got {answer:?}");
    };
    // 문자표의 17번째 글자(171 % 31 = 16)가 다섯 번 나옵니다.
    assert_eq!(made.room, "ttttt");
}
