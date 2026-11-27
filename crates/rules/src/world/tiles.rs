//! 타일 지도입니다. 맵의 모든 타일이 무엇으로 차 있는지를 적은 표이며, 「어디가 막혀 있는가」를 아는 곳은 이 표 하나입니다.
//!
//! 충돌, 몬스터의 이동 경로, 지도의 연결 검사가 모두 같은 표를 읽습니다.
//! 장애물, 마차, 채집 지점, 포탑, 무기 상자는 타일에 맞춰 놓이므로(정수 좌표) 사각형 목록을 따로 들고 있지 않습니다.
//!
//! 표를 바꾸는 `set`은 `world` 모듈 안에서만 부를 수 있습니다(`pub(super)`). 다른 모듈이 표를 직접 고치면 컴파일되지 않습니다.
//! 포탑을 놓는 일은 `World::add_tower`가 표와 포탑 목록을 함께 고칩니다.
//!
//! 타일 하나는 한 변이 1인 정사각형입니다. 플레이어와 몬스터의 반지름이 0.5보다 작으므로(`Tuning::validate`가 검사합니다),
//! 비어 있는 타일의 중심을 따라 걸으면 몸이 이웃한 막힌 타일에 걸리지 않고, 폭이 1인 통로도 지나갑니다.

use std::collections::BTreeSet;
use std::ops::Range;

use crate::ids::{ChestId, NodeId, TowerId};
use crate::math::{Rect, Vec2};
use crate::tuning::MapSpec;

/// 타일 하나의 위치입니다. 왼쪽 아래 타일이 (0, 0)이고 x는 동쪽, y는 북쪽입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    pub x: u16,
    pub y: u16,
}

impl Tile {
    pub const fn new(x: u16, y: u16) -> Tile {
        Tile { x, y }
    }

    /// 타일이 차지하는 사각형입니다. `[x, x + 1] x [y, y + 1]`입니다.
    pub fn rect(self) -> Rect {
        Rect::new(
            f32::from(self.x),
            f32::from(self.y),
            f32::from(self.x) + 1.0,
            f32::from(self.y) + 1.0,
        )
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(f32::from(self.x) + 0.5, f32::from(self.y) + 0.5)
    }

    /// `dx`, `dy`만큼 옮긴 타일입니다. `u16`의 범위를 벗어나면 `None`입니다. 맵 안인지는 `TileMap::contains`가 따로 확인합니다.
    pub fn step(self, dx: i32, dy: i32) -> Option<Tile> {
        let x = u16::try_from(i32::from(self.x) + dx).ok()?;
        let y = u16::try_from(i32::from(self.y) + dy).ok()?;
        Some(Tile { x, y })
    }

    /// 가로 `cols`, 세로 `rows` 타일인 맵에서 `p`가 속한 타일입니다. 맵 밖의 점은 가장 가까운 가장자리 타일로 봅니다.
    pub(crate) fn containing(p: Vec2, cols: u16, rows: u16) -> Tile {
        // 음수와 NaN을 `as u16`으로 바꾸면 0이 되고, 너무 큰 값은 최댓값이 됩니다. 뒤에서 맵 안으로 맞춥니다.
        let x = (p.x as u16).min(cols - 1);
        let y = (p.y as u16).min(rows - 1);
        Tile { x, y }
    }
}

/// 타일 하나를 차지하고 있는 것입니다. 한 타일에는 하나만 있습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TileUse {
    Free,
    Obstacle,
    Wagon,
    /// 채집 지점입니다. 원정 동안 그 자리에 그대로 있습니다.
    Node(NodeId),
    Tower(TowerId),
    /// 무기 상자입니다. 낮이 시작될 때 놓이고, 밤이 시작되면 사라집니다.
    Chest(ChestId),
}

/// 상하좌우 네 방향 뒤에 대각선 네 방향이 오는 이웃 타일의 차이입니다.
/// 걸음 수가 같으면 상하좌우가 먼저 뽑힙니다.
const OFFSETS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// `nearest_free`가 살펴보는 가장 바깥 고리의 크기입니다.
const NEAREST_FREE_RINGS: i32 = 8;

pub(crate) struct TileMap {
    cols: u16,
    rows: u16,
    uses: Vec<TileUse>,
    /// 비어 있는 타일의 수입니다. `set`이 값을 바꿀 때마다 함께 고치므로, 연결 검사가 타일을 다시 세지 않습니다.
    free_count: usize,
}

