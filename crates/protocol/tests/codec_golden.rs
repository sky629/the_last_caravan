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
    BossInfo, ClientMsg, InputMsg, MonsterKindInfo, NodeInfo, PROTOCOL_VERSION, RackInfo,
    RejectReason, SeatToken, ServerMsg, Snapshot, StructureKindInfo, WeaponInfo, Welcome, WireAim,
    WireAngle, WireAttack, WireBoss, WireBossAct, WireBuffs, WireBuildRefusal, WireCannotBuild,
    WireChest, WireDelivery, WireGroundWeapon, WireInteraction, WireLife, WireLoadout, WireMember,
    WireMonster, WireMonsterKind, WireMonsterStatus, WireNightStep, WireNode, WireOutcome,
    WirePhase, WirePile, WirePlayer, WirePos, WireProjectile, WirePrompt, WireRecordRow, WireRect,
    WireRepairTarget, WireReport, WireRoster, WireShot, WireStruck, WireStructureKind, WireTile,
    WireTower, WireTowerState, WireVec2, WireWagon, WireWeaponKind, WorldInfo, decode_client,
    decode_server, encode_client, encode_server,
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
    assert_eq!(PROTOCOL_VERSION, 19);
}

/// 값이 눈에 띄도록 0x10부터 이어지는 바이트로 만든 표식입니다.
const TOKEN: SeatToken = SeatToken([
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
]);

#[test]
fn join_is_variant_zero_then_version_then_the_optional_room_then_the_name_then_the_optional_token()
{
    assert_client_wire(
        ClientMsg::Join {
            version: PROTOCOL_VERSION,
            room: Some("test".to_string()),
            name: "KANG".to_string(),
            token: None,
        },
        &[
            0x00, // Join 변형
            0x13, // version
            0x01, 0x04, b't', b'e', b's', b't', // room: 있음, 글자 4개
            0x04, b'K', b'A', b'N', b'G', // name: 글자 4개
            0x00, // token: 없음
        ],
    );
    assert_client_wire(
        ClientMsg::Join {
            version: PROTOCOL_VERSION,
            room: None,
            name: "KANG".to_string(),
            token: Some(TOKEN),
        },
        &[
            0x00, 0x13, 0x00, 0x04, b'K', b'A', b'N', b'G', //
            0x01, // token: 있음
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, // 길이 없이 16바이트
            0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
        ],
    );
}

#[test]
fn leave_is_client_variant_two_and_nothing_else() {
    assert_client_wire(ClientMsg::Leave, &[0x02]);
}

#[test]
fn seat_taken_over_is_server_variant_four_and_nothing_else() {
    assert_server_wire(ServerMsg::SeatTakenOver, &[0x04]);
}

