//! 파괴병을 확인합니다: 플레이어를 지나쳐서 거리와 관계없이 걸어서 가장 가까운 포탑으로 가는 것, 포탑이 부서지거나 새로 서면 목표가 바뀌는 것,
//! 걸어서 닿는 포탑이 없으면(서 있어도 상자로 둘레가 막힌 포탑만 있는 경우를 포함합니다) 마차로 가는 것, 포탑에 주는 피해와 포탑에게서 받는 피해, 수치, 효과, 건설 규칙이 허락하는 가장 빽빽한 마을에서도 포탑이나 마차에 닿는 것.
//! 파괴병이 무리의 반대쪽에서 나타나는 것은 `days.rs`가 확인합니다.

use std::cmp::Ordering;

use super::support::{Scenario, close};
use crate::build::check_site;
use crate::expedition::Phase;
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, PlayerInput};
use crate::kind::{MonsterKind, StructureKind, WeaponKind};
use crate::math::Rect;
use crate::structures::{Tower, TowerState};
use crate::tuning::{Debuff, Prey, Tuning};
use crate::world::{Hit, Source, Target, Tile};

/// 마을을 채울 때 타일을 시험하는 순서입니다.
type TileOrder = fn(&Tile, &Tile) -> Ordering;

/// 감시탑의 HP입니다. 파괴병의 한 번의 공격(포탑에 40)은 이보다 훨씬 적습니다.
const TOWER_HP: u32 = 300;

/// 파괴병이 포탑 하나를 한 번 때려서 주는 피해입니다. 기본 몬스터의 피해 10에 포탑 피해 비율 400%를 곱한 값입니다.
const SIEGE_HIT: u32 = 40;

/// 파괴병이 죽지 않게 HP를 크게 한 들판입니다. 파괴병을 시험하는 테스트가 이 들판에서 시작합니다.
fn field(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.sapper.max_hp = 100_000;
        t.monsters.grunt.max_hp = 100_000;
        adjust(t);
    })
}

/// 처음 놓은 포탑 말고 `index`번째로 놓은 포탑의 HP입니다.
fn hp_of_tower(s: &Scenario, index: usize) -> u32 {
    s.sim.towers().nth(index).expect("the tower exists").hp()
}

#[test]
fn a_sapper_walks_past_a_player_beside_its_way_to_a_tower_far_beyond_a_grunts_chase_range_and_hits_only_the_tower()
 {
    // 파괴병은 y = 3.5의 줄을 따라 서쪽의 감시탑(타일 (2, 3))으로 갑니다. 플레이어는 줄에서 0.7타일 떨어진 곳에 서 있어서,
    // 파괴병이 곁을 지나가는 동안 플레이어가 파괴병의 공격이 닿는 거리 안에 있습니다. 감시탑은 10타일 넘게 떨어져 있어서 기본 몬스터의 추적 거리(5타일) 밖이고,
    // 플레이어는 출발한 자리에서 4.5타일 떨어져 있어서 기본 몬스터라면 추적 거리 안입니다.
    let mut s = field(|_| {});
    let player = s.player_at(9.0, 4.2);
    s.tower_at(2, 3);
    s.monster_of(MonsterKind::Sapper, 13.5, 3.5);

    s.run_until(400, |sim| sim.towers().any(|tower| tower.hp() < TOWER_HP));

    let t = s.sim.tuning();
    assert_eq!(
        s.player(player).hp(),
        t.player.max_hp,
        "the player was not hit"
    );
    assert_eq!(
        s.sim.wagon().hp,
        t.wagon.max_hp,
        "the wagon was not the target"
    );
    assert_eq!(s.tower().hp(), TOWER_HP - SIEGE_HIT);
}

#[test]
fn a_grunt_in_the_same_place_does_hit_the_player_so_that_the_sapper_test_tells_the_two_apart() {
    let mut s = field(|_| {});
    let player = s.player_at(9.0, 4.2);
    s.tower_at(2, 3);
    s.monster_of(MonsterKind::Grunt, 13.5, 3.5);

    s.run_until(400, |sim| {
        sim.player(player)
            .is_some_and(|player| player.hp() < sim.tuning().player.max_hp)
    });

    assert!(s.player(player).hp() < s.sim.tuning().player.max_hp);
}

