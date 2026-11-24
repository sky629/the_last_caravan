//! 몬스터 300마리와 플레이어 4명이 있는 스냅샷의 크기가 정해 둔 한계 안인지 확인합니다.
//!
//! 한계를 넘으면 접속 하나가 쓰는 대역폭이 예산(약 55 KB/s)을 넘으므로, 다음 단계의 설계를 다시 정해야 합니다.

use caravan_protocol::{
    ServerMsg, Snapshot, WireAngle, WireInteraction, WireLife, WireMonster, WireMonsterKind,
    WireNightStep, WirePhase, WirePlayer, WirePos, WireShot, WireTile, WireTower, WireTowerState,
    WireWagon, encode_server,
};

/// 몬스터 번호가 `first_id`부터 이어지는 스냅샷입니다. 모든 좌표 값은 2바이트로 기록되는 크기(128 이상)입니다.
fn crowded_snapshot(first_id: u32, monsters: u32) -> Snapshot {
    let player = |id: u32, life: WireLife| WirePlayer {
        id,
        team: 0,
        pos: WirePos { x: 1536, y: 1216 },
        facing: WireAngle(200),
        life,
        last_input_seq: 100_000,
        last_attack_tick: Some(100_000),
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
        towers: vec![WireTower {
            id: 1,
            tile: WireTile { x: 15, y: 11 },
            max_hp: 300,
            state: WireTowerState::Standing {
                hp: 120,
                last_shot: Some(WireShot {
                    target: first_id,
                    tick: 99_999,
                }),
            },
        }],
        players: vec![
            player(
                1,
                WireLife::Alive {
                    hp: 70,
                    max_hp: 100,
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
    }
}

fn encoded_len(snapshot: Snapshot) -> usize {
    encode_server(&ServerMsg::Snapshot(snapshot)).len()
}

/// 300마리 스냅샷 하나의 한계(바이트)입니다. 몬스터 번호가 16383 이하일 때, 몬스터 하나는 9바이트이므로 300마리가 2700바이트입니다.
/// 나머지(플레이어 4명, 감시탑, 마차, 머리)가 약 100바이트이고, 합계에 약 7%의 여유를 둡니다.
const BOUND_BYTES: usize = 3_000;
/// 몬스터 번호가 16384 이상이면 번호가 3바이트가 되어 몬스터 하나가 10바이트입니다. 오래 이어진 방에서 이 크기가 됩니다.
const BOUND_BYTES_WITH_LARGE_IDS: usize = 3_300;

#[test]
fn a_snapshot_with_300_monsters_and_4_players_stays_under_the_bound() {
    let size = encoded_len(crowded_snapshot(10_000, 300));
    println!(
        "300 monsters, 4 players: {size} bytes per snapshot, {} bytes per second at 20 Hz",
        size * 20
    );
    assert!(size < BOUND_BYTES, "{size} bytes");
}

#[test]
fn the_bound_still_holds_when_monster_ids_have_grown_past_16383() {
    let size = encoded_len(crowded_snapshot(20_000, 300));
    println!(
        "300 monsters with 3-byte ids: {size} bytes per snapshot, {} bytes per second at 20 Hz",
        size * 20
    );
    assert!(size < BOUND_BYTES_WITH_LARGE_IDS, "{size} bytes");
}

#[test]
fn one_monster_costs_nine_bytes_and_grows_the_snapshot_linearly() {
    // 목록 길이도 가변 길이 정수이므로, 두 크기 모두 길이가 2바이트인 128마리 이상에서 비교합니다.
    let base = encoded_len(crowded_snapshot(10_000, 130));
    let more = encoded_len(crowded_snapshot(10_000, 230));
    assert_eq!(more - base, 100 * 9);
}