#[test]
fn input_is_the_sequence_number_two_vectors_and_ten_buttons() {
    assert_client_wire(
        ClientMsg::Input(InputMsg {
            seq: 300,
            move_dir: WireVec2::new(1.0, -1.0),
            aim: WireVec2::new(0.0, 1.0),
            attack: true,
            interact: false,
            interact_pressed: true,
            build: true,
            sprint: true,
            switch_weapon: true,
            toggle_ready: true,
            confirm: true,
            cycle_tower: true,
            drop_weapon: true,
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
            0x01, // interact_pressed
            0x01, // build
            0x01, // sprint
            0x01, // switch_weapon
            0x01, // toggle_ready
            0x01, // confirm
            0x01, // cycle_tower
            0x01, // drop_weapon
        ],
    );
}

#[test]
fn welcome_carries_the_fixed_world() {
    let rect = |min_x, min_y, max_x, max_y| WireRect {
        min: WirePos { x: min_x, y: min_y },
        max: WirePos { x: max_x, y: max_y },
    };
    let welcome = Welcome {
        you: 2,
        team: 0,
        tick_hz: 20,
        capacity: 4,
        room: "k7m2q".to_string(),
        token: TOKEN,
        world: WorldInfo {
            size: WirePos { x: 3072, y: 3072 },
            obstacles: vec![rect(512, 640, 1024, 768)],
            wagon: rect(1280, 1408, 1792, 1664),
            day_count: 3,
            tower_limit: 40,
            carry_capacity: 10,
            node_amount: 15,
            nodes: vec![NodeInfo {
                id: 1,
                tile: WireTile { x: 9, y: 22 },
            }],
            player_radius: 0.5,
            monster_kinds: vec![
                MonsterKindInfo {
                    kind: WireMonsterKind::Grunt,
                    radius: 0.25,
                    max_hp: 60,
                },
                MonsterKindInfo {
                    kind: WireMonsterKind::Scarecrow,
                    radius: 0.25,
                    max_hp: 60,
                },
            ],
            weapons: vec![
                WeaponInfo {
                    kind: WireWeaponKind::Sword,
                    delivery: WireDelivery::Arc {
                        reach: 1.0,
                        half_angle_deg: 60.0,
                    },
                },
                WeaponInfo {
                    kind: WireWeaponKind::Bow,
                    delivery: WireDelivery::Projectile,
                },
                WeaponInfo {
                    kind: WireWeaponKind::Greatsword,
                    delivery: WireDelivery::Arc {
                        reach: 1.3,
                        half_angle_deg: 90.0,
                    },
                },
                WeaponInfo {
                    kind: WireWeaponKind::Spear,
                    delivery: WireDelivery::Arc {
                        reach: 2.2,
                        half_angle_deg: 15.0,
                    },
                },
                WeaponInfo {
                    kind: WireWeaponKind::Crossbow,
                    delivery: WireDelivery::Projectile,
                },
                WeaponInfo {
                    kind: WireWeaponKind::Rifle,
                    delivery: WireDelivery::Hitscan,
                },
            ],
            racks: vec![
                RackInfo {
                    kind: WireWeaponKind::Sword,
                    pos: WirePos { x: 512, y: 4672 },
                },
                RackInfo {
                    kind: WireWeaponKind::Bow,
                    pos: WirePos { x: 666, y: 4672 },
                },
            ],
            structure_kinds: vec![
                StructureKindInfo {
                    kind: WireStructureKind::Watchtower,
                    max_hp: 300,
                    build_cost: 10,
                    range: 6.0,
                    covers_area: false,
                },
                StructureKindInfo {
                    kind: WireStructureKind::SlowTower,
                    max_hp: 300,
                    build_cost: 15,
                    range: 4.0,
                    covers_area: true,
                },
            ],
            boss: Some(BossInfo {
                slam_radius: 2.5,
                charge_width: 2.0,
            }),
        },
    };
    let bytes: &[u8] = &[
        0x00, // Welcome 변형
        0x02, // you
        0x00, // team
        0x14, // tick_hz 20
        0x04, // capacity
        0x05, b'k', b'7', b'm', b'2', b'q', // room: 글자 5개
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, // token: 길이 없이 16바이트
        0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
        // world.size: 3072 = 24 * 128 -> 0x80 0x18
        0x80, 0x18, 0x80, 0x18, //
        0x01, // obstacles: 길이 1
        0x80, 0x04, 0x80, 0x05, 0x80, 0x08, 0x80, 0x06, // 512, 640 - 1024, 768
        0x80, 0x0A, 0x80, 0x0B, 0x80, 0x0E, 0x80, 0x0D, // wagon 10, 11 - 14, 13 타일
        0x03, // day_count
        0x28, // tower_limit 40
        0x0A, // carry_capacity 10
        0x0F, // node_amount 15
        0x01, // nodes: 길이 1
        0x01, 0x09, 0x16, // id 1, tile 9 22
        0x00, 0x00, 0x00, 0x3F, // player_radius 0.5
        0x02, // monster_kinds: 길이 2
        0x00, // Grunt
        0x00, 0x00, 0x80, 0x3E, // radius 0.25
        0x3C, // max_hp 60
        0x01, // Scarecrow
        0x00, 0x00, 0x80, 0x3E, // radius 0.25
        0x3C, // max_hp 60
        0x06, // weapons: 길이 6
        0x00, 0x00, // Sword, Arc
        0x00, 0x00, 0x80, 0x3F, // reach 1.0
        0x00, 0x00, 0x70, 0x42, // half_angle_deg 60.0
        0x01, 0x01, // Bow, Projectile
        0x02, 0x00, // Greatsword, Arc
        0x66, 0x66, 0xA6, 0x3F, // reach 1.3
        0x00, 0x00, 0xB4, 0x42, // half_angle_deg 90.0
        0x03, 0x00, // Spear, Arc
        0xCD, 0xCC, 0x0C, 0x40, // reach 2.2
        0x00, 0x00, 0x70, 0x41, // half_angle_deg 15.0
        0x04, 0x01, // Crossbow, Projectile
        0x05, 0x02, // Rifle, Hitscan
        0x02, // racks: 길이 2
        0x00, // Sword
        0x80, 0x04, 0xC0, 0x24, // pos 512, 4672
        0x01, // Bow
        0x9A, 0x05, 0xC0, 0x24, // pos 666, 4672
        0x02, // structure_kinds: 길이 2
        0x00, // Watchtower
        0xAC, 0x02, // max_hp 300
        0x0A, // build_cost 10
        0x00, 0x00, 0xC0, 0x40, // range 6.0
        0x00, // covers_area 거짓
        0x01, // SlowTower
        0xAC, 0x02, // max_hp 300
        0x0F, // build_cost 15
        0x00, 0x00, 0x80, 0x40, // range 4.0
        0x01, // covers_area 참
        0x01, // boss: 있음
        0x00, 0x00, 0x20, 0x40, // slam_radius 2.5
        0x00, 0x00, 0x00, 0x40, // charge_width 2.0
    ];
    assert_server_wire(ServerMsg::Welcome(Box::new(welcome.clone())), bytes);

    // 보스가 없는 설정은 마지막 한 바이트만 0이고 나머지는 같습니다.
    let mut without = welcome;
    without.world.boss = None;
    let mut shorter = bytes[..bytes.len() - 9].to_vec();
    shorter.push(0x00);
    assert_server_wire(ServerMsg::Welcome(Box::new(without)), &shorter);
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
            reason: RejectReason::VersionMismatch { server: 3 },
        },
        &[0x01, 0x02, 0x03],
    );
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::BadName,
        },
        &[0x01, 0x03],
    );
    assert_server_wire(
        ServerMsg::JoinRejected {
            reason: RejectReason::ServerFull,
        },
        &[0x01, 0x04],
    );
}

