//! 주변 몬스터를 찾는 균일 격자입니다.
//!
//! 「이 위치에서 반지름 r 안에 있는 몬스터」를 묻는 함수는 `MonsterGrid::query` 하나뿐입니다.
//! 몬스터 사이의 밀어내기, 근접 공격의 대상 찾기, 포탑의 목표 찾기가 모두 이 함수를 씁니다.
//! 몬스터를 모두 서로 비교하면 몬스터 수의 제곱에 비례해서 느려지지만, 격자는 가까운 칸만 보므로 몬스터 수에 비례합니다.
//!
//! 격자는 Tick마다 다시 만듭니다. 몬스터는 Tick마다 움직이므로, 변경을 추적하는 것보다 다시 만드는 편이 단순하고 빠릅니다.

use crate::math::{Rect, Vec2};
use crate::monster::Monster;

pub(crate) struct MonsterGrid {
    origin: Vec2,
    cell: f32,
    cols: usize,
    rows: usize,
    /// 칸 `c`에 들어 있는 몬스터는 `entries[starts[c]..starts[c + 1]]`입니다. 길이는 칸의 수 더하기 1입니다.
    starts: Vec<usize>,
    entries: Vec<Entry>,
    /// `rebuild`가 칸마다 다음에 넣을 자리를 기록하는 작업용 배열입니다.
    cursors: Vec<usize>,
}

#[derive(Clone, Copy)]
struct Entry {
    /// `World::monsters`에서의 위치입니다.
    index: usize,
    pos: Vec2,
}

impl MonsterGrid {
    pub fn new(bounds: Rect, cell: f32) -> MonsterGrid {
        let cols = (((bounds.max.x - bounds.min.x) / cell).ceil() as usize).max(1);
        let rows = (((bounds.max.y - bounds.min.y) / cell).ceil() as usize).max(1);
        MonsterGrid {
            origin: bounds.min,
            cell,
            cols,
            rows,
            starts: vec![0; cols * rows + 1],
            entries: Vec::new(),
            cursors: vec![0; cols * rows],
        }
    }

    /// 몬스터의 현재 위치로 격자를 다시 만듭니다. 칸마다 몬스터를 세고, 누적합으로 시작 위치를 정한 뒤, 차례로 채웁니다.
    /// 같은 칸 안에서는 몬스터의 순서가 유지되므로, 같은 상태에서는 `query`가 항상 같은 순서로 답합니다.
    pub fn rebuild(&mut self, monsters: &[Monster]) {
        self.starts.fill(0);
        for monster in monsters {
            let cell = self.cell_index(monster.pos);
            self.starts[cell + 1] += 1;
        }
        for cell in 0..self.cursors.len() {
            self.starts[cell + 1] += self.starts[cell];
        }
        let cells = self.cursors.len();
        self.cursors.copy_from_slice(&self.starts[..cells]);

        self.entries.clear();
        self.entries.resize(
            monsters.len(),
            Entry {
                index: 0,
                pos: Vec2::ZERO,
            },
        );
        for (index, monster) in monsters.iter().enumerate() {
            let cell = self.cell_index(monster.pos);
            self.entries[self.cursors[cell]] = Entry {
                index,
                pos: monster.pos,
            };
            self.cursors[cell] += 1;
        }
    }

    /// 중심이 `center`에서 `radius` 이내에 있는 몬스터의 위치(`World::monsters`의 번호)를 `out`에 채웁니다.
    /// 호출할 때마다 `out`을 비웁니다. 호출하는 쪽이 `out`을 재사용하면 Tick마다 메모리를 새로 할당하지 않습니다.
    pub fn query(&self, center: Vec2, radius: f32, out: &mut Vec<usize>) {
        out.clear();
        let (first_col, first_row) = self.cell_coords(center - Vec2::new(radius, radius));
        let (last_col, last_row) = self.cell_coords(center + Vec2::new(radius, radius));
        let radius_squared = radius * radius;
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                let cell = row * self.cols + col;
                for entry in &self.entries[self.starts[cell]..self.starts[cell + 1]] {
                    if (entry.pos - center).length_squared() <= radius_squared {
                        out.push(entry.index);
                    }
                }
            }
        }
    }

    /// 점이 속한 칸의 열과 행입니다. 격자 밖의 점은 가장 가까운 가장자리 칸으로 봅니다.
    fn cell_coords(&self, p: Vec2) -> (usize, usize) {
        // 음수와 NaN을 `as usize`로 바꾸면 0이 되고, 너무 큰 값은 최댓값이 됩니다. 뒤에서 격자 안으로 맞춥니다.
        let col = (((p.x - self.origin.x) / self.cell) as usize).min(self.cols - 1);
        let row = (((p.y - self.origin.y) / self.cell) as usize).min(self.rows - 1);
        (col, row)
    }

    fn cell_index(&self, p: Vec2) -> usize {
        let (col, row) = self.cell_coords(p);
        row * self.cols + col
    }
}