#[test]
fn a_sapper_that_a_player_hits_does_not_turn_on_the_player_and_goes_on_to_the_tower() {
    let mut s = field(|t| t.monsters.sapper.max_hp = 1000);
    let swordsman = s.player_at(9.0, 3.5);
    s.tower_at(2, 3);
    let sapper = s.monster_of(MonsterKind::Sapper, 13.5, 3.5);
    s.hold(
        swordsman,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    );
    s.run_until(200, |sim| {
        sim.monsters()
            .next()
            .is_some_and(|monster| monster.hp < 1000)
    });
    s.hold(swordsman, PlayerInput::default());
    assert!(s.monster(sapper).unwrap().hp < 1000, "the sword landed");

    s.run_until(400, |sim| sim.towers().any(|tower| tower.hp() < TOWER_HP));

    assert_eq!(
        s.player(swordsman).hp(),
        s.sim.tuning().player.max_hp,
        "the sapper never hit the player who hit it"
    );
    assert!(
        s.monster(sapper).unwrap().pos.x < 9.0,
        "it went past the player to the tower"
    );
}

#[test]
fn a_sapper_walks_to_the_tower_nearest_by_path_not_by_straight_line_however_far_it_is() {
    // 파괴병은 (4.5, 10.5)에 있습니다. 동쪽 감시탑은 직선으로 2.5타일이지만 벽(x = 6..7, y = 1..16)에 가려서 열여섯 걸음쯤 돌아가야 하고,
    // 남쪽 감시탑은 직선으로 7.5타일이지만 걸어서 여덟 걸음입니다. 기본 몬스터의 추적 거리(5)를 둘 다 넘습니다.
    let mut s = field(|t| t.map.obstacles = vec![Rect::new(6.0, 1.0, 7.0, 16.0)]);
    s.tower_at(7, 10);
    s.tower_at(4, 2);
    s.monster_of(MonsterKind::Sapper, 4.5, 10.5);

    s.run_until(300, |sim| sim.towers().any(|tower| tower.hp() < TOWER_HP));

    assert_eq!(hp_of_tower(&s, 0), TOWER_HP, "the wall hides this tower");
    assert_eq!(hp_of_tower(&s, 1), TOWER_HP - SIEGE_HIT);
}

#[test]
fn a_sapper_goes_on_to_the_next_nearest_tower_when_the_one_it_was_hitting_falls() {
    // 첫째 포탑은 한 번 맞으면 부서지는 HP이고, 둘째 포탑은 첫째 포탑에서 7타일 떨어져 있어서 기본 몬스터의 추적 거리 밖입니다.
    let mut s = field(|_| {});
    s.tower_at(3, 3);
    s.set_tower_hp(SIEGE_HIT);
    s.tower_at(3, 10);
    s.monster_of(MonsterKind::Sapper, 4.5, 3.5);

    s.run_until(100, |sim| sim.towers().count() == 1);
    assert_eq!(
        hp_of_tower(&s, 0),
        TOWER_HP,
        "the second tower is still whole"
    );

    s.run_until(400, |sim| sim.towers().any(|tower| tower.hp() < TOWER_HP));

    assert_eq!(hp_of_tower(&s, 0), TOWER_HP - SIEGE_HIT);
    assert_eq!(
        s.sim.wagon().hp,
        s.sim.tuning().wagon.max_hp,
        "a tower still stood, so the wagon was not the target"
    );
}

#[test]
fn a_sapper_with_no_tower_to_hit_goes_to_the_wagon_and_hits_it_for_a_grunts_damage() {
    let mut s = field(|_| {});
    s.monster_of(MonsterKind::Sapper, 12.0, 18.0);

    s.run_until(400, |sim| sim.wagon().hp < 500);

    assert_eq!(
        s.sim.wagon().hp,
        500 - 10,
        "the siege percent does not touch the wagon"
    );
}

#[test]
fn a_sapper_whose_last_tower_falls_walks_on_to_the_wagon() {
    let mut s = field(|_| {});
    s.tower_at(3, 3);
    s.set_tower_hp(SIEGE_HIT);
    s.monster_of(MonsterKind::Sapper, 4.5, 3.5);

    s.run_until(100, |sim| sim.towers().count() == 0);
    s.run_until(800, |sim| sim.wagon().hp < 500);

    assert_eq!(s.sim.wagon().hp, 500 - 10);
}