#[test]
fn the_roster_is_variant_three_then_the_revision_then_the_members_with_their_presence_in_order() {
    assert_server_wire(
        ServerMsg::Roster(WireRoster {
            revision: 5,
            members: vec![
                WireMember {
                    id: 1,
                    name: "KANG".to_string(),
                    here: true,
                },
                WireMember {
                    id: 2,
                    name: "MIRO".to_string(),
                    here: false,
                },
            ],
        }),
        &[
            0x03, // Roster 변형
            0x05, // 명단의 번호
            0x02, // 목록: 길이 2
            0x01, 0x04, b'K', b'A', b'N', b'G',
            0x01, // 번호 1, 이름 글자 4개, 자리에 있음
            0x02, 0x04, b'M', b'I', b'R', b'O',
            0x00, // 번호 2, 이름 글자 4개, 자리를 비움
        ],
    );
}

#[test]
fn the_report_is_variant_five_then_the_expedition_the_summary_and_a_row_for_each_player() {
    assert_server_wire(
        ServerMsg::Debrief(WireReport {
            expedition: 2,
            day: 3,
            elapsed_secs: 872,
            tower_kills: 41,
            rows: vec![
                WireRecordRow {
                    name: "KANG".to_string(),
                    kills: 88,
                    revives: 2,
                    downs: 1,
                    deposited: 34,
                },
                WireRecordRow {
                    name: "MIRO".to_string(),
                    kills: 300,
                    revives: 0,
                    downs: 3,
                    deposited: 52,
                },
            ],
        }),
        &[
            0x05, // Debrief 변형
            0x02, // 몇 번째 원정인지
            0x03, // 버틴 날
            0xE8, 0x06, // 걸린 시간 872초
            0x29, // 포탑의 처치 41
            0x02, // 목록: 길이 2
            0x04, b'K', b'A', b'N', b'G', 0x58, 0x02, 0x01,
            0x22, // 이름, 처치 88, 부활 2, 쓰러짐 1, 넣은 자원 34
            0x04, b'M', b'I', b'R', b'O', 0xAC, 0x02, 0x00, 0x03,
            0x34, // 이름, 처치 300, 부활 0, 쓰러짐 3, 넣은 자원 52
        ],
    );
}

