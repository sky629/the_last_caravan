//! 몬스터가 300마리 있는 방에서 서버가 실제로 보내는 스냅샷의 크기를 확인합니다.
//!
//! 프로토콜 크레이트의 크기 테스트는 손으로 만든 스냅샷을 쓰고, 이 테스트는 실제 규칙이 만든 상태를 씁니다.
//! 규칙의 수치를 바꿔서 마차와 플레이어가 죽지 않게 하고 몬스터가 Tick마다 한 마리씩 나타나게 하면,
//! 아무도 막지 않는 300마리가 마차 둘레에 쌓입니다.

mod common;

use std::time::{Duration, Instant};

use caravan_protocol::{ServerMsg, encode_server};
use caravan_rules::{MonsterKind, NightPlan, Tuning, WavePlan};
use common::rig_with;

/// 이 크기를 넘으면 접속 하나의 전송량이 측정한 값보다 늘어난 것입니다.
/// 몬스터 번호가 16383 이하이므로 몬스터 하나는 9바이트이고, 300마리가 2700바이트입니다. 감시탑 40개가 약 560바이트를 더하고,
/// 이 테스트의 감시탑은 HP가 `u32::MAX`여서 실제보다 값마다 3바이트씩 더 씁니다.
const BOUND_BYTES: usize = 3_600;

fn crowd_tuning() -> Tuning {
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 0.1;
    tuning.nights.plans = vec![NightPlan {
        waves: vec![WavePlan {
            kind: MonsterKind::Grunt,
            count: 300,
        }],
    }];
    tuning.nights.first_wave_delay_secs = 0.1;
    tuning.nights.spawn_interval_secs = 0.05;
    tuning.wagon.max_hp = u32::MAX;
    tuning.player.max_hp = u32::MAX;
    // 감시탑이 상한만큼 서 있습니다. 감시탑은 사격하지만 피해를 주지 않아서 300마리가 그대로 쌓입니다.
    tuning.village.start_towers = tuning.spread_tower_sites(tuning.village.tower_limit);
    tuning.structures.watchtower.max_hp = u32::MAX;
    tuning.structures.watchtower.damage = 0;
    tuning
}

#[test]
fn the_snapshot_of_a_real_300_monster_crowd_stays_under_the_bound() {
    let mut rig = rig_with(crowd_tuning());
    let (mut client, _) = rig.join_ok("load");

    let mut largest = (0, 0);
    let mut spent = Duration::ZERO;
    let mut worst = Duration::ZERO;
    for _ in 0..400 {
        rig.now += rig.period;
        let started = Instant::now();
        rig.server.pump(rig.now);
        let took = started.elapsed();
        spent += took;
        worst = worst.max(took);
        for snapshot in common::snapshots(&mut client) {
            let bytes = encode_server(&ServerMsg::Snapshot(snapshot.clone())).len();
            if snapshot.monsters.len() >= largest.0 {
                largest = (snapshot.monsters.len(), bytes);
            }
        }
    }

    println!(
        "{} monsters: {} bytes per snapshot, {} bytes per second at 20 Hz; \
         tick time over 400 ticks: average {:.2} ms, worst {:.2} ms (measured in this test's build profile)",
        largest.0,
        largest.1,
        largest.1 * 20,
        spent.as_secs_f64() * 1000.0 / 400.0,
        worst.as_secs_f64() * 1000.0
    );
    assert_eq!(largest.0, 300, "the whole wave was on the field at once");
    assert!(largest.1 < BOUND_BYTES, "{} bytes", largest.1);
}