/// 감시탑을 지을 자리 (12, 15)의 곁에 플레이어가 서서 짓고, 파괴병이 서쪽에서 마차로 걸어오는 들판입니다. 감시탑이 지어지는 시간만 `build_secs`로 정합니다.
fn field_with_a_tower_being_built(build_secs: f32) -> (Scenario, PlayerId, MonsterId) {
    let mut s = field(|t| t.structures.watchtower.build_secs = build_secs);
    let builder = s.player_at(13.5, 15.5);
    s.hold_build(builder, -1.0, 0.0);
    let sapper = s.monster_of(MonsterKind::Sapper, 3.5, 12.5);
    (s, builder, sapper)
}

#[test]
fn a_tower_that_is_still_being_built_is_not_a_target_for_a_sapper() {
    // 짓는 데 1000초가 걸려서 지어지지 않는 포탑 곁을 파괴병이 지나 마차를 때립니다. 짓는 중인 포탑은 포탑 흐름장의 도착 타일이 아니므로 목표가 아닙니다.
    let (mut s, _, sapper) = field_with_a_tower_being_built(1000.0);

    s.run_until(1000, |sim| sim.wagon().hp < 500);

    let towers: Vec<&Tower> = s.sim.towers().collect();
    assert_eq!(towers.len(), 1);
    assert!(
        matches!(towers[0].state, TowerState::Building { .. }),
        "still being built"
    );
    assert_eq!(s.sim.wagon().hp, 500 - 10, "the sapper came to the wagon");
    assert!(s.monster(sapper).unwrap().pos.x > 12.0, "past the tower");
}

#[test]
fn a_tower_that_stands_at_last_becomes_a_target_for_a_sapper_that_was_walking_to_the_wagon() {
    // 1초 만에 지어지는 포탑이 서면, 마차로 걷던 파괴병이 돌아서서 그 포탑을 때립니다.
    let (mut s, builder, sapper) = field_with_a_tower_being_built(1.0);
    s.run_until(40, |sim| sim.towers().any(Tower::is_standing));
    assert_eq!(s.sim.towers().count(), 1);
    assert!(
        s.monster(sapper).unwrap().last_attack_tick.is_none(),
        "it had hit nothing before the tower stood"
    );
    s.hold(builder, PlayerInput::default());

    s.run_until(600, |sim| sim.towers().any(|tower| tower.hp() < TOWER_HP));

    assert_eq!(s.tower().hp(), TOWER_HP - SIEGE_HIT);
    assert_eq!(
        s.sim.wagon().hp,
        500,
        "it went for the tower, not the wagon"
    );
}

#[test]
fn a_standing_tower_that_no_free_tile_reaches_is_not_a_target_for_a_sapper() {
    // 타일 (5, 5)의 감시탑은 서 있지만 둘레의 여덟 타일에 무기 상자가 있어서, 파괴병이 공격할 수 있는 자리에 걸어서 닿지 못합니다.
    // 파괴병은 마차보다 이 감시탑에 훨씬 가까운 자리(감시탑까지 약 7타일, 마차까지 약 14타일)에서 출발하지만, 마차로 걸어가서 마차를 때립니다.
    let mut s = field(|_| {});
    s.tower_at(5, 5);
    for (x, y) in [
        (4, 4),
        (5, 4),
        (6, 4),
        (4, 5),
        (6, 5),
        (4, 6),
        (5, 6),
        (6, 6),
    ] {
        s.sim
            .world_mut()
            .add_chest(Tile::new(x, y), WeaponKind::Sword);
    }
    s.monster_of(MonsterKind::Sapper, 3.5, 12.5);

    s.run_until(800, |sim| sim.wagon().hp < 500);

    assert!(s.tower().is_standing(), "the tower stands");
    assert_eq!(s.tower().hp(), TOWER_HP, "no free tile reaches the tower");
    assert_eq!(s.sim.wagon().hp, 500 - 10, "the sapper came to the wagon");
}

/// 파괴병 하나(`kind`가 `Sapper`이면)가 포탑 곁에 서서 포탑이 부서질 때까지 걸린 Tick입니다. 감시탑은 사격하지 않습니다.
fn ticks_to_break_a_tower(kind: MonsterKind) -> u32 {
    let mut s = field(|_| {});
    s.tower_at(5, 5);
    s.monster_of(kind, 6.5, 5.5);
    s.run_until(2000, |sim| sim.towers().count() == 0)
}