#[test]
fn a_busy_snapshot_matches_golden_bytes() {
    let snapshot = Snapshot {
        tick: 128,
        expedition: 2,
        phase: WirePhase::Night {
            day: 2,
            wave_count: 3,
            step: WireNightStep::Fighting { wave: 2 },
        },
        wagon: WireWagon {
            hp: 450,
            max_hp: 500,
        },
        stash: 7,
        towers: vec![WireTower {
            id: 1,
            tile: WireTile { x: 15, y: 11 },
            kind: WireStructureKind::SlowTower,
            state: WireTowerState::Standing {
                hp: 120,
                last_shot: Some(WireShot {
                    target: 200,
                    tick: 127,
                }),
            },
        }],
        nodes: vec![
            WireNode { id: 1, amount: 15 },
            WireNode { id: 2, amount: 0 },
        ],
        piles: vec![WirePile {
            id: 3,
            pos: WirePos { x: 1664, y: 1408 },
            amount: 6,
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
                    carried: 4,
                    stamina: 60,
                    interaction: Some(WireInteraction::Reviving {
                        target: 2,
                        progress: 0.5,
                    }),
                    build_refusal: None,
                    buffs: WireBuffs {
                        haste: true,
                        swift: false,
                    },
                },
                last_input_seq: 300,
                last_attack: Some(WireAttack {
                    tick: 120,
                    weapon: WireWeaponKind::Rifle,
                    trace_end: Some(WirePos { x: 2048, y: 1216 }),
                }),
                loadout: WireLoadout {
                    held: WireWeaponKind::Bow,
                    spare: Some(WireWeaponKind::Sword),
                },
                prompt: Some(WirePrompt::TakeWeapon(WireWeaponKind::Spear)),
                build_kind: WireStructureKind::SlowTower,
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
                last_attack: Some(WireAttack {
                    tick: 118,
                    weapon: WireWeaponKind::Sword,
                    trace_end: None,
                }),
                loadout: WireLoadout {
                    held: WireWeaponKind::Sword,
                    spare: None,
                },
                prompt: None,
                build_kind: WireStructureKind::Watchtower,
            },
        ],
        monsters: vec![WireMonster {
            id: 5,
            status: WireMonsterStatus::plain(WireMonsterKind::Grunt),
            pos: WirePos { x: 3072, y: 128 },
            facing: WireAngle(128),
            hp: 60,
        }],
        chests: vec![WireChest {
            id: 2,
            tile: WireTile { x: 47, y: 32 },
        }],
        ground_weapons: vec![WireGroundWeapon {
            id: 4,
            pos: WirePos { x: 1664, y: 1408 },
            kind: WireWeaponKind::Greatsword,
        }],
        projectiles: vec![WireProjectile {
            id: 9,
            pos: WirePos { x: 1600, y: 1280 },
            facing: WireAngle(32),
        }],
    };
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(snapshot)),
        &[
            0x02, // Snapshot 변형
            0x80, 0x01, // tick 128
            0x02, // expedition
            0x01, 0x02, 0x03, 0x01, 0x02, // phase: Night, day 2, 3 waves, Fighting, wave 2
            0xC2, 0x03, 0xF4, 0x03, // wagon hp 450, max_hp 500
            0x07, // stash
            0x01, // towers: 길이 1
            0x01, 0x0F, 0x0B, 0x01, // id 1, tile 15 11, SlowTower
            0x01, 0x78, // Standing, hp 120
            0x01, 0xC8, 0x01, 0x7F, // last_shot: 있음, target 200, tick 127
            0x02, // nodes: 길이 2
            0x01, 0x0F, // id 1, amount 15
            0x02, 0x00, // id 2, amount 0
            0x01, // piles: 길이 1
            0x03, 0x80, 0x0D, 0x80, 0x0B, 0x06, // id 3, pos 1664 1408, amount 6
            0x02, // players: 길이 2
            // 플레이어 1: 살아 있고 2번을 부활시키는 중
            0x01, 0x00, // id, team
            0x80, 0x0C, 0xC0, 0x09, // pos 1536, 1216
            0x40, // facing 64
            0x00, 0x46, 0x64, 0x04, 0x3C, // Alive, hp 70, max_hp 100, carried 4, stamina 60
            0x01, 0x00, 0x02, 0x00, 0x00, 0x00,
            0x3F, // interaction: 있음, Reviving, target 2, progress 0.5
            0x00, // build_refusal: 없음
            0x01, 0x00, // buffs: 공격 속도 있음, 이동 속도 없음
            0xAC, 0x02, // last_input_seq 300
            0x01, 0x78, 0x05, // last_attack: 있음, tick 120, 공격한 무기 Rifle
            0x01, 0x80, 0x10, 0xC0, 0x09, // trace_end: 있음, 2048, 1216
            0x01, 0x01, 0x00, // loadout: 든 무기 Bow, 다른 칸 있음 Sword
            0x01, 0x01, 0x03, // prompt: 있음, TakeWeapon(변형 1), Spear
            0x01, // build_kind: SlowTower
            // 플레이어 2: 쓰러졌고 25%까지 부활됨
            0x02, 0x00, // id, team
            0x80, 0x0C, 0xC0, 0x0E, // pos 1536, 1856
            0xC0, // facing 192
            0x01, 0x00, 0x00, 0x20, 0x40, // Downed, bleed_secs_left 2.5
            0x01, 0x00, 0x00, 0x80, 0x3E, // revive_progress: 있음, 0.25
            0x07, // last_input_seq
            0x01, 0x76, 0x00, // last_attack: 있음, tick 118, 공격한 무기 Sword
            0x00, // trace_end: 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x01, // monsters: 길이 1
            0x05, 0x00, // id 5, Grunt
            0x80, 0x18, 0x80, 0x01, // pos 3072, 128
            0x80, // facing 128
            0x3C, // hp 60
            0x01, // chests: 길이 1
            0x02, 0x2F, 0x20, // id 2, tile 47 32
            0x01, // ground_weapons: 길이 1
            0x04, 0x80, 0x0D, 0x80, 0x0B, 0x02, // id 4, pos 1664 1408, Greatsword
            0x01, // projectiles: 길이 1
            0x09, 0xC0, 0x0C, 0x80, 0x0A, // id 9, pos 1600, 1280
            0x20, // facing 32
        ],
    );
}

