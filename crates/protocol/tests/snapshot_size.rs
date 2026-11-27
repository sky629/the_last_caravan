//! 몬스터 300마리, 포탑 40개(상한), 플레이어 4명이 있는 스냅샷의 크기가 정해 둔 한계 안인지 확인합니다.
//!
//! 한계는 측정한 크기보다 3%에서 6% 큰 값입니다. 한계를 넘으면 접속 하나가 쓰는 대역폭이 늘어난 것이므로, 다음 단계의 설계를 다시 정해야 합니다.

use caravan_protocol::{
    ServerMsg, Snapshot, WireAim, WireAngle, WireAttack, WireBoss, WireBossAct, WireBuffs,
    WireGroundWeapon, WireInteraction, WireLife, WireLoadout, WireMonster, WireMonsterEffects,
    WireMonsterKind, WireMonsterStatus, WireNightStep, WireNode, WirePhase, WirePile, WirePlayer,
    WirePos, WireProjectile, WirePrompt, WireRecordRow, WireReport, WireShot, WireStruck,
    WireStructureKind, WireTile, WireTower, WireTowerState, WireWagon, WireWeaponKind,
    encode_server,
};

/// 세 효과가 모두 걸린 몬스터의 종류와 효과입니다. 효과가 크기를 늘리지 않는다는 것을 가장 큰 경우로 확인하려고 모든 몬스터가 이 상태입니다.
fn afflicted_grunt() -> WireMonsterStatus {
    WireMonsterStatus::new(
        WireMonsterKind::Grunt,
        WireMonsterEffects {
            stunned: true,
            slowed: true,
            bleeding: true,
        },
    )
}

