//! 몬스터의 종류와 걸려 있는 효과가 1바이트에 담기고 되돌아오는지 확인합니다.

use caravan_protocol::{
    ServerMsg, Snapshot, WireAngle, WireMonster, WireMonsterEffects, WireMonsterKind,
    WireMonsterStatus, WirePhase, WirePos, WireWagon, decode_server, encode_server,
};
use caravan_rules::MonsterKind;

fn byte_of(status: WireMonsterStatus) -> u8 {
    u8::from(status)
}

fn effects(stunned: bool, slowed: bool, bleeding: bool) -> WireMonsterEffects {
    WireMonsterEffects {
        stunned,
        slowed,
        bleeding,
    }
}

#[test]
fn every_kind_with_every_combination_of_effects_survives_one_byte() {
    for kind in MonsterKind::ALL {
        for bits in 0..8u8 {
            let combination = effects(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            let status = WireMonsterStatus::new(WireMonsterKind::of(kind), combination);
            assert_eq!(WireMonsterStatus::try_from(byte_of(status)), Ok(status));
            assert_eq!(status.kind(), WireMonsterKind::of(kind));
            assert_eq!(status.effects(), combination);
        }
    }
}

#[test]
fn the_layout_is_the_kind_in_the_low_bits_and_one_bit_per_effect_above_it() {
    let grunt = WireMonsterKind::Grunt;
    let scarecrow = WireMonsterKind::Scarecrow;
    assert_eq!(byte_of(WireMonsterStatus::plain(grunt)), 0b0000_0000);
    assert_eq!(byte_of(WireMonsterStatus::plain(scarecrow)), 0b0000_0001);
    assert_eq!(
        byte_of(WireMonsterStatus::plain(WireMonsterKind::Boss)),
        0b0000_0010
    );
    assert_eq!(
        byte_of(WireMonsterStatus::plain(WireMonsterKind::Runner)),
        0b0000_0011
    );
    assert_eq!(
        byte_of(WireMonsterStatus::new(grunt, effects(true, false, false))),
        0b0001_0000,
        "stunned"
    );
    assert_eq!(
        byte_of(WireMonsterStatus::new(grunt, effects(false, true, false))),
        0b0010_0000,
        "slowed"
    );
    assert_eq!(
        byte_of(WireMonsterStatus::new(
            scarecrow,
            effects(false, false, true)
        )),
        0b0100_0001,
        "bleeding"
    );
}

#[test]
fn a_byte_that_names_no_kind_or_sets_the_unused_bit_is_refused() {
    assert!(WireMonsterStatus::try_from(0b0000_1111).is_err(), "kind 15");
    assert!(WireMonsterStatus::try_from(0b1000_0000).is_err(), "unused");
}

#[test]
fn a_snapshot_with_a_corrupt_monster_byte_is_an_ordinary_decode_error() {
    let snapshot = Snapshot {
        monsters: vec![WireMonster {
            id: 1,
            status: WireMonsterStatus::plain(WireMonsterKind::Grunt),
            pos: WirePos { x: 1000, y: 1000 },
            facing: WireAngle(0),
            hp: 60,
        }],
        ..empty_snapshot()
    };
    let mut bytes = encode_server(&ServerMsg::Snapshot(Box::new(snapshot)));
    // 몬스터 하나는 번호 1, 종류와 효과의 바이트, 위치 x(1000)와 y(1000), 방향 0, HP 60의 순서로 기록됩니다.
    let monster = [1, 0, 0xE8, 0x07, 0xE8, 0x07, 0, 60];
    let at = bytes
        .windows(monster.len())
        .position(|window| window == monster)
        .expect("the monster is in the message")
        + 1;
    for corrupt in [0b0000_1111, 0b1000_0000] {
        bytes[at] = corrupt;
        assert!(decode_server(&bytes).is_err(), "{corrupt:#010b}");
    }
}

fn empty_snapshot() -> Snapshot {
    Snapshot {
        tick: 1,
        expedition: 1,
        phase: WirePhase::Day {
            day: 1,
            secs_left: 1.0,
        },
        wagon: WireWagon { hp: 1, max_hp: 1 },
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
