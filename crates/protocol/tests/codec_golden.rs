//! 전송 형식의 바이트를 고정하는 테스트입니다.
//!
//! 기대하는 바이트는 postcard의 규칙으로 직접 계산했습니다.
//! - 열거형의 변형은 선언 순서의 번호(가변 길이 정수)입니다.
//! - 부호 없는 정수는 가변 길이 정수입니다. 7비트씩 기록하고, 뒤에 이어지는 바이트가 있으면 최상위 비트가 1입니다.
//! - `u8`은 그대로 1바이트이고, `bool`은 0이나 1입니다.
//! - `f32`는 IEEE 754 값을 낮은 바이트부터 4바이트로 기록합니다. 0.5는 0x3F000000이므로 `00 00 00 3F`입니다.
//! - `Option`은 없으면 `00`, 있으면 `01` 뒤에 값입니다.
//! - 문자열과 목록은 길이 뒤에 내용이고, 구조체는 필드를 선언 순서대로 이어 붙입니다.
//!
//! 이 테스트가 실패하면 전송 형식이 바뀐 것입니다. 기대 바이트를 고칠 때 `PROTOCOL_VERSION`도 함께 올립니다.

use caravan_protocol::{
    ClientMsg, InputMsg, MonsterKindInfo, PROTOCOL_VERSION, RejectReason, ServerMsg, Snapshot,
    TowerSite, Welcome, WireAngle, WireInteraction, WireLife, WireMonster, WireMonsterKind,
    WireOutcome, WirePhase, WirePlayer, WirePos, WireRect, WireShot, WireTower, WireTowerState,
    WireVec2, WireWagon, WorldInfo, decode_client, decode_server, encode_client, encode_server,
};

fn assert_client_wire(msg: ClientMsg, bytes: &[u8]) {
    assert_eq!(encode_client(&msg), bytes, "encoding of {msg:?}");
    assert_eq!(decode_client(bytes), Ok(msg));
}

fn assert_server_wire(msg: ServerMsg, bytes: &[u8]) {
    assert_eq!(encode_server(&msg), bytes, "encoding of {msg:?}");
    assert_eq!(decode_server(bytes), Ok(msg));
}

#[test]
fn the_protocol_version_matches_these_golden_bytes() {
    assert_eq!(PROTOCOL_VERSION, 2);
}

#[test]
fn join_is_variant_zero_then_version_then_room_text() {
    assert_client_wire(
        ClientMsg::Join {
            version: 2,
            room: "test".to_string(),
        },
        &[0x00, 0x02, 0x04, b't', b'e', b's', b't'],
    );
}

#[test]
fn input_is_the_sequence_number_two_vectors_and_two_buttons() {
    assert_client_wire(
        ClientMsg::Input(InputMsg {
            seq: 300,
            move_dir: WireVec2::new(1.0, -1.0),
            aim: WireVec2::new(0.0, 1.0),
            attack: true,
            interact: false,
        }),
        &[
            0x01, // Input 변형
            0xAC, 0x02, // seq 300
            0x00, 0x00, 0x80, 0x3F, // move_dir.x 1.0
            0x00, 0x00, 0x80, 0xBF, // move_dir.y -1.0
            0x00, 0x00, 0x00, 0x00, // aim.x 0.0
            0x00, 0x00, 0x80, 0x3F, // aim.y 1.0
            0x01, // attack
            0x00, // interact
        ],
    );
}

#[test]
fn welcome_carries_the_fixed_world() {
    let rect = |min_x, min_y, max_x, max_y| WireRect {
        min: WirePos { x: min_x, y: min_y },
        max: WirePos { x: max_x, y: max_y },
    };
    assert_server_wire(
        ServerMsg::Welcome(Welcome {
            you: 2,
            team: 0,
            tick_hz: 20,
            capacity: 4,
            world: WorldInfo {
                size: WirePos { x: 3072, y: 3072 },
                obstacles: vec![rect(512, 640, 1024, 768)],
                wagon: rect(1280, 1408, 1792, 1664),
                towers: vec![TowerSite {
                    id: 1,
                    rect: rect(1920, 1408, 2048, 1536),
                }],
                player_radius: 0.5,
                monster_kinds: vec![MonsterKindInfo {
                    kind: WireMonsterKind::Grunt,
                    radius: 0.25,
                    max_hp: 60,
                }],
            },
        }),
        &[
            0x00, // Welcome 변형
            0x02, // you
            0x00, // team
            0x14, // tick_hz 20
            0x04, // capacity
            // world.size: 3072 = 24 * 128 -> 0x80 0x18
            0x80, 0x18, 0x80, 0x18, //
            0x01, // obstacles: 길이 1
            0x80, 0x04, 0x80, 0x05, 0x80, 0x08, 0x80, 0x06, // 512, 640 - 1024, 768
            0x80, 0x0A, 0x80, 0x0B, 0x80, 0x0E, 0x80, 0x0D, // wagon 10, 11 - 14, 13 타일
            0x01, // towers: 길이 1
            0x01, // tower id
            0x80, 0x0F, 0x80, 0x0B, 0x80, 0x10, 0x80, 0x0C, // 15, 11 - 16, 12 타일
            0x00, 0x00, 0x00, 0x3F, // player_radius 0.5
            0x01, // monster_kinds: 길이 1
            0x00, // Grunt
            0x00, 0x00, 0x80, 0x3E, // radius 0.25
            0x3C, // max_hp 60
        ],
    );
}

