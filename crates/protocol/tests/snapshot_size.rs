//! 몬스터 300마리, 감시탑 40개(상한), 플레이어 4명이 있는 스냅샷의 크기가 정해 둔 한계 안인지 확인합니다.
//!
//! 한계는 측정한 크기에 약 7%의 여유를 둔 값입니다. 한계를 넘으면 접속 하나가 쓰는 대역폭이 늘어난 것이므로, 다음 단계의 설계를 다시 정해야 합니다.

use caravan_protocol::{
    ServerMsg, Snapshot, WireAngle, WireInteraction, WireLife, WireMonster, WireMonsterKind,
    WireNightStep, WireNode, WirePhase, WirePile, WirePlayer, WirePos, WireProjectile, WireShot,
    WireTile, WireTower, WireTowerState, WireWagon, WireWeaponKind, encode_server,
};

/// 몬스터와 탄환의 번호가 `first_id`부터 이어지는 스냅샷입니다. 모든 좌표 값은 2바이트로 기록되는 크기(128 이상)입니다.
fn crowded_snapshot(first_id: u32, monsters: u32, projectiles: u32) -> Snapshot {
    let player = |id: u32, life: WireLife| WirePlayer {
        id,
        team: 0,
        pos: WirePos { x: 1536, y: 1216 },
        facing: WireAngle(200),
        life,
        last_input_seq: 100_000,
        last_attack_tick: Some(100_000),
        weapon: WireWeaponKind::Bow,
    };
    Snapshot {
        tick: 100_000,
        expedition: 7,
        phase: WirePhase::Night {
            day: 3,
            wave_count: 3,
            step: WireNightStep::Fighting { wave: 3 },
        },
        wagon: WireWagon {
            hp: 450,
            max_hp: 500,
        },
        stash: 12,
        // 감시탑이 상한(40개)만큼 서 있고 모두 사격 중입니다.
        towers: (1..=40)
            .map(|id| WireTower {
                id,
                tile: WireTile {
                    x: (2 * id % 64) as u8,
                    y: (2 * id % 56) as u8,
                },
                max_hp: 300,
                state: WireTowerState::Standing {
                    hp: 120,
                    last_shot: Some(WireShot {
                        target: first_id,
                        tick: 99_999,
                    }),
                },
            })
            .collect(),
        nodes: (1..=8)
            .map(|id| WireNode {
                id,
                amount: 15 - id,
            })
            .collect(),
        piles: (1..=3)
            .map(|id| WirePile {
                id,
                pos: WirePos {
                    x: 1000 * id as u16,
                    y: 1500,
                },
                amount: 6,
            })
            .collect(),
        players: vec![
            player(
                1,
                WireLife::Alive {
                    hp: 70,
                    max_hp: 100,
                    carried: 4,
                    stamina: 60,
                    interaction: Some(WireInteraction::Reviving {
                        target: 2,
                        progress: 0.5,
                    }),
                    build_refusal: None,
                },
            ),
            player(
                2,
                WireLife::Downed {
                    bleed_secs_left: 12.5,
                    revive_progress: Some(0.5),
                },
            ),
            player(
                3,
                WireLife::Alive {
                    hp: 100,
                    max_hp: 100,
                    carried: 10,
                    stamina: 100,
                    interaction: None,
                    build_refusal: None,
                },
            ),
            player(4, WireLife::Dead),
        ],
        monsters: (0..monsters)
            .map(|i| WireMonster {
                id: first_id + i,
                kind: WireMonsterKind::Grunt,
                pos: WirePos {
                    x: 200 + (i as u16 * 9) % 2800,
                    y: 200 + (i as u16 * 13) % 2800,
                },
                facing: WireAngle((i * 7) as u8),
                hp: 60,
            })
            .collect(),
        projectiles: (0..projectiles)
            .map(|i| WireProjectile {
                id: first_id + i,
                pos: WirePos {
                    x: 200 + (i as u16 * 37) % 2800,
                    y: 300 + (i as u16 * 41) % 2800,
                },
                facing: WireAngle((i * 31) as u8),
            })
            .collect(),
    }
}