impl TileMap {
    /// 장애물과 마차를 표에 옮깁니다. 채집 지점은 `World`가 원정을 시작할 때, 포탑은 `add_tower`로 하나씩 놓습니다.
    /// 맵의 크기와 사각형이 정수 좌표라는 전제는 `Tuning::validate`가 검사합니다.
    pub fn from_map(map: &MapSpec) -> TileMap {
        let cols = map.width as u16;
        let rows = map.height as u16;
        let count = usize::from(cols) * usize::from(rows);
        let mut tiles = TileMap {
            cols,
            rows,
            uses: vec![TileUse::Free; count],
            free_count: count,
        };
        for obstacle in &map.obstacles {
            tiles.fill(*obstacle, TileUse::Obstacle);
        }
        tiles.fill(map.wagon, TileUse::Wagon);
        tiles
    }

    /// `rect`가 덮는 타일을 모두 `value`로 바꿉니다. 맵 밖의 타일은 건너뜁니다.
    fn fill(&mut self, rect: Rect, value: TileUse) {
        for y in span(rect.min.y, rect.max.y, self.rows) {
            for x in span(rect.min.x, rect.max.x, self.cols) {
                self.set(Tile { x, y }, value);
            }
        }
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, f32::from(self.cols), f32::from(self.rows))
    }

    pub fn contains(&self, tile: Tile) -> bool {
        tile.x < self.cols && tile.y < self.rows
    }

    /// 타일을 차지하고 있는 것입니다. 맵 밖의 타일은 `None`입니다.
    pub fn use_at(&self, tile: Tile) -> Option<TileUse> {
        self.contains(tile).then(|| self.uses[self.index_of(tile)])
    }

    /// 맵 안에 있고 비어 있는 타일입니다.
    pub fn is_free(&self, tile: Tile) -> bool {
        self.use_at(tile) == Some(TileUse::Free)
    }

    /// `p`가 속한 타일입니다. 맵 밖의 점은 가장 가까운 가장자리 타일로 봅니다.
    pub fn tile_at(&self, p: Vec2) -> Tile {
        Tile::containing(p, self.cols, self.rows)
    }

    /// 맵의 가로와 세로 타일 수입니다.
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// 표를 바꾸는 유일한 함수입니다. `world` 모듈 밖에서는 부를 수 없습니다. 맵 밖의 타일은 무시합니다.
    pub(super) fn set(&mut self, tile: Tile, value: TileUse) {
        if !self.contains(tile) {
            return;
        }
        let index = self.index_of(tile);
        let was_free = self.uses[index] == TileUse::Free;
        let now_free = value == TileUse::Free;
        self.uses[index] = value;
        match (was_free, now_free) {
            (true, false) => self.free_count -= 1,
            (false, true) => self.free_count += 1,
            _ => {}
        }
    }

    /// `center`에서 반지름 `radius`의 원이 겹칠 수 있는 타일 가운데 막힌 타일의 사각형을 `out`에 채웁니다.
    /// 호출할 때마다 `out`을 비웁니다. 충돌 처리가 이동할 때마다 부르므로 호출하는 쪽이 `out`을 재사용합니다.
    /// 맵 밖의 타일은 건너뜁니다. 맵 경계는 충돌 처리가 따로 막습니다.
    pub fn blocked_rects_near(&self, center: Vec2, radius: f32, out: &mut Vec<Rect>) {
        out.clear();
        for y in span(center.y - radius, center.y + radius, self.rows) {
            for x in span(center.x - radius, center.x + radius, self.cols) {
                let tile = Tile { x, y };
                if self.uses[self.index_of(tile)] != TileUse::Free {
                    out.push(tile.rect());
                }
            }
        }
    }

    /// 중심이 `rect`에서 `reach` 이내에 있는 비어 있는 타일입니다.
    /// 몬스터가 `rect`를 공격할 수 있는 자리이며, 흐름장의 도착 타일이 됩니다.
    pub fn engage_tiles(&self, rect: Rect, reach: f32) -> Vec<Tile> {
        let mut tiles = Vec::new();
        for y in span(rect.min.y - reach, rect.max.y + reach, self.rows) {
            for x in span(rect.min.x - reach, rect.max.x + reach, self.cols) {
                let tile = Tile { x, y };
                if self.is_free(tile) && rect.distance_to(tile.center()) <= reach {
                    tiles.push(tile);
                }
            }
        }
        tiles
    }

    /// `p`에서 가장 가까운 비어 있는 타일입니다. `p`가 속한 타일에서 바깥으로 고리 모양으로 넓혀 가며 찾고,
    /// 고리 몇 개 안에 없으면 `None`입니다.
    pub fn nearest_free(&self, p: Vec2) -> Option<Tile> {
        let here = self.tile_at(p);
        for ring in 0..=NEAREST_FREE_RINGS {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs().max(dy.abs()) != ring {
                        continue;
                    }
                    if let Some(tile) = here.step(dx, dy)
                        && self.is_free(tile)
                    {
                        return Some(tile);
                    }
                }
            }
        }
        None
    }

    /// `tile`에 이웃한 여덟 타일 가운데 비어 있는 타일입니다. 상하좌우가 먼저이고 대각선이 뒤입니다.
    /// 막힌 타일(채집 지점) 둘레에서 서 있을 자리를 찾을 때 씁니다. 대각선 걸음의 제약(`neighbours`)은 없습니다.
    pub fn free_around(&self, tile: Tile) -> Vec<Tile> {
        OFFSETS
            .iter()
            .filter_map(|(dx, dy)| tile.step(*dx, *dy))
            .filter(|next| self.is_free(*next))
            .collect()
    }

    /// 한 걸음에 갈 수 있는 이웃 타일 여덟 곳 가운데 비어 있는 곳입니다. 앞의 네 칸이 상하좌우입니다.
    /// 대각선 걸음은 그 옆의 상하좌우 두 타일이 모두 비어 있을 때만 허용하므로, 경로가 막힌 타일의 모서리를 가로지르지 않습니다.
    pub fn neighbours(&self, tile: Tile) -> [Option<Tile>; 8] {
        let mut result = [None; 8];
        for (slot, (dx, dy)) in OFFSETS.iter().enumerate() {
            let Some(next) = tile.step(*dx, *dy) else {
                continue;
            };
            if !self.is_free(next) {
                continue;
            }
            let is_diagonal = *dx != 0 && *dy != 0;
            if is_diagonal {
                let side_a = tile.step(*dx, 0);
                let side_b = tile.step(0, *dy);
                let both_free = matches!((side_a, side_b), (Some(a), Some(b)) if self.is_free(a) && self.is_free(b));
                if !both_free {
                    continue;
                }
            }
            result[slot] = Some(next);
        }
        result
    }

    /// 비어 있는 타일이 모두 `sources`에서 걸어서 닿는지 검사합니다. `extra_blocked`의 타일도 막힌 것으로 봅니다.
    ///
    /// 상하좌우로만 이어지는 연결을 셉니다. 대각선으로만 맞닿은 두 타일 사이는 몸이 지나가지 못하기 때문입니다.
    /// 닿은 타일의 수를 비어 있는 타일의 수(`extra_blocked` 가운데 비어 있는 타일을 뺀 수, 같은 타일은 한 번만 뺍니다)와 비교합니다.
    /// 그래서 처음부터 모두 이어져 있는 지도에서만 의미가 있고, `Tuning::validate`가 이 조건을 검사합니다.
    pub fn is_fully_connected(&self, sources: &[Tile], extra_blocked: &[Tile]) -> bool {
        let blocked: BTreeSet<Tile> = extra_blocked
            .iter()
            .copied()
            .filter(|tile| self.is_free(*tile))
            .collect();
        let open = |tile: Tile| self.is_free(tile) && !blocked.contains(&tile);
        let expected = self.free_count - blocked.len();

        let mut reached = vec![false; self.uses.len()];
        let mut queue = Vec::new();
        let mut reached_count = 0;
        for source in sources {
            let slot = self.index_of(*source);
            if open(*source) && !reached[slot] {
                reached[slot] = true;
                reached_count += 1;
                queue.push(*source);
            }
        }
        while let Some(tile) = queue.pop() {
            for (dx, dy) in &OFFSETS[..4] {
                let Some(next) = tile.step(*dx, *dy) else {
                    continue;
                };
                if !open(next) {
                    continue;
                }
                let slot = self.index_of(next);
                if !reached[slot] {
                    reached[slot] = true;
                    reached_count += 1;
                    queue.push(next);
                }
            }
        }
        reached_count == expected
    }

    /// 타일의 번호입니다. 타일마다 값을 하나씩 적는 표(`FlowField`)가 같은 번호로 칸을 찾습니다. 타일은 맵 안에 있어야 합니다.
    pub fn index_of(&self, tile: Tile) -> usize {
        usize::from(tile.y) * usize::from(self.cols) + usize::from(tile.x)
    }

    /// 타일의 수입니다.
    pub fn len(&self) -> usize {
        self.uses.len()
    }
}