#[test]
fn join_rejected_reasons_match_golden_bytes() {
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::RoomFull { capacity: 4 },
        },
        &[0x01, 0x00, 0x04],
    );
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::BadRoomCode,
        },
        &[0x01, 0x01],
    );
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::VersionMismatch { server: 2 },
        },
        &[0x01, 0x02, 0x02],
    );
}

#[test]
fn a_busy_snapshot_matches_golden_bytes() {
    let snapshot = Snapshot {
        tick: 128,
        expedition: 2,
        phase: WirePhase::Fighting { wave: 2 },
        wave_count: 3,
        wagon: WireWagon {
            hp: 450,
            max_hp: 500,
        },
        towers: vec![WireTower {
            id: 1,
            max_hp: 300,
            state: WireTowerState::Standing {
                hp: 120,
                last_shot: Some(WireShot {
                    target: 200,
                    tick: 127,
                }),
            },
        }],
        players: vec![
            WirePlayer {
                id: 1,
                team: 0,
                pos: WirePos { x: 1536, y: 1216 },
                facing: WireAngle(64),
                life: WireLife::Alive {
                    hp: 70,
                    max_hp: 100,
                    interaction: Some(WireInteraction::Reviving {
                        target: 2,
                        progress: 0.5,
                    }),
                },
                last_input_seq: 300,
                last_attack_tick: Some(120),
            },
            WirePlayer {
                id: 2,
                team: 0,
                pos: WirePos { x: 1536, y: 1856 },
                facing: WireAngle(192),
                life: WireLife::Downed {
                    bleed_secs_left: 2.5,
                    revive_progress: Some(0.25),
                },
                last_input_seq: 7,
                last_attack_tick: None,
            },
        ],
        monsters: vec![WireMonster {
            id: 5,
            kind: WireMonsterKind::Grunt,
            pos: WirePos { x: 3072, y: 128 },
            facing: WireAngle(128),
            hp: 60,
        }],
    };
    assert_server_wire(
        ServerMsg::Snapshot(snapshot),
        &[
            0x02, // Snapshot 변형
            0x80, 0x01, // tick 128
            0x02, // expedition
            0x01, 0x02, // phase: Fighting, wave 2
            0x03, // wave_count
            0xC2, 0x03, 0xF4, 0x03, // wagon hp 450, max_hp 500
            0x01, // towers: 길이 1
            0x01, 0xAC, 0x02, // id 1, max_hp 300
            0x00, 0x78, // Standing, hp 120
            0x01, 0xC8, 0x01, 0x7F, // last_shot: 있음, target 200, tick 127
            0x02, // players: 길이 2
            // 플레이어 1: 살아 있고 2번을 부활시키는 중
            0x01, 0x00, // id, team
            0x80, 0x0C, 0xC0, 0x09, // pos 1536, 1216
            0x40, // facing 64
            0x00, 0x46, 0x64, // Alive, hp 70, max_hp 100
            0x01, 0x00, 0x02, 0x00, 0x00, 0x00,
            0x3F, // interaction: 있음, Reviving, target 2, progress 0.5
            0xAC, 0x02, // last_input_seq 300
            0x01, 0x78, // last_attack_tick: 있음, 120
            // 플레이어 2: 쓰러졌고 25%까지 부활됨
            0x02, 0x00, // id, team
            0x80, 0x0C, 0xC0, 0x0E, // pos 1536, 1856
            0xC0, // facing 192
            0x01, 0x00, 0x00, 0x20, 0x40, // Downed, bleed_secs_left 2.5
            0x01, 0x00, 0x00, 0x80, 0x3E, // revive_progress: 있음, 0.25
            0x07, // last_input_seq
            0x00, // last_attack_tick: 없음
            0x01, // monsters: 길이 1
            0x05, 0x00, // id 5, Grunt
            0x80, 0x18, 0x80, 0x01, // pos 3072, 128
            0x80, // facing 128
            0x3C, // hp 60
        ],
    );
}

