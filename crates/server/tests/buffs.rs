//! 기본 서버에서는 어떤 플레이어도 버프를 받지 않는 것을 공개 API(`GameServer`)로만 확인합니다. 클라이언트가 받는 스냅샷만 봅니다.
//! 프로세스 내부 전달과 가짜 시각을 사용하므로 소켓, 스레드, 대기가 없습니다.
//!
//! 두 사람이 야영지에서 준비한 뒤 원정의 첫날과 첫 밤을 지나면서 걷고, 달리고, 공격하고, 건설하고, 상호작용합니다.
//! 그동안 두 사람이 받은 모든 스냅샷에서 살아 있는 플레이어의 버프가 비어 있어야 합니다.

mod common;

use caravan_protocol::{
    ClientMsg, ClientTransport, InputMsg, WireBuffs, WireLife, WirePhase, WireVec2,
};
use common::{press_ready, rig, snapshots};

/// 두 사람의 입력입니다. 첫째는 2초마다 방향을 바꿔 사각형으로 걸으며 달리고 공격합니다.
/// 둘째는 제자리에서 10초 가운데 5초는 건설 키를, 5초는 상호작용 키를 누릅니다.
fn input_of(person: usize, seq: u32) -> ClientMsg {
    let second = seq / 20;
    let mut input = InputMsg {
        seq,
        ..InputMsg::default()
    };
    if person == 0 {
        let (x, y) = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)][(second / 2 % 4) as usize];
        input.move_dir = WireVec2::new(x, y);
        input.aim = WireVec2::new(x, y);
        input.sprint = true;
        input.attack = true;
    } else {
        input.aim = WireVec2::new(1.0, 0.0);
        input.build = second % 10 < 5;
        input.interact = !input.build;
    }
    ClientMsg::Input(input)
}

#[test]
fn nobody_has_a_buff_in_any_snapshot_of_the_camp_the_first_day_and_the_first_night() {
    let mut rig = rig();
    let (mut first, _) = rig.join_ok("buffs");
    let (mut second, _) = rig.join_ok("buffs");
    first.send(&press_ready(1));
    second.send(&press_ready(1));

    let (mut in_the_camp, mut by_day, mut by_night) = (false, false, false);
    let mut alive_players_seen = 0;
    let (mut attacked, mut sprinted) = (false, false);
    // 카운트다운 3초, 낮 90초, 첫 무리가 나타나기까지 3초, 그리고 싸우는 15초입니다. 순서 번호 1은 준비 키를 누르는 입력에 쓰였습니다.
    for seq in 2..2 + rig.ticks(3.0 + 90.0 + 3.0 + 15.0) {
        first.send(&input_of(0, seq));
        second.send(&input_of(1, seq));
        rig.run_ticks(1);
        for snapshot in [snapshots(&mut first), snapshots(&mut second)].concat() {
            match snapshot.phase {
                WirePhase::Camp { .. } => in_the_camp = true,
                WirePhase::Day { .. } => by_day = true,
                WirePhase::Night { .. } => by_night = true,
                WirePhase::Dawn { .. } | WirePhase::Debrief { .. } => {}
            }
            for player in &snapshot.players {
                attacked |= player.last_attack.is_some();
                if let WireLife::Alive { buffs, stamina, .. } = player.life {
                    alive_players_seen += 1;
                    sprinted |= stamina < 100;
                    assert_eq!(
                        buffs,
                        WireBuffs::NONE,
                        "player {} at tick {}",
                        player.id,
                        snapshot.tick
                    );
                }
            }
        }
    }

    assert!(
        in_the_camp && by_day && by_night,
        "the run covers the camp, a day and a night"
    );
    assert!(alive_players_seen > 1000, "{alive_players_seen}");
    assert!(
        attacked && sprinted,
        "the people fought and ran during the run"
    );
}