/// 지금까지의 테스트에 나오지 않은 변형들을 한 번씩 보냅니다. 단계를 뺀 나머지(tick 1, expedition 1, 빈 마차, 빈 목록)는 모두 같습니다.
fn quiet_snapshot(phase: WirePhase) -> Snapshot {
    Snapshot {
        tick: 1,
        expedition: 1,
        phase,
        wagon: WireWagon { hp: 0, max_hp: 0 },
        stash: 0,
        towers: vec![],
        nodes: vec![],
        piles: vec![],
        players: vec![],
        monsters: vec![],
        chests: vec![],
        ground_weapons: vec![],
        projectiles: vec![],
    }
}

/// `quiet_snapshot`의 바이트에서 phase 자리를 `phase`로 채운 것입니다.
fn quiet_bytes(phase: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x02, 0x01, 0x01]; // Snapshot, tick 1, expedition 1
    bytes.extend_from_slice(phase);
    bytes.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]); // wagon 0/0, stash 0, 빈 목록 여덟 개
    bytes
}

#[test]
fn the_five_tower_kinds_are_the_bytes_zero_to_four_in_the_order_of_choosing() {
    let mut snapshot = quiet_snapshot(WirePhase::Day {
        day: 1,
        secs_left: 3.0,
    });
    snapshot.towers = [
        WireStructureKind::Watchtower,
        WireStructureKind::SlowTower,
        WireStructureKind::StunTower,
        WireStructureKind::HasteTower,
        WireStructureKind::SwiftTower,
    ]
    .into_iter()
    .zip(1..)
    .map(|(kind, id)| WireTower {
        id,
        tile: WireTile { x: 2, y: 3 },
        kind,
        state: WireTowerState::Building { progress: 0.5 },
    })
    .collect();
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(snapshot)),
        &[
            0x02, 0x01, 0x01, // Snapshot, tick 1, expedition 1
            0x00, 0x01, 0x00, 0x00, 0x40, 0x40, // phase: Day, day 1, 3.0초
            0x00, 0x00, // wagon
            0x00, // stash
            0x05, // towers: 길이 5
            0x01, 0x02, 0x03, 0x00, // id 1, tile 2 3, Watchtower
            0x00, 0x00, 0x00, 0x00, 0x3F, // Building, progress 0.5
            0x02, 0x02, 0x03, 0x01, // id 2, tile 2 3, SlowTower
            0x00, 0x00, 0x00, 0x00, 0x3F, //
            0x03, 0x02, 0x03, 0x02, // id 3, tile 2 3, StunTower
            0x00, 0x00, 0x00, 0x00, 0x3F, //
            0x04, 0x02, 0x03, 0x03, // id 4, tile 2 3, HasteTower
            0x00, 0x00, 0x00, 0x00, 0x3F, //
            0x05, 0x02, 0x03, 0x04, // id 5, tile 2 3, SwiftTower
            0x00, 0x00, 0x00, 0x00, 0x3F, //
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 빈 목록 일곱 개
        ],
    );
}

#[test]
fn every_phase_matches_golden_bytes() {
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Day {
            day: 2,
            secs_left: 3.0,
        }))),
        &quiet_bytes(&[0x00, 0x02, 0x00, 0x00, 0x40, 0x40]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Night {
            day: 1,
            wave_count: 2,
            step: WireNightStep::Waiting {
                next_wave: 2,
                secs_left: 4.0,
            },
        }))),
        &quiet_bytes(&[0x01, 0x01, 0x02, 0x00, 0x02, 0x00, 0x00, 0x80, 0x40]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Night {
            day: 3,
            wave_count: 3,
            step: WireNightStep::Fighting { wave: 3 },
        }))),
        &quiet_bytes(&[0x01, 0x03, 0x03, 0x01, 0x03]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Dawn {
            next_day: 2,
            secs_left: 4.0,
        }))),
        &quiet_bytes(&[0x02, 0x02, 0x00, 0x00, 0x80, 0x40]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Camp {
            ready: vec![1, 2],
            starting_in: Some(3.0),
            struck: vec![WireStruck {
                monster: 1,
                tick: 5,
                amount: 25,
            }],
        }))),
        // Camp, 준비 2명(1, 2), 남은 3.0초, 맞은 허수아비 1개(번호 1, Tick 5, 피해 25)
        &quiet_bytes(&[
            0x03, 0x02, 0x01, 0x02, 0x01, 0x00, 0x00, 0x40, 0x40, 0x01, 0x01, 0x05, 0x19,
        ]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Debrief {
            outcome: WireOutcome::Victory,
            secs_left: 5.0,
            confirmed: vec![],
        }))),
        &quiet_bytes(&[0x04, 0x00, 0x00, 0x00, 0xA0, 0x40, 0x00]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Debrief {
            outcome: WireOutcome::Defeat,
            secs_left: 5.0,
            confirmed: vec![2],
        }))),
        &quiet_bytes(&[0x04, 0x01, 0x00, 0x00, 0xA0, 0x40, 0x01, 0x02]),
    );
}

