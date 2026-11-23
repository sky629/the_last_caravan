//! 장애물을 피해서 목표까지 가는 몬스터의 이동 경로입니다.
//!
//! 맵을 작은 칸으로 나눈 격자(`NavGrid`) 위에서, 목표에서 각 칸까지 몇 걸음인지를 모든 칸에 적은 흐름장(`FlowField`)을 만듭니다.
//! 몬스터는 자기 칸의 이웃 칸 가운데 걸음 수가 가장 적은 칸으로 걸어갑니다.
//! 흐름장은 목표마다 한 번만 계산하고 그 목표를 향하는 모든 몬스터가 함께 쓰므로, 계산량이 몬스터의 수와 관계없습니다.

use std::collections::VecDeque;

use crate::math::{Rect, Vec2};

/// 격자의 칸 하나입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub col: usize,
    pub row: usize,
}

/// 이웃 칸으로 가는 여덟 방향입니다. 앞의 네 개가 상하좌우이므로, 걸음 수가 같으면 상하좌우가 먼저 뽑힙니다.
const OFFSETS: [(isize, isize); 8] = [
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
const NEAREST_FREE_RINGS: isize = 8;

/// 몬스터의 중심이 있을 수 있는 칸을 정한 격자입니다.
/// 칸의 중심이 장애물이나 맵 경계에서 몬스터의 반지름보다 가까우면 막힌 칸입니다.
/// 그래서 자유로운 칸의 중심으로만 걸어가면 몸이 장애물에 걸리지 않습니다.
pub(crate) struct NavGrid {
    cols: usize,
    rows: usize,
    cell: f32,
    origin: Vec2,
    blocked: Vec<bool>,
}

impl NavGrid {
    pub fn build(bounds: Rect, solids: &[Rect], clearance: f32, cell: f32) -> NavGrid {
        let cols = (((bounds.max.x - bounds.min.x) / cell).ceil() as usize).max(1);
        let rows = (((bounds.max.y - bounds.min.y) / cell).ceil() as usize).max(1);
        let mut grid = NavGrid {
            cols,
            rows,
            cell,
            origin: bounds.min,
            blocked: vec![false; cols * rows],
        };
        let area = bounds.shrink(clearance);
        for row in 0..rows {
            for col in 0..cols {
                let center = grid.center(Cell { col, row });
                let blocked = !area.contains(center)
                    || solids
                        .iter()
                        .any(|solid| solid.distance_to(center) < clearance);
                grid.blocked[row * cols + col] = blocked;
            }
        }
        grid
    }

    /// 점이 속한 칸입니다. 격자 밖의 점은 가장 가까운 가장자리 칸으로 봅니다.
    pub fn cell_of(&self, p: Vec2) -> Cell {
        // 음수와 NaN을 `as usize`로 바꾸면 0이 되고, 너무 큰 값은 최댓값이 됩니다. 뒤에서 격자 안으로 맞춥니다.
        let col = (((p.x - self.origin.x) / self.cell) as usize).min(self.cols - 1);
        let row = (((p.y - self.origin.y) / self.cell) as usize).min(self.rows - 1);
        Cell { col, row }
    }

    pub fn center(&self, c: Cell) -> Vec2 {
        Vec2::new(
            self.origin.x + (c.col as f32 + 0.5) * self.cell,
            self.origin.y + (c.row as f32 + 0.5) * self.cell,
        )
    }

    fn index(&self, c: Cell) -> usize {
        c.row * self.cols + c.col
    }

    pub fn is_free(&self, c: Cell) -> bool {
        !self.blocked[self.index(c)]
    }

    /// 점에서 가장 가까운 자유로운 칸입니다. 벽에 붙어 선 플레이어는 몬스터에게 막힌 칸에 서 있을 수 있으므로,
    /// 점이 속한 칸에서 바깥으로 고리 모양으로 넓혀 가며 찾습니다.
    pub fn nearest_free(&self, p: Vec2) -> Option<Cell> {
        let here = self.cell_of(p);
        for ring in 0..=NEAREST_FREE_RINGS {
            for d_row in -ring..=ring {
                for d_col in -ring..=ring {
                    if d_row.abs().max(d_col.abs()) != ring {
                        continue;
                    }
                    if let Some(cell) = self.offset(here, d_col, d_row)
                        && self.is_free(cell)
                    {
                        return Some(cell);
                    }
                }
            }
        }
        None
    }

    /// 중심이 `rect`에서 `reach` 이내에 있는 자유로운 칸입니다. 시설을 공격할 수 있는 칸들이며, 흐름장의 도착 칸이 됩니다.
    pub fn cells_near(&self, rect: Rect, reach: f32) -> Vec<Cell> {
        let mut cells = Vec::new();
        for row in 0..self.rows {
            for col in 0..self.cols {
                let cell = Cell { col, row };
                if self.is_free(cell) && rect.distance_to(self.center(cell)) <= reach {
                    cells.push(cell);
                }
            }
        }
        cells
    }

    fn offset(&self, c: Cell, d_col: isize, d_row: isize) -> Option<Cell> {
        let col = c.col.checked_add_signed(d_col)?;
        let row = c.row.checked_add_signed(d_row)?;
        (col < self.cols && row < self.rows).then_some(Cell { col, row })
    }

    /// 몬스터가 한 걸음에 갈 수 있는 이웃 칸입니다. 자유로운 칸만 포함합니다.
    /// 대각선 걸음은 그 옆의 상하좌우 두 칸이 모두 자유로울 때만 허용하므로, 경로가 벽의 모서리를 가로지르지 않습니다.
    fn neighbours(&self, c: Cell) -> [Option<Cell>; 8] {
        let mut result = [None; 8];
        for (slot, (d_col, d_row)) in OFFSETS.iter().enumerate() {
            let Some(next) = self.offset(c, *d_col, *d_row) else {
                continue;
            };
            if !self.is_free(next) {
                continue;
            }
            let is_diagonal = *d_col != 0 && *d_row != 0;
            if is_diagonal {
                let side_a = self.offset(c, *d_col, 0);
                let side_b = self.offset(c, 0, *d_row);
                let both_free = matches!((side_a, side_b), (Some(a), Some(b)) if self.is_free(a) && self.is_free(b));
                if !both_free {
                    continue;
                }
            }
            result[slot] = Some(next);
        }
        result
    }
}

/// 목표까지의 걸음 수를 모든 칸에 적은 표입니다. 닿을 수 없는 칸은 `UNREACHABLE`입니다.
pub(crate) struct FlowField {
    steps: Vec<u16>,
}

const UNREACHABLE: u16 = u16::MAX;

impl FlowField {
    /// `goals`의 모든 칸에서 동시에 퍼져 나가는 너비 우선 탐색으로 만듭니다.
    pub fn toward(grid: &NavGrid, goals: &[Cell]) -> FlowField {
        let mut steps = vec![UNREACHABLE; grid.cols * grid.rows];
        let mut queue = VecDeque::new();
        for goal in goals {
            if grid.is_free(*goal) {
                steps[grid.index(*goal)] = 0;
                queue.push_back(*goal);
            }
        }
        while let Some(cell) = queue.pop_front() {
            let next_steps = steps[grid.index(cell)]
                .saturating_add(1)
                .min(UNREACHABLE - 1);
            for next in grid.neighbours(cell).into_iter().flatten() {
                let slot = grid.index(next);
                if steps[slot] == UNREACHABLE {
                    steps[slot] = next_steps;
                    queue.push_back(next);
                }
            }
        }
        FlowField { steps }
    }

    /// `from`에 있는 몬스터가 다음에 향할 곳입니다. 걸음 수가 가장 적은 이웃 칸의 중심입니다.
    /// 이미 도착 칸에 있거나 길이 없으면 `None`이고, 호출하는 쪽이 목표를 향해 곧바로 걸어갑니다.
    pub fn next_waypoint(&self, grid: &NavGrid, from: Vec2) -> Option<Vec2> {
        let here = grid.cell_of(from);
        let here_steps = self.steps[grid.index(here)];
        if here_steps == 0 {
            return None;
        }
        let mut best: Option<(u16, Cell)> = None;
        for next in grid.neighbours(here).into_iter().flatten() {
            let next_steps = self.steps[grid.index(next)];
            let better = match best {
                Some((best_steps, _)) => next_steps < best_steps,
                None => next_steps < here_steps,
            };
            if better {
                best = Some((next_steps, next));
            }
        }
        best.map(|(_, cell)| grid.center(cell))
    }
}
