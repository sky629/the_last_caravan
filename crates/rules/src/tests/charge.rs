//! 보스의 돌진: 보스에서 대상 쪽으로 뻗는 띠의 예고, 예고된 뒤에 띠에서 벗어난 플레이어가 맞지 않는 것,
//! 한 대상이 한 번만 맞는 것, 지형과 포탑과 마차와 맵의 끝 앞에서 멈추는 것.

use super::boss::{act, boss_tuning, hit_boss, hp_of_player, put_boss, skip_the_rest, with_boss};
use super::support::{Scenario, close, nights};
use crate::boss::Act;
use crate::ids::PlayerId;
use crate::kind::StructureKind;
use crate::math::{Rect, Vec2};
use crate::player::Presence;
use crate::structures::Tower;
use crate::telegraph::Zone;
use crate::tuning::{Move, Tuning};
use crate::world::Tile;

/// 돌진만 되풀이하는 보스와 서 있기만 하는 몬스터가 있는 20 x 20 들판입니다. 마차는 죽지 않습니다. `adjust`가 지형을 더합니다.
pub(super) fn charge_field(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    Scenario::with_cycle(|t| {
        with_boss(t);
        t.monsters.boss.speed = 0.0;
        t.monsters.grunt.speed = 0.0;
        boss_tuning(t).script = vec![Move::Charge];
        t.nights.plans = nights(&[&[1]]);
        t.wagon.max_hp = u32::MAX;
        adjust(t);
    })
}

/// 보스를 `boss`에, 보스가 노릴 플레이어를 `player`에 놓고 보스가 돌진을 예고하는 Tick까지 진행합니다.
pub(super) fn winding_toward(s: &mut Scenario, boss: (f32, f32), player: (f32, f32)) -> PlayerId {
    let id = s.player_at(player.0, player.1);
    put_boss(s, boss.0, boss.1, 4);
    hit_boss(s, id, 10);
    s.run_ticks(1);
    skip_the_rest(s);
    s.run_until(5, |sim| {
        matches!(sim.boss().map(|boss| &boss.act), Some(Act::Winding { .. }))
    });
    id
}

/// 보스가 돌진을 마치고 다시 걷는 Tick까지 진행합니다.
pub(super) fn run_until_walking(s: &mut Scenario) -> u32 {
    s.run_until(400, |sim| {
        matches!(sim.boss().map(|boss| &boss.act), Some(Act::Walking { .. }))
    })
}

/// 예고하거나 달리는 중인 띠의 시작, 끝, 반폭입니다.
fn lane_of(s: &Scenario) -> (Vec2, Vec2, f32) {
    let zone = match act(s) {
        Act::Winding { telegraph, .. } => telegraph.zone,
        Act::Charging(charge) => charge.zone(),
        other => panic!("the boss is not charging: {other:?}"),
    };
    match zone {
        Zone::Lane {
            from,
            to,
            half_width,
        } => (from, to, half_width),
        other => panic!("a charge is a lane: {other:?}"),
    }
}

fn boss_x(s: &Scenario) -> f32 {
    let id = s.sim.boss().expect("a boss").id;
    s.monster(id).expect("the body").pos.x
}

#[test]
fn the_charge_is_telegraphed_for_one_point_five_seconds_as_a_lane_from_the_boss_toward_the_aim() {
    let mut s = charge_field(|_| {});
    winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));

    let (from, to, half_width) = lane_of(&s);
    assert_eq!(from, Vec2::new(3.0, 10.0));
    assert_eq!(half_width, 1.0, "two tiles wide");
    assert!(
        close(to.x, 3.0 + 10.0 + 1.0) && close(to.y, 10.0),
        "ten tiles and the front half width: {to:?}"
    );
    let Act::Winding { telegraph, .. } = act(&s) else {
        panic!("winding");
    };
    assert_eq!(telegraph.ticks_left, s.ticks(1.5));
}