#[test]
fn a_sapper_breaks_a_watchtower_in_eight_hits_where_a_grunt_needs_thirty() {
    // 감시탑의 HP는 300입니다. 파괴병은 1초마다 40이라서 여덟 번째(첫 공격이 1 Tick, 그다음부터 20 Tick마다)에 부수고, 기본 몬스터는 10이라서 서른 번째에 부숩니다.
    assert_eq!(
        (
            ticks_to_break_a_tower(MonsterKind::Sapper),
            ticks_to_break_a_tower(MonsterKind::Grunt)
        ),
        (1 + 20 * 7, 1 + 20 * 29)
    );
}

#[test]
fn a_sapper_is_slower_than_a_grunt_and_has_more_hp_and_hits_the_wagon_later_from_the_same_start() {
    let t = Tuning::default();
    assert!(t.monsters.sapper.speed < t.monsters.grunt.speed);
    assert!(t.monsters.sapper.max_hp > t.monsters.grunt.max_hp);

    let ticks_to_the_wagon = |kind| {
        let mut s = Scenario::with_tuning(|_| {});
        s.monster_of(kind, 5.0, 18.0);
        s.run_until(400, |sim| sim.wagon().hp < 500)
    };
    let (sapper, grunt) = (
        ticks_to_the_wagon(MonsterKind::Sapper),
        ticks_to_the_wagon(MonsterKind::Grunt),
    );
    // 마차의 가장자리까지 12타일이고 공격이 닿는 곳(0.9타일 앞)까지는 11.1타일입니다. 1.7타일/초는 한 Tick에 0.085타일이므로 약 131 Tick이 걸리고,
    // 기본 몬스터는 2.2타일/초에 약 101 Tick입니다. 둘 다 처음 한 걸음과 첫 공격까지의 몇 Tick이 더해집니다.
    assert_eq!((sapper, grunt), (133, 103));
}

#[test]
fn a_sapper_adds_nothing_to_the_largest_body_or_the_longest_reach_the_rules_read() {
    let monsters = Tuning::default().monsters;
    let (sapper, grunt) = (&monsters.sapper, &monsters.grunt);

    assert!(sapper.radius <= grunt.radius);
    assert!(sapper.radius + sapper.attack_reach <= grunt.radius + grunt.attack_reach);
    assert_eq!(monsters.largest_radius(), grunt.radius);
    assert_eq!(
        monsters.longest_engage_distance(),
        grunt.radius + grunt.attack_reach
    );
    assert_eq!(sapper.prey, Prey::Towers);
    assert_eq!(sapper.siege_percent, 400);
}

/// `START`에 선 파괴병 하나와 멀리 선 플레이어 한 명이 있고 포탑이 없는 들판입니다. 파괴병은 마차로 걷습니다.
fn walking_sapper(adjust: impl FnOnce(&mut Tuning)) -> (Scenario, MonsterId, PlayerId) {
    let mut s = field(adjust);
    let owner = s.player_at(2.0, 2.0);
    let sapper = s.monster_of(MonsterKind::Sapper, 5.0, 18.0);
    (s, sapper, owner)
}

/// 몬스터가 한 Tick에 걷는 거리입니다. 이 함수가 한 Tick을 진행합니다.
fn walked_in_a_tick(s: &mut Scenario, id: MonsterId) -> f32 {
    let before = s.monster(id).unwrap().pos;
    s.run_ticks(1);
    s.monster(id).unwrap().pos.distance(before)
}

#[test]
fn a_slowed_sapper_walks_at_the_slowed_speed_for_the_time_of_the_slow() {
    let (mut s, sapper, owner) = walking_sapper(|_| {});
    let full = walked_in_a_tick(&mut s, sapper);
    assert!(
        close(full, 1.7 * 0.05),
        "1.7 tiles a second at 20 Hz: {full}"
    );

    s.afflict(
        sapper,
        Debuff::Slow {
            factor: 0.5,
            secs: 2.0,
        },
        owner,
    );

    for tick in 1..=s.ticks(2.0) {
        let walked = walked_in_a_tick(&mut s, sapper);
        assert!(
            close(walked, full * 0.5),
            "tick {tick} of the slow: {walked}"
        );
    }
    assert!(close(walked_in_a_tick(&mut s, sapper), full));
}

