//! 버프 포탑(공격 속도 포탑, 이동 속도 포탑)을 확인합니다. 버프는 버프가 걸려 있는지의 질문(`has_haste`, `has_swift`)과
//! 플레이어의 공격이 나가는 Tick, 한 Tick에 걸은 거리로 읽습니다.
//!
//! 시나리오는 20 x 20 맵이고 버프 포탑은 타일 (5, 5)에 섭니다. 중심은 (5.5, 5.5)이고 범위는 3타일입니다.

use super::support::{Scenario, close};
use crate::ids::PlayerId;
use crate::input::{AimDir, PlayerInput};
use crate::kind::StructureKind;
use crate::math::Vec2;
use crate::player::{Life, Presence};
use crate::tuning::{AuraTuning, Buff, TowerAction, Tuning};

/// 포탑에서 2타일 떨어진 자리입니다. 범위 안입니다.
const IN_RANGE: Vec2 = Vec2::new(7.5, 5.5);
/// 포탑에서 7타일 떨어진 자리입니다. 범위 밖입니다.
const OUT_OF_RANGE: Vec2 = Vec2::new(12.5, 5.5);

/// 플레이어가 지금 가진 버프입니다. 앞이 공격 속도, 뒤가 이동 속도입니다.
fn buffs_of(s: &Scenario, id: PlayerId) -> (bool, bool) {
    match &s.player(id).life {
        Life::Alive { buffs, .. } => (buffs.has_haste(), buffs.has_swift()),
        Life::Downed { .. } | Life::Dead => (false, false),
    }
}

/// 공격 키를 누르고 있으면서 `ticks` Tick을 진행하고, 공격이 나간 Tick을 돌려줍니다. 진행을 시작한 뒤 몇 번째 Tick인지를 1부터 셉니다.
fn swing_ticks_in(s: &mut Scenario, id: PlayerId, ticks: u32) -> Vec<u32> {
    s.hold(
        id,
        PlayerInput {
            attack: true,
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let mut swings = Vec::new();
    let mut last = s.player(id).last_attack.map(|attack| attack.tick);
    for tick in 1..=ticks {
        s.run_ticks(1);
        let now = s.player(id).last_attack.map(|attack| attack.tick);
        if now != last {
            swings.push(tick);
            last = now;
        }
    }
    swings
}

/// `is_on`이 참인 동안 Tick을 진행하고, 진행한 Tick의 수를 돌려줍니다.
fn ticks_while(s: &mut Scenario, mut is_on: impl FnMut(&Scenario) -> bool) -> u32 {
    let mut ticks = 0;
    while is_on(s) && ticks < 1000 {
        ticks += 1;
        s.run_ticks(1);
    }
    ticks
}

#[test]
fn the_default_buff_towers_cost_15_reach_three_tiles_and_keep_their_buff_for_two_seconds() {
    let t = Tuning::default();
    for (row, buff) in [
        (&t.structures.haste_tower, Buff::Haste { secs: 2.0 }),
        (&t.structures.swift_tower, Buff::Swift { secs: 2.0 }),
    ] {
        assert_eq!((row.range, row.build_cost), (3.0, 15));
        assert_eq!(row.action, TowerAction::Aura(AuraTuning { buff }));
    }
}

#[test]
fn a_player_in_the_range_of_a_haste_tower_has_haste_and_keeps_it_for_exactly_two_seconds_after_leaving()
 {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::HasteTower, 5, 5);
    let near = s.player_at(IN_RANGE.x, IN_RANGE.y);
    let far = s.player_at(OUT_OF_RANGE.x, OUT_OF_RANGE.y);

    s.run_ticks(1);
    assert_eq!(buffs_of(&s, near), (true, false), "only haste");
    assert_eq!(buffs_of(&s, far), (false, false), "out of the range");
    s.run_ticks(100);
    assert_eq!(
        buffs_of(&s, near),
        (true, false),
        "it never runs out in the range"
    );

    s.move_player(near, OUT_OF_RANGE);
    let lingered = ticks_while(&mut s, |s| buffs_of(s, near).0);
    assert_eq!(lingered, s.ticks(2.0), "two seconds of ticks after leaving");
}

#[test]
fn a_player_in_the_range_of_a_swift_tower_walks_at_130_percent_for_exactly_two_seconds_after_leaving()
 {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::SwiftTower, 5, 5);
    let a = s.player_at(5.5, 7.5);
    s.run_ticks(1);
    assert_eq!(buffs_of(&s, a), (false, true), "only swift");

    s.move_player(a, Vec2::new(1.5, 12.5));
    s.hold_move(a, 0.0, -1.0);
    let mut steps = Vec::new();
    for _ in 0..s.ticks(2.0) + 1 {
        let before = s.player(a).pos.y;
        s.run_ticks(1);
        steps.push(before - s.player(a).pos.y);
    }

    let (last, lingering) = steps.split_last().expect("a step was taken");
    assert!(
        lingering.iter().all(|step| close(*step, 0.26)),
        "4 x 1.3 x 0.05 for each of the two seconds: {lingering:?}"
    );
    assert!(close(*last, 0.2), "4 x 0.05 after them: {last}");
}