#[test]
fn a_site_a_refusal_a_builder_two_repairers_a_gatherer_an_opener_and_a_dead_player_match_golden_bytes()
 {
    let mut snapshot = quiet_snapshot(WirePhase::Night {
        day: 1,
        wave_count: 3,
        step: WireNightStep::Fighting { wave: 1 },
    });
    snapshot.towers = vec![WireTower {
        id: 1,
        tile: WireTile { x: 2, y: 3 },
        kind: WireStructureKind::StunTower,
        state: WireTowerState::Building { progress: 0.5 },
    }];
    let alive_with = |id, interaction, build_refusal, buffs| WirePlayer {
        id,
        team: 0,
        pos: WirePos { x: 0, y: 0 },
        facing: WireAngle(0),
        life: WireLife::Alive {
            hp: 100,
            max_hp: 100,
            carried: 3,
            stamina: 100,
            interaction,
            build_refusal,
            buffs,
        },
        last_input_seq: 0,
        last_attack: None,
        loadout: WireLoadout {
            held: WireWeaponKind::Sword,
            spare: None,
        },
        prompt: None,
        build_kind: WireStructureKind::Watchtower,
    };
    let alive = |id, interaction, build_refusal| {
        alive_with(id, interaction, build_refusal, WireBuffs::NONE)
    };
    let haste = WireBuffs {
        haste: true,
        swift: false,
    };
    let swift = WireBuffs {
        haste: false,
        swift: true,
    };
    snapshot.players = vec![
        alive(
            3,
            Some(WireInteraction::Repairing {
                target: WireRepairTarget::Tower { tower: 1 },
                stash_short: false,
            }),
            None,
        ),
        WirePlayer {
            prompt: Some(WirePrompt::OpenChest),
            build_kind: WireStructureKind::StunTower,
            ..alive_with(
                4,
                None,
                Some(WireBuildRefusal {
                    tile: WireTile { x: 4, y: 5 },
                    why: WireCannotBuild::WouldBlockPath,
                }),
                haste,
            )
        },
        alive_with(5, Some(WireInteraction::Building { tower: 1 }), None, swift),
        alive_with(
            7,
            Some(WireInteraction::Gathering { node: 2 }),
            None,
            WireBuffs {
                haste: true,
                swift: true,
            },
        ),
        alive(
            8,
            Some(WireInteraction::Repairing {
                target: WireRepairTarget::Wagon,
                stash_short: true,
            }),
            None,
        ),
        alive(
            9,
            Some(WireInteraction::Opening {
                chest: 3,
                progress: 0.5,
            }),
            None,
        ),
        WirePlayer {
            life: WireLife::Dead,
            loadout: WireLoadout {
                held: WireWeaponKind::Bow,
                spare: None,
            },
            ..alive(6, None, None)
        },
    ];
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(snapshot)),
        &[
            0x02, 0x01, 0x01, // Snapshot, tick 1, expedition 1
            0x01, 0x01, 0x03, 0x01, 0x01, // phase: Night, day 1, 3 waves, Fighting, wave 1
            0x00, 0x00, // wagon
            0x00, // stash
            0x01, // towers: 길이 1
            0x01, 0x02, 0x03, 0x02, // id 1, tile 2 3, StunTower
            0x00, 0x00, 0x00, 0x00, 0x3F, // Building, progress 0.5
            0x00, // nodes: 길이 0
            0x00, // piles: 길이 0
            0x07, // players: 길이 7
            0x03, 0x00, 0x00, 0x00, 0x00, // id 3, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, 0x03, 0x64, // Alive, hp 100, max_hp 100, carried 3, stamina 100
            0x01, 0x01, 0x00, 0x01,
            0x00, // interaction: 있음, Repairing, Tower, tower 1, stash_short 거짓
            0x00, // build_refusal: 없음
            0x00, 0x00, // buffs: 없음
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x04, 0x00, 0x00, 0x00, 0x00, // id 4, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, 0x03, 0x64, // Alive, hp 100, max_hp 100, carried 3, stamina 100
            0x00, // interaction: 없음
            0x01, 0x04, 0x05, 0x04, // build_refusal: 있음, tile 4 5, WouldBlockPath
            0x01, 0x00, // buffs: 공격 속도 있음, 이동 속도 없음
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x01, 0x00, // prompt: 있음, OpenChest
            0x02, // build_kind: StunTower
            0x05, 0x00, 0x00, 0x00, 0x00, // id 5, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, 0x03, 0x64, // Alive, hp 100, max_hp 100, carried 3, stamina 100
            0x01, 0x02, 0x01, // interaction: 있음, Building, tower 1
            0x00, // build_refusal: 없음
            0x00, 0x01, // buffs: 공격 속도 없음, 이동 속도 있음
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x07, 0x00, 0x00, 0x00, 0x00, // id 7, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, 0x03, 0x64, // Alive, hp 100, max_hp 100, carried 3, stamina 100
            0x01, 0x03, 0x02, // interaction: 있음, Gathering, node 2
            0x00, // build_refusal: 없음
            0x01, 0x01, // buffs: 공격 속도 있음, 이동 속도 있음
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x08, 0x00, 0x00, 0x00, 0x00, // id 8, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, 0x03, 0x64, // Alive, hp 100, max_hp 100, carried 3, stamina 100
            0x01, 0x01, 0x01, 0x01, // interaction: 있음, Repairing, Wagon, stash_short 참
            0x00, // build_refusal: 없음
            0x00, 0x00, // buffs: 없음
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x09, 0x00, 0x00, 0x00, 0x00, // id 9, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, 0x03, 0x64, // Alive, hp 100, max_hp 100, carried 3, stamina 100
            0x01, 0x04, 0x03, 0x00, 0x00, 0x00,
            0x3F, // interaction: 있음, Opening, chest 3, progress 0.5
            0x00, // build_refusal: 없음
            0x00, 0x00, // buffs: 없음
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x06, 0x00, 0x00, 0x00, 0x00, // id 6, team, pos 0 0, facing 0
            0x02, // Dead
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x01,
            0x00, // loadout: 든 무기 Bow, 다른 칸 없음 (쓰러지거나 죽어도 무기를 가집니다)
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x00, // monsters: 길이 0
            0x00, // chests: 길이 0
            0x00, // ground_weapons: 길이 0
            0x00, // projectiles: 길이 0
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

#[test]
fn the_boss_warning_and_the_boss_step_match_golden_bytes() {
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Night {
            day: 3,
            wave_count: 3,
            step: WireNightStep::Warning { secs_left: 5.0 },
        }))),
        &quiet_bytes(&[0x01, 0x03, 0x03, 0x02, 0x00, 0x00, 0xA0, 0x40]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Night {
            day: 3,
            wave_count: 3,
            step: WireNightStep::Boss(WireBoss {
                max_hp: 12_000,
                aim: WireAim::Player(2),
                act: WireBossAct::Slam {
                    center: WirePos { x: 640, y: 1280 },
                    progress: 0.5,
                },
            }),
        }))),
        // Night, 날 3, 무리 3개, 단계 Boss, 최대 HP 12000, 노리는 대상 플레이어 2, 내려찍기(중심 640 1280, 진행도 0.5)
        &quiet_bytes(&[
            0x01, 0x03, 0x03, 0x03, 0xE0, 0x5D, 0x00, 0x02, 0x01, 0x80, 0x05, 0x80, 0x0A, 0x00,
            0x00, 0x00, 0x3F,
        ]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(quiet_snapshot(WirePhase::Night {
            day: 3,
            wave_count: 3,
            step: WireNightStep::Boss(WireBoss {
                max_hp: 3_000,
                aim: WireAim::Wagon,
                act: WireBossAct::Walking,
            }),
        }))),
        // 최대 HP 3000, 노리는 대상 마차, 걷는 중
        &quiet_bytes(&[0x01, 0x03, 0x03, 0x03, 0xB8, 0x17, 0x01, 0x00]),
    );
}