/// 몬스터와 탄환의 번호가 `first_id`부터 이어지는 스냅샷입니다. 모든 좌표 값은 2바이트로 기록되는 크기(128 이상)입니다.
fn crowded_snapshot(first_id: u32, monsters: u32, projectiles: u32) -> Snapshot {
    let player = |id: u32, life: WireLife| WirePlayer {
        id,
        team: 0,
        pos: WirePos { x: 1536, y: 1216 },
        facing: WireAngle(200),
        life,
        last_input_seq: 100_000,
        last_attack: Some(WireAttack {
            tick: 100_000,
            weapon: WireWeaponKind::Bow,
            trace_end: None,
        }),
        loadout: WireLoadout {
            held: WireWeaponKind::Bow,
            spare: Some(WireWeaponKind::Sword),
        },
        // 네 명이 모두 바닥의 무기 곁에 서 있는 경우입니다.
        prompt: Some(WirePrompt::TakeWeapon(WireWeaponKind::Spear)),
        build_kind: WireStructureKind::SwiftTower,
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
        // 포탑이 상한(40개)만큼 서 있고 모두 사격 중입니다. 종류는 번갈아 가며 정합니다.
        towers: (1..=40)
            .map(|id| WireTower {
                id,
                tile: WireTile {
                    x: (2 * id % 64) as u8,
                    y: (2 * id % 56) as u8,
                },
                kind: [
                    WireStructureKind::Watchtower,
                    WireStructureKind::SlowTower,
                    WireStructureKind::StunTower,
                    WireStructureKind::HasteTower,
                    WireStructureKind::SwiftTower,
                ][id as usize % 5],
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
                    buffs: WireBuffs::NONE,
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
                    buffs: WireBuffs::NONE,
                },
            ),
            player(4, WireLife::Dead),
        ],
        monsters: (0..monsters)
            .map(|i| WireMonster {
                id: first_id + i,
                status: afflicted_grunt(),
                pos: WirePos {
                    x: 200 + (i as u16 * 9) % 2800,
                    y: 200 + (i as u16 * 13) % 2800,
                },
                facing: WireAngle((i * 7) as u8),
                hp: 60,
            })
            .collect(),
        // 상자는 낮에만 있으므로 밤의 스냅샷에는 없습니다.
        chests: vec![],
        // 플레이어가 나가면서 떨어뜨린 무기 넷입니다.
        ground_weapons: (1..=4)
            .map(|id| WireGroundWeapon {
                id,
                pos: WirePos {
                    x: 1000 * id as u16,
                    y: 1500,
                },
                kind: WireWeaponKind::Greatsword,
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
    encode_server(&ServerMsg::Snapshot(Box::new(snapshot))).len()
}

/// 300마리 스냅샷 하나의 한계(바이트)입니다. 몬스터 번호가 16383 이하일 때, 몬스터 하나는 9바이트이므로 300마리가 2700바이트입니다.
/// 서 있는 포탑 하나가 사격 중일 때 13바이트이므로 40개가 520바이트이고, 나머지(플레이어 4명, 마차, 채집 지점, 바닥의 무기 4개, 머리)가 약 170바이트입니다.
/// 이 한계 아래에서 가장 큰 경우는 `the_worst_case_...` 시험이 확인합니다.
const BOUND_BYTES: usize = 3_600;
/// 몬스터 번호가 16384 이상이면 번호가 3바이트가 되어 몬스터 하나가 10바이트입니다. 오래 이어진 방에서 이 크기가 됩니다.
/// 포탑이 쏜 몬스터의 번호도 3바이트가 되어 측정값은 3,729바이트이고, 한계가 6% 큽니다.
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
fn monsters_with_every_effect_cost_the_same_nine_bytes_as_monsters_with_none() {
    let afflicted = encoded_len(crowded_snapshot(10_000, 300, 0));
    let mut plain = crowded_snapshot(10_000, 300, 0);
    for monster in &mut plain.monsters {
        monster.status = WireMonsterStatus::plain(WireMonsterKind::Grunt);
    }
    assert_eq!(afflicted, encoded_len(plain));
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

/// 한 원정에서 바닥에 놓일 수 있는 무기의 가장 큰 수입니다. 낮마다 상자가 셋이고 원정이 사흘이며 상자마다 무기가 하나입니다.
const MOST_WEAPONS_ON_THE_GROUND: u32 = 9;

/// 네 플레이어가 모두 소총을 쏜 직후이고, 바닥에 무기가 가장 많이 놓인 스냅샷입니다.
fn four_rifle_traces_and_the_most_weapons_on_the_ground(projectiles: u32) -> Snapshot {
    let mut snapshot = crowded_snapshot(10_000, 300, projectiles);
    for (i, player) in snapshot.players.iter_mut().enumerate() {
        player.last_attack = Some(WireAttack {
            tick: 100_000,
            weapon: WireWeaponKind::Rifle,
            trace_end: Some(WirePos {
                x: 3000 + 500 * i as u16,
                y: 2500 - 300 * i as u16,
            }),
        });
    }
    snapshot.ground_weapons = (1..=MOST_WEAPONS_ON_THE_GROUND)
        .map(|id| WireGroundWeapon {
            id,
            pos: WirePos {
                x: 800 * id as u16,
                y: 1500,
            },
            kind: WireWeaponKind::Rifle,
        })
        .collect();
    snapshot
}

#[test]
fn the_worst_case_of_four_rifle_traces_and_nine_weapons_on_the_ground_stays_under_the_bound() {
    let base = encoded_len(crowded_snapshot(10_000, 300, 0));
    let worst = encoded_len(four_rifle_traces_and_the_most_weapons_on_the_ground(0));
    let with_arrows = encoded_len(four_rifle_traces_and_the_most_weapons_on_the_ground(8));
    println!(
        "300 monsters, 40 towers, 4 players: {base} bytes; with 4 rifle traces and 9 weapons on the ground: {worst} bytes; with 8 projectiles as well: {with_arrows} bytes"
    );
    assert!(worst < BOUND_BYTES, "{worst} bytes");
    assert!(with_arrows < BOUND_BYTES, "{with_arrows} bytes");
}

#[test]
fn a_rifle_trace_costs_four_bytes_more_than_an_attack_without_one() {
    // 끝 자리는 `WirePos`이고, 두 축이 모두 2바이트로 기록되는 크기(128 이상)입니다.
    let without = encoded_len(crowded_snapshot(10_000, 300, 0));
    let mut snapshot = crowded_snapshot(10_000, 300, 0);
    snapshot.players[0].last_attack = Some(WireAttack {
        tick: 100_000,
        weapon: WireWeaponKind::Rifle,
        trace_end: Some(WirePos { x: 3000, y: 2500 }),
    });

    assert_eq!(encoded_len(snapshot) - without, 4);
}

/// 플레이어 넷이 모두 살아 있고 두 버프를 모두 가진 상태로 바꿉니다. 버프를 실어야 해서 가장 큰 경우입니다.
fn make_all_four_alive_with_both_buffs(snapshot: &mut Snapshot) {
    for player in &mut snapshot.players {
        player.life = alive_with(WireBuffs {
            haste: true,
            swift: true,
        });
    }
}

fn alive_with(buffs: WireBuffs) -> WireLife {
    WireLife::Alive {
        hp: 100,
        max_hp: 100,
        carried: 10,
        stamina: 100,
        interaction: None,
        build_refusal: None,
        buffs,
    }
}

#[test]
fn four_alive_players_with_both_buffs_stay_under_the_bound() {
    let mut worst = four_rifle_traces_and_the_most_weapons_on_the_ground(8);
    make_all_four_alive_with_both_buffs(&mut worst);
    let mut large_ids = crowded_snapshot(20_000, 300, 0);
    make_all_four_alive_with_both_buffs(&mut large_ids);
    let (worst, large_ids) = (encoded_len(worst), encoded_len(large_ids));
    println!(
        "four alive players with both buffs: {worst} bytes in the worst case, {large_ids} bytes with 3-byte monster ids"
    );
    assert!(worst < BOUND_BYTES, "{worst} bytes");
    assert!(large_ids < BOUND_BYTES_WITH_LARGE_IDS, "{large_ids} bytes");
}

#[test]
fn the_buffs_of_an_alive_player_cost_two_bytes_whether_or_not_it_has_any() {
    // 살아 있는 플레이어의 생명 상태는 변형 1, HP 1, 최대 HP 1, 든 자원 1, 달리기 게이지 1, 상호작용 없음 1, 건설 거절 없음 1, 버프 2로 9바이트이고,
    // 죽은 플레이어는 변형 1바이트입니다. 네 번째 플레이어가 죽어 있는 기준 상황에서 그 플레이어를 살립니다.
    let dead = encoded_len(crowded_snapshot(10_000, 300, 0));
    for buffs in [
        WireBuffs::NONE,
        WireBuffs {
            haste: true,
            swift: true,
        },
    ] {
        let mut snapshot = crowded_snapshot(10_000, 300, 0);
        snapshot.players[3].life = alive_with(buffs);
        assert_eq!(encoded_len(snapshot) - dead, 8, "{buffs:?}");
    }
}

/// 야영지의 스냅샷입니다. 플레이어 넷이 모두 준비해서 카운트다운이 돌고, 허수아비 셋이 모두 막 맞은 가장 큰 경우입니다.
fn camp_snapshot() -> Snapshot {
    let mut snapshot = crowded_snapshot(1, 3, 0);
    snapshot.phase = WirePhase::Camp {
        ready: vec![1, 2, 3, 4],
        starting_in: Some(2.5),
        struck: (1..=3)
            .map(|monster| WireStruck {
                monster,
                tick: 100_000,
                amount: 100,
            })
            .collect(),
    };
    snapshot.towers = vec![];
    snapshot.nodes = vec![];
    snapshot.piles = vec![];
    for monster in &mut snapshot.monsters {
        monster.status = WireMonsterStatus::plain(WireMonsterKind::Scarecrow);
    }
    snapshot
}

#[test]
fn a_camp_snapshot_with_four_ready_players_and_three_struck_scarecrows_stays_far_under_the_bound() {
    let size = encoded_len(camp_snapshot());
    println!(
        "camp: 4 ready players, 3 struck scarecrows: {size} bytes per snapshot, {} bytes per second at 20 Hz",
        size * 20
    );
    assert!(size < BOUND_BYTES / 4, "{size} bytes");
}

#[test]
fn a_result_snapshot_with_four_confirmed_players_stays_under_the_bound() {
    let mut snapshot = crowded_snapshot(10_000, 300, 0);
    snapshot.phase = WirePhase::Debrief {
        outcome: caravan_protocol::WireOutcome::Defeat,
        secs_left: 59.5,
        confirmed: vec![1, 2, 3, 4],
    };
    let size = encoded_len(snapshot);
    println!("result: 300 monsters, 40 towers, 4 confirmed players: {size} bytes per snapshot");
    assert!(size < BOUND_BYTES, "{size} bytes");
}

#[test]
fn the_result_of_four_players_with_the_longest_names_is_sent_once_and_is_small() {
    let result = ServerMsg::Debrief(WireReport {
        expedition: 7,
        day: 3,
        elapsed_secs: 3_600,
        tower_kills: 1_000,
        rows: (0..4)
            .map(|index| WireRecordRow {
                name: format!("NAME_{index}_LONG1"),
                kills: 5_000,
                revives: 20,
                downs: 20,
                deposited: 2_000,
            })
            .collect(),
    });
    let size = encode_server(&result).len();
    println!(
        "result: 4 players with 12-letter names, sent once when the result opens: {size} bytes"
    );
    assert!(size < 200, "{size} bytes");
}

/// 보스의 몸이 몬스터 목록의 첫 몬스터이고, 밤의 단계가 보스와 싸우는 중인 스냅샷입니다. 보스의 몸은 보통 몬스터와 같은 9바이트에 HP가 한 바이트 더 큽니다.
fn with_the_boss(mut snapshot: Snapshot, act: WireBossAct) -> Snapshot {
    snapshot.monsters[0].status = WireMonsterStatus::new(
        WireMonsterKind::Boss,
        WireMonsterEffects {
            stunned: false,
            slowed: true,
            bleeding: true,
        },
    );
    snapshot.monsters[0].hp = 12_000;
    snapshot.phase = WirePhase::Night {
        day: 3,
        wave_count: 3,
        step: WireNightStep::Boss(WireBoss {
            max_hp: 12_000,
            aim: WireAim::Player(4),
            act,
        }),
    };
    snapshot
}

/// 보스가 하는 일의 모든 경우입니다. 위치는 두 축이 모두 2바이트로 기록되는 크기(128 이상)입니다.
fn every_boss_act() -> [(&'static str, WireBossAct); 2] {
    [
        ("walking", WireBossAct::Walking),
        (
            "slam",
            WireBossAct::Slam {
                center: WirePos { x: 1500, y: 1200 },
                progress: 0.5,
            },
        ),
    ]
}

#[test]
fn the_boss_step_costs_at_most_sixteen_bytes_more_than_a_fight_and_stays_under_the_bound() {
    let fight = encoded_len(crowded_snapshot(10_000, 300, 0));
    for (name, act) in every_boss_act() {
        let boss = encoded_len(with_the_boss(crowded_snapshot(10_000, 300, 0), act));
        println!(
            "boss {name}: {boss} bytes, {} more than a fight",
            boss - fight
        );
        assert!(boss - fight <= 16, "{name}: {} more bytes", boss - fight);
        assert!(boss < BOUND_BYTES, "{name}: {boss} bytes");
    }
}

#[test]
fn the_worst_case_with_the_boss_and_every_other_worst_case_at_once_stays_under_both_bounds() {
    for (name, act) in every_boss_act() {
        let mut worst = with_the_boss(four_rifle_traces_and_the_most_weapons_on_the_ground(8), act);
        make_all_four_alive_with_both_buffs(&mut worst);
        let mut large_ids = with_the_boss(crowded_snapshot(20_000, 300, 0), act);
        make_all_four_alive_with_both_buffs(&mut large_ids);
        let (worst, large_ids) = (encoded_len(worst), encoded_len(large_ids));
        println!(
            "boss {name}, four players with both buffs, 4 rifle traces, 9 weapons, 8 arrows: {worst} bytes; with 3-byte ids: {large_ids} bytes"
        );
        assert!(worst < BOUND_BYTES, "{name}: {worst} bytes");
        assert!(
            large_ids < BOUND_BYTES_WITH_LARGE_IDS,
            "{name}: {large_ids} bytes"
        );
    }
}

#[test]
fn the_warning_costs_no_more_than_a_wave_of_the_fight() {
    let mut warning = crowded_snapshot(10_000, 300, 0);
    warning.phase = WirePhase::Night {
        day: 3,
        wave_count: 3,
        step: WireNightStep::Warning { secs_left: 4.5 },
    };
    let fight = encoded_len(crowded_snapshot(10_000, 300, 0));

    assert!(encoded_len(warning) - fight <= 4);
}