#[test]
fn the_lane_does_not_turn_while_it_is_shown_even_when_the_aim_runs_around() {
    let mut s = charge_field(|_| {});
    let player = winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));
    let fixed = lane_of(&s);

    s.hold_move(player, 0.0, 1.0);
    for _ in 0..s.ticks(1.5) - 1 {
        s.run_ticks(1);
        assert_eq!(lane_of(&s), fixed, "the lane stays");
    }
}

#[test]
fn a_player_who_leaves_the_lane_is_not_hit_even_when_the_input_is_four_tenths_of_a_second_late() {
    let mut s = charge_field(|_| {});
    let player = winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));
    s.run_ticks(s.ticks(0.4));
    s.hold_move(player, 0.0, 1.0);

    run_until_walking(&mut s);

    assert_eq!(hp_of_player(&s, player), 100);
}

#[test]
fn a_player_who_stays_in_the_lane_is_hit_once_and_not_once_per_tick() {
    let mut s = charge_field(|_| {});
    let player = winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));

    run_until_walking(&mut s);

    assert_eq!(hp_of_player(&s, player), 100 - 40);
}

#[test]
fn a_charge_ends_where_its_lane_ends_and_the_boss_walks_again() {
    let mut s = charge_field(|_| {});
    winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));
    let (_, to, half_width) = lane_of(&s);

    run_until_walking(&mut s);

    assert!(
        close(boss_x(&s), to.x - half_width),
        "{} against the lane end {to:?}",
        boss_x(&s)
    );
    assert!(matches!(act(&s), Act::Walking { .. }));
}

#[test]
fn a_charge_hits_a_tower_on_its_lane_once_and_not_the_one_beside_it() {
    let mut s = charge_field(|_| {});
    // 띠는 y = 10.5를 따라 폭 2타일이므로 y가 9.5에서 11.5 사이입니다. 포탑 하나는 띠 안에, 하나는 띠 옆에 섭니다.
    winding_toward(&mut s, (3.0, 10.5), (7.0, 10.5));
    let on_the_lane = s.tower_at(12, 10);
    let beside = s.tower_at(12, 13);

    run_until_walking(&mut s);

    let full = s.sim.tuning().structures.watchtower.max_hp;
    let hp = |id| s.sim.towers().find(|tower| tower.id == id).map(Tower::hp);
    assert_eq!(hp(on_the_lane), Some(full - 100));
    assert_eq!(hp(beside), Some(full));
}

#[test]
fn a_charge_toward_the_wagon_hits_it_and_stops_in_front_of_it() {
    let mut s = charge_field(|_| {});
    // 마차의 사각형은 (17, 17)에서 (19, 19)까지입니다. 보스가 y = 18을 따라 동쪽으로 달립니다.
    winding_toward(&mut s, (9.0, 18.0), (13.0, 18.0));
    s.set_wagon_hp(500);

    run_until_walking(&mut s);

    assert_eq!(s.sim.wagon().hp, 500 - 80, "once");
    let x = boss_x(&s);
    assert!(
        x <= 17.0 - 0.4 + 1e-3 && x > 16.0,
        "stopped in front of the wagon: {x}"
    );
}

#[test]
fn a_charge_stops_at_a_wall_and_at_the_edge_of_the_map_and_never_leaves_the_map() {
    // 벽: 타일 (10, 9)와 (10, 10)이 막혀 있습니다.
    let mut s = charge_field(|t| {
        t.map.obstacles.push(Rect::new(10.0, 9.0, 11.0, 11.0));
        t.map.player_spawns = vec![Vec2::new(5.0, 5.0)];
    });
    winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));
    let (_, to, half_width) = lane_of(&s);
    assert!(
        to.x <= 10.0 - 0.4 + half_width + 1e-3,
        "the lane shown ends where the boss will stop, not behind the wall: {to:?}"
    );
    run_until_walking(&mut s);
    let x = boss_x(&s);
    assert!(
        x <= 10.0 - 0.4 + 1e-3 && x > 9.0,
        "in front of the wall: {x}"
    );

    // 맵의 끝: 폭이 20타일입니다.
    let mut s = charge_field(|_| {});
    winding_toward(&mut s, (10.0, 10.0), (14.0, 10.0));
    run_until_walking(&mut s);
    let x = boss_x(&s);
    assert!(
        x <= 20.0 - 0.4 + 1e-3 && x > 19.0,
        "in front of the edge: {x}"
    );
}