#[test]
fn a_charge_a_dash_and_a_roar_are_boss_act_variants_two_three_and_four() {
    let boss_snapshot = |act| {
        quiet_snapshot(WirePhase::Night {
            day: 3,
            wave_count: 3,
            step: WireNightStep::Boss(WireBoss {
                max_hp: 3_000,
                aim: WireAim::Wagon,
                act,
            }),
        })
    };
    // Night, 날 3, 무리 3개, 단계 Boss, 최대 HP 3000, 노리는 대상 마차
    let head = [0x01, 0x03, 0x03, 0x03, 0xB8, 0x17, 0x01];
    let with_act = |act: &[u8]| quiet_bytes(&[&head[..], act].concat());

    assert_server_wire(
        ServerMsg::Snapshot(Box::new(boss_snapshot(WireBossAct::Charge {
            from: WirePos { x: 128, y: 1280 },
            to: WirePos { x: 1408, y: 1280 },
            progress: 0.5,
        }))),
        // 돌진 예고(시작 128 1280, 끝 1408 1280, 진행도 0.5)
        &with_act(&[
            0x02, 0x80, 0x01, 0x80, 0x0A, 0x80, 0x0B, 0x80, 0x0A, 0x00, 0x00, 0x00, 0x3F,
        ]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(boss_snapshot(WireBossAct::Dash {
            from: WirePos { x: 128, y: 1280 },
            to: WirePos { x: 1408, y: 1280 },
        }))),
        // 달리는 중(시작 128 1280, 끝 1408 1280)
        &with_act(&[0x03, 0x80, 0x01, 0x80, 0x0A, 0x80, 0x0B, 0x80, 0x0A]),
    );
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(boss_snapshot(WireBossAct::Roar {
            progress: 0.25,
        }))),
        // 포효(진행도 0.25)
        &with_act(&[0x04, 0x00, 0x00, 0x80, 0x3E]),
    );
}

