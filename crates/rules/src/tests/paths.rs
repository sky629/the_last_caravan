//! 몬스터의 이동 경로를 확인합니다: 폭이 1타일인 통로, 감시탑 쪽으로 가는 길.

use super::support::Scenario;
use crate::kind::StructureKind;
use crate::math::{Rect, Vec2};
use crate::tuning::StructurePlacement;
use crate::world::Tile;

#[test]
fn a_one_tile_wide_corridor_is_a_path_for_monsters() {
    // x = 10..11의 벽에 폭이 1타일인 틈(y = 9..10)이 하나뿐입니다. 몬스터의 몸(지름 0.8)은 이 틈을 지나갑니다.
    let mut s = Scenario::with_tuning(|t| {
        t.map.obstacles = vec![
            Rect::new(10.0, 0.0, 11.0, 9.0),
            Rect::new(10.0, 10.0, 11.0, 20.0),
        ];
        t.map.player_spawns = vec![Vec2::new(3.0, 3.0)];
        t.map.monster_spawns = vec![Vec2::new(3.0, 0.6)];
    });
    let monster = s.monster_at(5.5, 9.5);

    s.run_until(900, |sim| sim.wagon().hp < 500);

    let pos = s.monster(monster).unwrap().pos;
    assert!(
        pos.x > 11.0,
        "it passed through the gap and ended up on the wagon's side, {pos:?}"
    );
}

#[test]
fn a_monster_chasing_a_tower_walks_to_the_tower_nearest_by_path_not_by_straight_line() {
    // 몬스터는 (4.5, 10.5)에 있습니다. 오른쪽 감시탑은 직선으로 2.5타일이지만 벽(x = 6..7, y = 1..16)에 가려서
    // 걸어서는 열한 걸음을 돌아가야 하고, 아래쪽 감시탑은 직선으로 3.5타일이지만 걸어서 세 걸음입니다.
    let mut s = Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 100_000;
        t.map.obstacles = vec![Rect::new(6.0, 1.0, 7.0, 16.0)];
        t.map.structures = vec![
            StructurePlacement {
                kind: StructureKind::Watchtower,
                tile: Tile::new(7, 10),
            },
            StructurePlacement {
                kind: StructureKind::Watchtower,
                tile: Tile::new(4, 6),
            },
        ];
    });
    s.monster_at(4.5, 10.5);
    let max_hp = s.sim.tuning().structures.watchtower.max_hp;

    s.run_until(200, |sim| sim.towers().any(|tower| tower.hp() < max_hp));

    let behind_the_wall = s.sim.towers().next().unwrap();
    let reachable = s.sim.towers().nth(1).unwrap();
    assert_eq!(behind_the_wall.hp(), max_hp, "the wall hides this tower");
    assert!(reachable.hp() < max_hp, "the monster went for this one");
}