/// 구간 `[lo, hi]`에 걸치는 타일의 번호 범위입니다. 맵 밖의 번호(`limit` 이상)와 음수는 뺍니다.
fn span(lo: f32, hi: f32, limit: u16) -> Range<u16> {
    // 음수와 NaN을 `as u16`으로 바꾸면 0이 되고, 너무 큰 값은 최댓값이 됩니다. 뒤에서 맵 안으로 맞춥니다.
    let first = (lo.floor() as u16).min(limit);
    let end = (hi.ceil() as u16).min(limit);
    first..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::Tuning;

    /// 가로 `width`, 세로 `height` 타일의 빈 지도에 `blocked` 타일만 막은 표입니다.
    fn map_with(width: u16, height: u16, blocked: &[(u16, u16)]) -> TileMap {
        let mut map = Tuning::default().map;
        map.width = f32::from(width);
        map.height = f32::from(height);
        map.obstacles = blocked
            .iter()
            .map(|(x, y)| Tile::new(*x, *y).rect())
            .collect();
        map.wagon = Rect::new(0.0, 0.0, 0.0, 0.0);
        TileMap::from_map(&map)
    }

    #[test]
    fn a_diagonal_step_is_not_allowed_around_a_blocked_corner() {
        let tiles = map_with(3, 3, &[(1, 0)]);

        let from_origin = tiles.neighbours(Tile::new(0, 0));

        assert!(from_origin.contains(&Some(Tile::new(0, 1))));
        assert!(
            !from_origin.contains(&Some(Tile::new(1, 1))),
            "the blocked tile beside it forbids the diagonal"
        );
        assert!(!from_origin.contains(&Some(Tile::new(1, 0))));
    }

    #[test]
    fn free_tiles_cut_off_by_a_wall_are_not_fully_connected() {
        let split = map_with(5, 3, &[(2, 0), (2, 1), (2, 2)]);
        let open = map_with(5, 3, &[(2, 0), (2, 1)]);

        assert!(!split.is_fully_connected(&[Tile::new(0, 0)], &[]));
        assert!(open.is_fully_connected(&[Tile::new(0, 0)], &[]));
    }

    #[test]
    fn two_tiles_touching_only_at_a_corner_are_not_connected() {
        // 가운데 대각선으로만 이어진 두 칸: (0,1)과 (1,0)은 모서리에서만 닿습니다.
        let diagonal = map_with(2, 2, &[(0, 0), (1, 1)]);

        assert!(!diagonal.is_fully_connected(&[Tile::new(0, 1)], &[]));
    }

    #[test]
    fn a_tile_that_would_close_the_last_gap_is_found_by_the_extra_block() {
        let corridor = map_with(5, 3, &[(2, 0), (2, 2)]);

        assert!(corridor.is_fully_connected(&[Tile::new(0, 0)], &[]));
        assert!(
            !corridor.is_fully_connected(&[Tile::new(0, 0)], &[Tile::new(2, 1)]),
            "blocking the only gap cuts the map in two"
        );
        assert!(
            corridor.is_fully_connected(&[Tile::new(0, 0)], &[Tile::new(4, 1)]),
            "a dead end that is blocked leaves the rest connected"
        );
        assert!(
            corridor.is_fully_connected(&[Tile::new(0, 0)], &[Tile::new(4, 1), Tile::new(4, 1)]),
            "a tile named twice counts once"
        );
        assert!(
            !corridor.is_fully_connected(&[Tile::new(0, 0)], &[Tile::new(4, 1), Tile::new(2, 1)]),
            "one of the blocked tiles cuts the map even when another does not"
        );
    }
}