#[test]
fn a_boss_that_stands_against_a_wall_runs_the_whole_lane_away_from_it() {
    // 벽(타일 (5, 9), (5, 10))의 동쪽 가장자리에 보스의 몸이 닿아 있습니다. 보스의 중심은 벽에서 반지름만큼 떨어진 x = 6.4입니다.
    let mut s = charge_field(|t| {
        t.map.obstacles.push(Rect::new(5.0, 9.0, 6.0, 11.0));
        t.map.player_spawns = vec![Vec2::new(2.0, 2.0)];
    });
    winding_toward(&mut s, (6.4, 10.0), (10.0, 10.0));

    let (from, to, half_width) = lane_of(&s);

    assert!(
        close(to.x - from.x, 10.0 + half_width),
        "ten tiles and the front half width: {from:?} to {to:?}"
    );
}

#[test]
fn a_player_who_is_away_in_the_lane_is_not_hit_and_a_monster_in_it_is_not_either() {
    let mut s = charge_field(|_| {});
    // 노려지는 플레이어는 띠의 끝 쪽에 서서 보스가 8타일 안에서 돌진을 시작하게 합니다.
    winding_toward(&mut s, (3.0, 10.0), (10.0, 10.0));
    let away = s.player_at(8.0, 10.2);
    let grunt = s.monster_at(9.0, 10.0);
    s.sim.set_presence(away, Presence::Away);

    run_until_walking(&mut s);

    assert_eq!(hp_of_player(&s, away), 100);
    assert_eq!(
        s.monster(grunt).expect("alive").hp,
        s.sim.tuning().monsters.grunt.max_hp
    );
}

#[test]
fn a_tower_raised_beside_the_middle_of_a_shown_lane_stops_the_charge_instead_of_bending_it() {
    for gap in [0.05_f32, 0.15, 0.3] {
        let mut s = charge_field(|_| {});
        let line = 10.0 - gap;
        let target = s.player_at(8.0, line);
        // 구경꾼은 예고된 띠의 경계에서 0.05타일 바깥에 서 있습니다.
        let bystander = s.player_at(9.5, line - 1.05);
        put_boss(&mut s, 3.0, line, 4);
        hit_boss(&mut s, target, 10);
        s.run_ticks(1);
        skip_the_rest(&mut s);
        s.run_until(5, |sim| {
            matches!(sim.boss().map(|boss| &boss.act), Some(Act::Winding { .. }))
        });
        // 예고가 보인 뒤에 띠의 가운데 선 곁의 타일에 포탑이 섭니다. 보스의 걷는 몸이 그 모서리에 걸립니다.
        let state = Tower::standing_state(StructureKind::Watchtower, s.sim.tuning());
        s.sim
            .world_mut()
            .add_tower(StructureKind::Watchtower, Tile::new(6, 10), state);
        let boss = s.sim.boss().expect("the boss").id;

        run_until_walking(&mut s);

        let end = s.monster(boss).expect("the boss body").pos;
        assert!(
            end.x < 6.0,
            "gap {gap}: the boss stops before the tower, at {end:?}"
        );
        assert!(
            close(end.y, line),
            "gap {gap}: the boss stays on the line, at {end:?}"
        );
        assert_eq!(hp_of_player(&s, bystander), 100, "gap {gap}");
    }
}