#[test]
fn a_stunned_sapper_stands_still_for_the_time_of_the_stun() {
    let (mut s, sapper, owner) = walking_sapper(|_| {});
    let from = s.monster(sapper).unwrap().pos;

    s.afflict(sapper, Debuff::Stun { secs: 0.8 }, owner);
    s.run_ticks(s.ticks(0.8));

    assert_eq!(s.monster(sapper).unwrap().pos, from);
    s.run_ticks(1);
    assert_ne!(s.monster(sapper).unwrap().pos, from, "it walks again");
}

#[test]
fn a_bleeding_sapper_loses_hp_once_a_second() {
    let (mut s, sapper, owner) = walking_sapper(|t| {
        t.monsters.sapper.speed = 0.0;
        t.monsters.sapper.separation_speed = 0.0;
    });
    let full = s.monster(sapper).unwrap().hp;

    s.afflict(
        sapper,
        Debuff::Bleed {
            damage: 5,
            secs: 3.0,
        },
        owner,
    );
    s.run_ticks(s.ticks(1.0) - 1);
    assert_eq!(s.monster(sapper).unwrap().hp, full, "not yet");
    s.run_ticks(1);
    assert_eq!(s.monster(sapper).unwrap().hp, full - 5);
}

/// 건설 규칙(`check_site`)이 허락하는 곳마다 감시탑을 세워서 마을을 채웁니다. `order`가 타일을 시험하는 순서이고, 이 순서로 놓을 수 있는 타일에 차례로 세우므로
/// 더는 놓을 수 있는 타일이 없을 때까지 채웁니다. 포탑의 상한은 풀어서(`tower_limit`) 규칙이 허락하는 가장 빽빽한 배치를 만듭니다.
fn fill_the_village(s: &mut Scenario, order: impl FnMut(&Tile, &Tile) -> Ordering) {
    let t = s.sim.tuning().clone();
    let builder = s.player_at(2.0, 2.0);
    let (cols, rows) = s.sim.tiles().size();
    let mut tiles: Vec<Tile> = (0..cols)
        .flat_map(|x| (0..rows).map(move |y| Tile::new(x, y)))
        .collect();
    tiles.sort_by(order);
    let world = s.sim.world_mut();
    for tile in tiles {
        if check_site(world, &t, builder, tile, None).is_ok() {
            let kind = StructureKind::Watchtower;
            world.add_tower(kind, tile, Tower::standing_state(kind, &t));
        }
    }
}

/// 비어 있는 모든 타일 가운데 마차의 중심에서 `radius`타일 안에 있는 타일입니다.
fn free_tiles_near_the_wagon(s: &Scenario, radius: f32) -> Vec<Tile> {
    let tiles = s.sim.tiles();
    let center = s.sim.wagon().rect.center();
    let (cols, rows) = tiles.size();
    (0..cols)
        .flat_map(|x| (0..rows).map(move |y| Tile::new(x, y)))
        .filter(|tile| tiles.is_free(*tile) && tile.center().distance(center) <= radius)
        .collect()
}

/// 마을을 `order`의 순서로 가득 채운 기본 지도에서, 비어 있는 모든 타일(마차에서 `radius`타일 안)마다 파괴병 하나를 놓고 포탑이나 마차를 때리는지 봅니다.
/// 때리지 못하고 서 있는 자리의 목록을 돌려줍니다. 서 있는 자리는 `limit` Tick 안에 아무것도 때리지 못한 자리입니다.
fn tiles_where_a_sapper_hits_nothing(
    order: impl FnMut(&Tile, &Tile) -> Ordering,
    radius: f32,
    limit: u32,
) -> (Vec<Tile>, usize, usize) {
    let mut s = Scenario::default_game(|t| {
        t.village.tower_limit = 10_000;
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
        t.loot.chests_per_day = 0;
        t.monsters.sapper.max_hp = 1_000_000;
        t.structures.watchtower.max_hp = 10_000;
    });
    s.sim.set_phase(Phase::Day {
        day: 1,
        ticks_left: u32::MAX,
    });
    fill_the_village(&mut s, order);
    let t = s.sim.tuning().clone();
    let towers = s.sim.towers().count();
    let starts = free_tiles_near_the_wagon(&s, radius);
    s.run_ticks(1);

    let mut standing = Vec::new();
    for start in &starts {
        s.clear_monsters();
        s.monster_of(MonsterKind::Sapper, start.center().x, start.center().y);
        let hit = (0..limit).any(|_| {
            s.run_ticks(1);
            s.sim.wagon().hp < t.wagon.max_hp
                || s.sim
                    .towers()
                    .any(|tower| tower.hp() < t.structures.watchtower.max_hp)
        });
        if !hit {
            standing.push(*start);
        }
        s.set_wagon_hp(t.wagon.max_hp);
        for tower in s.sim.world_mut().towers.values_mut() {
            tower.state = Tower::standing_state(tower.kind, &t);
        }
    }
    (standing, towers, starts.len())
}