#[test]
fn two_haste_towers_in_range_at_once_keep_the_sword_at_seven_ticks_and_two_swift_towers_at_130_percent()
 {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::HasteTower, 5, 5);
    s.tower_of(StructureKind::HasteTower, 5, 7);
    s.tower_of(StructureKind::SwiftTower, 8, 5);
    s.tower_of(StructureKind::SwiftTower, 8, 7);
    let a = s.player_at(7.5, 6.5);
    s.run_ticks(1);
    assert_eq!(buffs_of(&s, a), (true, true), "inside all four");

    assert_eq!(
        swing_ticks_in(&mut s, a, 15),
        [1, 8, 15],
        "seven ticks apart, not five"
    );

    s.hold_move(a, 0.0, -1.0);
    let before = s.player(a).pos.y;
    s.run_ticks(1);
    assert!(
        close(before - s.player(a).pos.y, 0.26),
        "130 percent, not 169"
    );
}

#[test]
fn a_tower_that_is_being_built_gives_no_buff_and_the_standing_one_gives_it() {
    let mut s = Scenario::open_field();
    let builder = s.player_at(14.5, 14.5);
    s.set_stash(100);
    for _ in 0..3 {
        s.tap_tower_key(builder);
    }
    assert_eq!(s.player(builder).build_kind, StructureKind::HasteTower);
    s.hold_build(builder, 1.0, 0.0);
    s.run_ticks(1);
    assert!(
        s.sim
            .towers()
            .next()
            .is_some_and(|tower| !tower.is_standing()),
        "the site is being built"
    );
    assert_eq!(
        buffs_of(&s, builder),
        (false, false),
        "a site gives nothing"
    );

    s.run_until(200, |sim| {
        sim.towers().next().is_some_and(|tower| tower.is_standing())
    });
    s.hold(builder, PlayerInput::default());
    s.run_ticks(1);
    assert_eq!(
        buffs_of(&s, builder),
        (true, false),
        "the standing tower gives it"
    );
}

#[test]
fn a_destroyed_buff_tower_stops_giving_its_buff() {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::HasteTower, 5, 5);
    let a = s.player_at(5.5, 3.0);
    s.run_ticks(1);
    assert_eq!(buffs_of(&s, a), (true, false));

    s.set_tower_hp(1);
    s.monster_at(5.5, 7.5);
    s.run_until(200, |sim| sim.towers().count() == 0);
    s.clear_monsters();
    assert!(
        s.player(a).is_alive(),
        "the monster went for the tower only"
    );
    let lingered = ticks_while(&mut s, |s| buffs_of(s, a).0);

    assert!(
        lingered <= s.ticks(2.0),
        "the buff ran out two seconds after the tower fell, not later: {lingered}"
    );
    assert_eq!(buffs_of(&s, a), (false, false));
}

#[test]
fn a_longer_haste_is_not_cut_short_by_a_haste_tower() {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::HasteTower, 5, 5);
    let a = s.player_at(OUT_OF_RANGE.x, OUT_OF_RANGE.y);
    s.grant(a, Buff::Haste { secs: 10.0 });

    s.move_player(a, IN_RANGE);
    s.run_ticks(s.ticks(3.0));
    s.move_player(a, OUT_OF_RANGE);
    let rest = ticks_while(&mut s, |s| buffs_of(s, a).0);

    assert_eq!(
        rest + s.ticks(3.0),
        s.ticks(10.0),
        "it ends ten seconds after it was given, as it did before the tower"
    );
}

#[test]
fn a_haste_tower_does_not_shorten_a_swift_buff_and_the_two_are_held_together() {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::HasteTower, 5, 5);
    let a = s.player_at(OUT_OF_RANGE.x, OUT_OF_RANGE.y);
    s.grant(a, Buff::Swift { secs: 10.0 });

    s.move_player(a, IN_RANGE);
    s.run_ticks(1);
    assert_eq!(buffs_of(&s, a), (true, true));
    s.move_player(a, OUT_OF_RANGE);
    s.run_ticks(s.ticks(2.0));

    assert_eq!(
        buffs_of(&s, a),
        (false, true),
        "swift still has its own time"
    );
}

#[test]
fn a_downed_player_in_the_range_has_no_buff_and_a_dead_one_neither() {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::HasteTower, 5, 5);
    let downed = s.player_at(IN_RANGE.x, IN_RANGE.y);
    let dead = s.player_at(IN_RANGE.x, 4.5);
    s.run_ticks(1);
    assert_eq!(buffs_of(&s, downed), (true, false));
    s.knock_down(downed);
    s.kill(dead);

    s.run_ticks(5);

    assert_eq!(buffs_of(&s, downed), (false, false));
    assert_eq!(buffs_of(&s, dead), (false, false));
}

#[test]
fn an_away_player_in_the_range_holds_the_buff_like_any_other_player() {
    let mut s = Scenario::open_field();
    s.tower_of(StructureKind::HasteTower, 5, 5);
    let away = s.player_at(IN_RANGE.x, IN_RANGE.y);
    s.sim.set_presence(away, Presence::Away);

    s.run_ticks(1);

    assert_eq!(buffs_of(&s, away), (true, false));
}