fn encoded_len(snapshot: Snapshot) -> usize {
    encode_server(&ServerMsg::Snapshot(snapshot)).len()
}

/// 300마리 스냅샷 하나의 한계(바이트)입니다. 몬스터 번호가 16383 이하일 때, 몬스터 하나는 9바이트이므로 300마리가 2700바이트입니다.
/// 서 있는 감시탑 하나가 사격 중일 때 약 14바이트이므로 40개가 약 560바이트이고, 나머지(플레이어 4명, 마차, 채집 지점, 머리)가 약 100바이트입니다.
/// 측정한 3,367바이트(탄환 없음)와 3,423바이트(탄환 8개)에 약 7%의 여유를 둡니다.
const BOUND_BYTES: usize = 3_600;
/// 몬스터 번호가 16384 이상이면 번호가 3바이트가 되어 몬스터 하나가 10바이트입니다. 오래 이어진 방에서 이 크기가 됩니다.
/// 감시탑이 쏜 몬스터의 번호도 3바이트가 되어 측정값은 3,702바이트이고, 여기에 약 7%의 여유를 둡니다.
const BOUND_BYTES_WITH_LARGE_IDS: usize = 3_950;

#[test]
fn a_snapshot_with_300_monsters_40_towers_and_4_players_stays_under_the_bound() {
    let size = encoded_len(crowded_snapshot(10_000, 300, 0));
    println!(
        "300 monsters, 40 towers, 4 players: {size} bytes per snapshot, {} bytes per second at 20 Hz",
        size * 20
    );
    assert!(size < BOUND_BYTES, "{size} bytes");
}

#[test]
fn the_bound_still_holds_when_monster_ids_have_grown_past_16383() {
    let size = encoded_len(crowded_snapshot(20_000, 300, 0));
    println!(
        "300 monsters with 3-byte ids: {size} bytes per snapshot, {} bytes per second at 20 Hz",
        size * 20
    );
    assert!(size < BOUND_BYTES_WITH_LARGE_IDS, "{size} bytes");
}

#[test]
fn one_monster_costs_nine_bytes_and_grows_the_snapshot_linearly() {
    // 목록 길이도 가변 길이 정수이므로, 두 크기 모두 길이가 2바이트인 128마리 이상에서 비교합니다.
    let base = encoded_len(crowded_snapshot(10_000, 130, 0));
    let more = encoded_len(crowded_snapshot(10_000, 230, 0));
    assert_eq!(more - base, 100 * 9);
}

#[test]
fn eight_projectiles_in_the_air_add_about_fifty_bytes_and_stay_under_the_bound() {
    // 네 명이 0.4초마다 쏘고 탄환이 0.65초 날아가면 동시에 8개 안팎입니다.
    let without = encoded_len(crowded_snapshot(10_000, 300, 0));
    let with = encoded_len(crowded_snapshot(10_000, 300, 8));
    println!(
        "300 monsters, 40 towers, 4 players, 8 projectiles: {with} bytes per snapshot ({} more), {} bytes per second at 20 Hz",
        with - without,
        with * 20
    );
    assert!(with < BOUND_BYTES, "{with} bytes");
}

#[test]
fn one_projectile_costs_seven_bytes_and_grows_the_snapshot_linearly() {
    // 번호 2, 위치 4, 방향 1바이트입니다. 목록 길이도 가변 길이 정수이므로, 두 크기 모두 길이가 2바이트인 128개 이상에서 비교합니다.
    let base = encoded_len(crowded_snapshot(10_000, 0, 130));
    let more = encoded_len(crowded_snapshot(10_000, 0, 230));
    assert_eq!(more - base, 100 * 7);
}