fn distance_from_the_wagon(tile: &Tile) -> f32 {
    let wagon = Tuning::default().map.wagon;
    wagon.center().distance(tile.center())
}

#[test]
fn a_sapper_hits_a_tower_or_the_wagon_from_every_free_tile_of_the_densest_village_the_build_rules_allow()
 {
    // 규칙이 허락하는 한 마을을 가득 채운 배치를 네 가지 순서로 만듭니다. 마차에 가까운 곳부터, 먼 곳부터, 줄을 따라, 섞은 순서입니다.
    let orders: [(&str, TileOrder); 4] = [
        ("nearest the wagon first", |a, b| {
            distance_from_the_wagon(a).total_cmp(&distance_from_the_wagon(b))
        }),
        ("farthest from the wagon first", |a, b| {
            distance_from_the_wagon(b).total_cmp(&distance_from_the_wagon(a))
        }),
        ("row by row", |a, b| (a.y, a.x).cmp(&(b.y, b.x))),
        ("scrambled", |a, b| {
            let scramble = |tile: &Tile| {
                (u32::from(tile.x).wrapping_mul(2_654_435_761)
                    ^ u32::from(tile.y).wrapping_mul(40_503))
                .rotate_left(7)
            };
            scramble(a).cmp(&scramble(b))
        }),
    ];
    for (name, order) in orders {
        let (standing, towers, starts) = tiles_where_a_sapper_hits_nothing(order, 13.0, 1200);
        println!(
            "{name}: {towers} towers, {starts} start tiles, {} stand",
            standing.len()
        );
        assert!(
            towers > 40,
            "{name}: a village of {towers} towers is denser than the limit"
        );
        assert!(
            standing.is_empty(),
            "{name}: a sapper that starts on {standing:?} hits nothing in 1200 ticks"
        );
    }
}

#[test]
fn a_sapper_takes_half_of_a_towers_damage_rounded_up_and_all_of_a_players_damage() {
    let mut s = field(|t| t.monsters.sapper.max_hp = 1000);
    let owner = s.player_at(2.0, 2.0);
    let sapper = s.monster_of(MonsterKind::Sapper, 10.0, 10.0);
    let t = s.sim.tuning().clone();
    let lost_to = |s: &mut Scenario, by: Source, amount| {
        let before = s.monster(sapper).unwrap().hp;
        s.sim.world_mut().apply_hits(
            &[Hit {
                target: Target::Monster(sapper),
                amount,
                by,
                debuff: None,
            }],
            &t,
        );
        before - s.monster(sapper).unwrap().hp
    };

    assert_eq!(lost_to(&mut s, Source::Tower, 100), 50);
    assert_eq!(
        lost_to(&mut s, Source::Tower, 15),
        8,
        "half of the watchtower's 15 is 7.5, rounded up to 8"
    );
    assert_eq!(lost_to(&mut s, Source::Player(owner), 100), 100);
    assert_eq!(lost_to(&mut s, Source::Player(owner), 15), 15);
}

#[test]
fn a_watchtower_shot_takes_eight_hp_off_a_sapper() {
    let mut s = field(|_| {});
    s.tower_at(1, 17);
    let sapper = s.monster_of(MonsterKind::Sapper, 4.0, 17.5);

    s.run_ticks(1);

    assert_eq!(100_000 - s.monster(sapper).expect("alive").hp, 8);
}
