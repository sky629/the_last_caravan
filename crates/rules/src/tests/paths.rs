//! 몬스터의 이동 경로를 확인합니다: 폭이 1타일인 통로, 감시탑 쪽으로 가는 길.

use super::support::{Scenario, nights};
use crate::input::PlayerInput;
use crate::math::{Rect, Vec2};
use crate::world::{Tile, TileUse};

#[test]
fn a_one_tile_wide_corridor_is_a_path_for_monsters() {
    // x = 10..11의 벽에 폭이 1타일인 틈(y = 9..10)이 하나뿐입니다. 몬스터의 몸(지름 0.8)은 이 틈을 지나갑니다.
    let mut s = Scenario::with_tuning(|t| {
        t.map.obstacles = vec![
            Rect::new(10.0, 0.0, 11.0, 9.0),
            Rect::new(10.0, 10.0, 11.0, 20.0),
        ];
        t.map.player_spawns = vec![Vec2::new(3.0, 3.0)];
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
    });
    s.tower_at(7, 10);
    s.tower_at(4, 6);
    s.monster_at(4.5, 10.5);
    let max_hp = s.sim.tuning().structures.watchtower.max_hp;

    s.run_until(200, |sim| sim.towers().any(|tower| tower.hp() < max_hp));

    let behind_the_wall = s.sim.towers().next().unwrap();
    let reachable = s.sim.towers().nth(1).unwrap();
    assert_eq!(behind_the_wall.hp(), max_hp, "the wall hides this tower");
    assert!(reachable.hp() < max_hp, "the monster went for this one");
}

#[test]
fn a_crowd_breaks_through_a_village_full_of_towers_and_reaches_the_wagon() {
    // 감시탑이 상한만큼 마차 둘레에 서 있고, 몬스터 60마리가 몰려옵니다. 감시탑이 피해를 주지 않으므로 몬스터는 줄지 않습니다.
    // 앞줄이 감시탑을 부수면 뒤에 줄을 섰던 몬스터가 다시 걸어가므로, 무리가 감시탑 앞에서 멈춘 채로 남지 않습니다.
    let mut s = Scenario::default_game(|t| {
        t.cycle.day_secs = t.dt();
        t.nights.plans = nights(&[&[60]]);
        t.nights.first_wave_delay_secs = t.dt();
        t.nights.spawn_interval_secs = t.dt();
        t.guards.per_node = 0;
        t.wagon.max_hp = u32::MAX;
        t.structures.watchtower.damage = 0;
        t.village.start_towers = t.spread_tower_sites(t.village.tower_limit);
    });
    let towers_at_start = s.sim.towers().count();

    let took = s.run_until(s.ticks(180.0), |sim| sim.wagon().hp < u32::MAX);

    assert!(
        s.sim.wagon().hp < u32::MAX,
        "a monster reached the wagon and hit it within {took} ticks"
    );
    assert!(
        s.sim.towers().count() < towers_at_start,
        "the crowd destroyed towers on its way"
    );
}

/// 고정된 시드의 간단한 선형 합동 생성기입니다. 테스트가 매번 같은 순서로 같은 일을 하게 합니다.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn unit(&mut self) -> f32 {
        (self.next() % 2001) as f32 / 1000.0 - 1.0
    }
}

/// 타일 지도의 감시탑 타일이 감시탑 목록과 같고, 비어 있는 타일이 모두 마차에 이어져 있는지 세계를 직접 읽어서 확인합니다.
/// 이 파일에서 `&Sim`이 공개하지 않는 값을 읽는 유일한 곳입니다. 감시탑을 놓고 지우는 함수가 셋뿐이라는 약속을 이 테스트가 지킵니다.
fn assert_layout_matches_towers(s: &mut Scenario) {
    let reach = s.sim.tuning().monsters.longest_engage_distance();
    let world = s.sim.world_mut();
    let bounds = world.tiles.bounds();
    let mut on_the_map = Vec::new();
    for y in 0..bounds.max.y as u16 {
        for x in 0..bounds.max.x as u16 {
            if let Some(TileUse::Tower(id)) = world.tiles.use_at(Tile::new(x, y)) {
                on_the_map.push((id, Tile::new(x, y)));
            }
        }
    }
    let in_the_list: Vec<_> = world
        .towers
        .values()
        .map(|tower| (tower.id, tower.tile))
        .collect();
    on_the_map.sort_by_key(|(id, _)| *id);
    assert_eq!(on_the_map, in_the_list);

    let sources = world.tiles.engage_tiles(world.wagon.rect, reach);
    assert!(world.tiles.is_fully_connected(&sources, None));
}

#[test]
fn layout_matches_towers_after_any_sequence_of_builds_cancels_and_destruction() {
    let mut s = Scenario::with_tuning(|t| {
        t.wagon.start_stash = 100_000;
        t.wagon.max_hp = u32::MAX;
        t.structures.watchtower.max_hp = 40;
        t.structures.watchtower.build_secs = 0.5;
        t.village.build_range = 8.0;
        t.village.tower_limit = 30;
        t.monsters.grunt.max_hp = 100_000;
    });
    let players: Vec<_> = (0..3)
        .map(|index| s.player_at(12.0 + index as f32, 12.0))
        .collect();
    let mut random = Lcg(7);
    let (mut completed, mut cancelled, mut destroyed) = (0, 0, 0);
    let mut before: std::collections::BTreeMap<_, _> = std::collections::BTreeMap::new();
    for tick in 0..3000 {
        if tick % 9 == 0 {
            for id in &players {
                let (dx, dy) = (random.unit(), random.unit());
                s.hold(
                    *id,
                    PlayerInput {
                        move_dir: crate::input::MoveDir::new(dx, dy),
                        aim: crate::input::AimDir::new(random.unit(), random.unit()),
                        build: !random.next().is_multiple_of(3),
                        ..PlayerInput::default()
                    },
                );
            }
        }
        if tick % 60 == 0 {
            s.monster_at(11.0 + random.unit() * 4.0, 11.0 + random.unit() * 4.0);
        }
        s.run_ticks(1);
        assert_layout_matches_towers(&mut s);

        let after: std::collections::BTreeMap<_, _> = s
            .sim
            .towers()
            .map(|tower| (tower.id, tower.is_standing()))
            .collect();
        for (id, was_standing) in &before {
            match after.get(id) {
                Some(true) if !was_standing => completed += 1,
                None if *was_standing => destroyed += 1,
                None => cancelled += 1,
                _ => {}
            }
        }
        before = after;
    }
    assert!(
        completed > 0 && cancelled > 0 && destroyed > 0,
        "the run covered every way a tower can come and go: {completed} built, {cancelled} cancelled, {destroyed} destroyed"
    );
}
