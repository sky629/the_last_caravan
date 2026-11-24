//! 장애물을 피해서 목표까지 가는 몬스터의 이동 경로입니다.
//!
//! 이동 경로의 격자는 타일 지도(`TileMap`) 그 자체입니다. 장애물, 마차, 감시탑이 모두 타일에 맞춰 놓이고
//! 몬스터의 반지름이 0.5보다 작으므로, 비어 있는 타일의 중심을 잇는 길이 곧 몬스터가 걸을 수 있는 길입니다.
//! 폭이 1인 통로도 지나가고, 연결 검사(`TileMap::is_fully_connected`)도 같은 지도를 읽으므로
//! 「길이 있다」의 뜻이 두 곳에서 어긋나지 않습니다.
//!
//! 흐름장(`FlowField`)은 도착 타일에서 각 타일까지 몇 걸음인지를 모든 타일에 적은 표입니다.
//! 몬스터는 이웃 타일 가운데 걸음 수가 가장 적은 타일로 걸어갑니다.
//! 같은 목표를 향하는 모든 몬스터가 표를 함께 쓰므로, 계산량이 몬스터의 수와 관계없습니다.
//!
//! 모든 몬스터가 공유하는 흐름장은 둘뿐이고(`Paths`), 감시탑이 몇 개이든 둘입니다.
//! - 마차 흐름장: 마차를 공격할 수 있는 타일이 도착 타일입니다.
//! - 감시탑 흐름장: 서 있는 모든 감시탑을 공격할 수 있는 타일이 한꺼번에 도착 타일입니다. 몬스터는 걸어서 가장 가까운 감시탑으로 갑니다.
//!
//! 감시탑이 생기거나 파괴되면 `World::layout_revision`이 바뀌고, `Paths::sync`가 그것을 보고 두 흐름장을 다시 만듭니다.
//! 한 Tick에 감시탑이 여럿 파괴되어도 다시 만드는 것은 한 번입니다.

use std::collections::VecDeque;

use crate::ids::Team;
use crate::math::Vec2;
use crate::tuning::Tuning;
use crate::world::{Tile, TileMap, World};

/// 도착 타일까지의 걸음 수를 모든 타일에 적은 표입니다. 닿을 수 없는 타일은 `UNREACHABLE`입니다.
pub(crate) struct FlowField {
    steps: Vec<u16>,
}

const UNREACHABLE: u16 = u16::MAX;

impl FlowField {
    /// `goals`의 모든 타일에서 동시에 퍼져 나가는 너비 우선 탐색으로 만듭니다. 도착 타일이 여럿이어도 한 번의 탐색입니다.
    pub fn toward(tiles: &TileMap, goals: &[Tile]) -> FlowField {
        let mut field = FlowField {
            steps: vec![UNREACHABLE; tiles.len()],
        };
        let mut queue = VecDeque::new();
        for goal in goals {
            if tiles.is_free(*goal) && field.steps[tiles.index_of(*goal)] == UNREACHABLE {
                field.steps[tiles.index_of(*goal)] = 0;
                queue.push_back(*goal);
            }
        }
        while let Some(tile) = queue.pop_front() {
            let next_steps = field.steps[tiles.index_of(tile)]
                .saturating_add(1)
                .min(UNREACHABLE - 1);
            for next in tiles.neighbours(tile).into_iter().flatten() {
                let slot = tiles.index_of(next);
                if field.steps[slot] == UNREACHABLE {
                    field.steps[slot] = next_steps;
                    queue.push_back(next);
                }
            }
        }
        field
    }

    /// 타일 `tile`에서 도착 타일까지의 걸음 수입니다. 닿을 수 없으면 `None`입니다.
    pub fn steps_at(&self, tiles: &TileMap, tile: Tile) -> Option<u16> {
        if !tiles.contains(tile) {
            return None;
        }
        let steps = self.steps[tiles.index_of(tile)];
        (steps != UNREACHABLE).then_some(steps)
    }

    /// `from`에 있는 몬스터가 다음에 향할 곳입니다. 걸음 수가 가장 적은 이웃 타일의 중심입니다.
    /// 이미 도착 타일에 있거나 길이 없으면 `None`이고, 호출하는 쪽이 목표를 향해 곧바로 걸어갑니다.
    pub fn next_waypoint(&self, tiles: &TileMap, from: Vec2) -> Option<Vec2> {
        let here = tiles.tile_at(from);
        let here_steps = self.steps_at(tiles, here)?;
        if here_steps == 0 {
            return None;
        }
        let mut best: Option<(u16, Tile)> = None;
        for next in tiles.neighbours(here).into_iter().flatten() {
            let Some(next_steps) = self.steps_at(tiles, next) else {
                continue;
            };
            let better = match best {
                Some((best_steps, _)) => next_steps < best_steps,
                None => next_steps < here_steps,
            };
            if better {
                best = Some((next_steps, next));
            }
        }
        best.map(|(_, tile)| tile.center())
    }
}

/// 모든 몬스터가 공유하는 이동 경로입니다. 타일 지도와 감시탑에서 얻는 값이므로 따로 고치지 않고, `sync`가 필요할 때 다시 만듭니다.
pub(crate) struct Paths {
    /// 마지막으로 만들 때의 `World::layout_revision`입니다.
    built_for: u64,
    wagon: FlowField,
    towers: FlowField,
}

impl Paths {
    pub fn new(world: &World, t: &Tuning) -> Paths {
        Paths {
            built_for: world.layout_revision(),
            wagon: wagon_field(world, t),
            towers: tower_field(world, t),
        }
    }

    /// 세계의 배치가 마지막으로 만들 때와 달라졌으면 흐름장을 다시 만듭니다.
    /// 같은 배치에서는 아무것도 하지 않으므로 몇 번을 불러도 안전합니다.
    pub fn sync(&mut self, world: &World, t: &Tuning) {
        if self.built_for != world.layout_revision() {
            *self = Paths::new(world, t);
        }
    }

    pub fn wagon(&self) -> &FlowField {
        &self.wagon
    }

    pub fn towers(&self) -> &FlowField {
        &self.towers
    }
}

fn wagon_field(world: &World, t: &Tuning) -> FlowField {
    let reach = t.monsters.longest_engage_distance();
    let goals = world.tiles.engage_tiles(world.wagon.rect, reach);
    FlowField::toward(&world.tiles, &goals)
}

/// 몬스터 진영이 공격할 수 있는 서 있는 감시탑을 향하는 흐름장입니다.
fn tower_field(world: &World, t: &Tuning) -> FlowField {
    let reach = t.monsters.longest_engage_distance();
    let goals: Vec<Tile> = world
        .towers
        .values()
        .filter(|tower| tower.is_standing() && Team::HORDE.is_hostile_to(tower.team))
        .flat_map(|tower| world.tiles.engage_tiles(tower.rect(), reach))
        .collect();
    FlowField::toward(&world.tiles, &goals)
}