#[test]
fn a_pushed_player_is_interaction_variant_five_and_nothing_else() {
    let mut snapshot = quiet_snapshot(WirePhase::Night {
        day: 1,
        wave_count: 3,
        step: WireNightStep::Fighting { wave: 1 },
    });
    snapshot.players = vec![WirePlayer {
        id: 3,
        team: 0,
        pos: WirePos { x: 0, y: 0 },
        facing: WireAngle(0),
        life: WireLife::Alive {
            hp: 100,
            max_hp: 100,
            carried: 3,
            stamina: 100,
            interaction: Some(WireInteraction::Knocked),
            build_refusal: None,
            buffs: WireBuffs::NONE,
        },
        last_input_seq: 0,
        last_attack: None,
        loadout: WireLoadout {
            held: WireWeaponKind::Sword,
            spare: None,
        },
        prompt: None,
        build_kind: WireStructureKind::Watchtower,
    }];
    assert_server_wire(
        ServerMsg::Snapshot(Box::new(snapshot)),
        &[
            0x02, 0x01, 0x01, // Snapshot, tick 1, expedition 1
            0x01, 0x01, 0x03, 0x01, 0x01, // phase: Night, day 1, 3 waves, Fighting, wave 1
            0x00, 0x00, // wagon
            0x00, // stash
            0x00, // towers: 길이 0
            0x00, // nodes: 길이 0
            0x00, // piles: 길이 0
            0x01, // players: 길이 1
            0x03, 0x00, 0x00, 0x00, 0x00, // id 3, team, pos 0 0, facing 0
            0x00, 0x64, 0x64, 0x03, 0x64, // Alive, hp 100, max_hp 100, carried 3, stamina 100
            0x01, 0x05, // interaction: 있음, Knocked
            0x00, // build_refusal: 없음
            0x00, 0x00, // buffs: 없음
            0x00, 0x00, // last_input_seq 0, last_attack 없음
            0x00, 0x00, // loadout: 든 무기 Sword, 다른 칸 없음
            0x00, // prompt: 없음
            0x00, // build_kind: Watchtower
            0x00, // monsters: 길이 0
            0x00, // chests: 길이 0
            0x00, // ground_weapons: 길이 0
            0x00, // projectiles: 길이 0
        ],
    );
}

#[test]
fn a_boss_body_is_a_monster_of_kind_two() {
    let mut snapshot = quiet_snapshot(WirePhase::Day {
        day: 1,
        secs_left: 3.0,
    });
    snapshot.monsters = vec![WireMonster {
        id: 5,
        status: WireMonsterStatus::plain(WireMonsterKind::Boss),
        pos: WirePos { x: 3072, y: 128 },
        facing: WireAngle(128),
        hp: 12_000,
    }];
    // Snapshot, tick 1, expedition 1, 낮(날 1, 3.0초), 마차 0/0, 보관함 0, 탑과 지점과 더미와 플레이어는 빈 목록 넷
    let mut expected = vec![
        0x02, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    expected.extend_from_slice(&[
        0x01, // monsters: 길이 1
        0x05, // id 5
        0x02, // 종류 Boss, 효과 없음
        0x80, 0x18, 0x80, 0x01, // pos 3072, 128
        0x80, // facing 128
        0xE0, 0x5D, // hp 12000
        0x00, 0x00, 0x00, // chests, ground_weapons, projectiles 비어 있음
    ]);
    assert_eq!(
        encode_server(&ServerMsg::Snapshot(Box::new(snapshot))),
        expected
    );
}

#[test]
fn a_sapper_body_is_a_monster_of_kind_four() {
    let mut snapshot = quiet_snapshot(WirePhase::Day {
        day: 1,
        secs_left: 3.0,
    });
    snapshot.monsters = vec![WireMonster {
        id: 6,
        status: WireMonsterStatus::plain(WireMonsterKind::Sapper),
        pos: WirePos { x: 3072, y: 128 },
        facing: WireAngle(128),
        hp: 120,
    }];
    // Snapshot, tick 1, expedition 1, 낮(날 1, 3.0초), 마차 0/0, 보관함 0, 탑과 지점과 더미와 플레이어는 빈 목록 넷
    let mut expected = vec![
        0x02, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    expected.extend_from_slice(&[
        0x01, // monsters: 길이 1
        0x06, // id 6
        0x04, // 종류 Sapper, 효과 없음
        0x80, 0x18, 0x80, 0x01, // pos 3072, 128
        0x80, // facing 128
        0x78, // hp 120
        0x00, 0x00, 0x00, // chests, ground_weapons, projectiles 비어 있음
    ]);
    assert_eq!(
        encode_server(&ServerMsg::Snapshot(Box::new(snapshot))),
        expected
    );
}