/// 지금까지의 테스트에 나오지 않은 변형들을 한 번씩 보냅니다. 앞부분(tick 1, expedition 1, wave_count 3)은 모두 같습니다.
fn quiet_snapshot(phase: WirePhase) -> Snapshot {
    Snapshot {
        tick: 1,
        expedition: 1,
        phase,
        wave_count: 3,
        wagon: WireWagon { hp: 0, max_hp: 0 },
        towers: vec![],
        players: vec![],
        monsters: vec![],
    }
}

/// `quiet_snapshot`의 바이트에서 phase 자리를 `phase`로 채운 것입니다.
fn quiet_bytes(phase: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x02, 0x01, 0x01]; // Snapshot, tick 1, expedition 1
    bytes.extend_from_slice(phase);
    bytes.extend_from_slice(&[0x03, 0x00, 0x00, 0x00, 0x00, 0x00]); // wave_count 3, wagon 0/0, 빈 목록 세 개
    bytes
}

#[test]
fn every_phase_matches_golden_bytes() {
    assert_server_wire(
        ServerMsg::Snapshot(quiet_snapshot(WirePhase::Intermission {
            next_wave: 1,
            secs_left: 3.0,
        })),
        &quiet_bytes(&[0x00, 0x01, 0x00, 0x00, 0x40, 0x40]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(quiet_snapshot(WirePhase::Fighting { wave: 3 })),
        &quiet_bytes(&[0x01, 0x03]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(quiet_snapshot(WirePhase::Over {
            outcome: WireOutcome::Victory,
            secs_left: 5.0,
        })),
        &quiet_bytes(&[0x02, 0x00, 0x00, 0x00, 0xA0, 0x40]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(quiet_snapshot(WirePhase::Over {
            outcome: WireOutcome::Defeat,
            secs_left: 5.0,
        })),
        &quiet_bytes(&[0x02, 0x01, 0x00, 0x00, 0xA0, 0x40]),
    );
}

#[test]
fn a_repairing_player_a_dead_player_and_a_destroyed_tower_match_golden_bytes() {
    let mut snapshot = quiet_snapshot(WirePhase::Fighting { wave: 1 });
    snapshot.towers = vec![WireTower {
        id: 1,
        max_hp: 300,
        state: WireTowerState::Destroyed,
    }];
    snapshot.players = vec![
        WirePlayer {
            id: 3,
            team: 0,
            pos: WirePos { x: 0, y: 0 },
            facing: WireAngle(0),
            life: WireLife::Alive {
                hp: 100,
                max_hp: 100,
                interaction: Some(WireInteraction::Repairing { tower: 1 }),
            },
            last_input_seq: 0,
            last_attack_tick: None,
        },
        WirePlayer {
            id: 4,
            team: 0,
            pos: WirePos { x: 0, y: 0 },
            facing: WireAngle(0),
            life: WireLife::Dead,
            last_input_seq: 0,
            last_attack_tick: None,
        },
    ];
    assert_server_wire(
        ServerMsg::Snapshot(snapshot),
        &[
            0x02, 0x01, 0x01, // Snapshot, tick 1, expedition 1
            0x01, 0x01, // phase: Fighting, wave 1
            0x03, // wave_count
            0x00, 0x00, // wagon
            0x01, // towers: 길이 1
            0x01, 0xAC, 0x02, 0x01, // id 1, max_hp 300, Destroyed
            0x02, // players: 길이 2
            0x03, 0x00, 0x00, 0x00, 0x00, // id 3, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, // Alive, hp 100, max_hp 100
            0x01, 0x01, 0x01, // interaction: 있음, Repairing, tower 1
            0x00, 0x00, // last_input_seq 0, last_attack_tick 없음
            0x04, 0x00, 0x00, 0x00, 0x00, // id 4, team, pos 0 0, facing 0
            0x02, // Dead
            0x00, 0x00, // last_input_seq 0, last_attack_tick 없음
            0x00, // monsters: 길이 0
        ],
    );
}

#[test]
fn bytes_that_are_not_exactly_one_message_are_rejected() {
    assert!(decode_client(&[]).is_err(), "empty frame");
    assert!(decode_client(&[0x07]).is_err(), "unknown variant");
    assert!(decode_client(&[0x01, 0xAC]).is_err(), "truncated varint");
    let input = encode_client(&ClientMsg::Input(InputMsg::default()));
    assert!(
        decode_client(&input[..input.len() - 1]).is_err(),
        "truncated"
    );
    let mut trailing = input;
    trailing.push(0x00);
    assert!(decode_client(&trailing).is_err(), "trailing byte");
    assert!(
        decode_server(&[0x00, 0x01, 0x04, b't']).is_err(),
        "client bytes on the server channel"
    );
}
