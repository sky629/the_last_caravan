//! `Sim`은 방 하나의 게임 상태 전체이며, 규칙 크레이트의 공개 진입점입니다.

use std::collections::BTreeSet;
use std::f32::consts::TAU;

use crate::ids::PlayerId;
use crate::math::Vec2;
use crate::tuning::Tuning;

/// 표식(marker)이 원점 둘레를 도는 원의 반지름입니다 (타일).
const MARKER_RADIUS: f32 = 4.0;
/// 표식이 원을 한 바퀴 도는 데 걸리는 시간입니다 (초).
const MARKER_PERIOD_SECS: f32 = 4.0;

/// 방 하나의 게임입니다. 바깥 코드는 `&Sim`으로 상태를 읽고,
/// `add_player`, `remove_player`, `step`으로만 상태를 바꿉니다.
pub struct Sim {
    tuning: Tuning,
    /// 방이 만들어진 뒤 실행한 Tick의 수입니다. 줄어들지 않으므로 스냅샷의 Tick 번호는 계속 커집니다.
    tick: u64,
    players: BTreeSet<PlayerId>,
    next_player: u32,
    /// 서버가 위치를 계산하고 모든 클라이언트가 그리는 사각형 하나입니다.
    /// 정해진 원을 따라 Tick마다 조금씩 움직입니다.
    marker: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinError {
    RoomFull { capacity: usize },
}

impl Sim {
    pub fn new(tuning: Tuning) -> Sim {
        let marker = marker_position(0, tuning.tick_hz);
        Sim {
            tuning,
            tick: 0,
            players: BTreeSet::new(),
            next_player: 1,
            marker,
        }
    }

    /// 새 플레이어를 추가하고 번호를 돌려줍니다. 방이 가득 차 있으면 거절합니다.
    pub fn add_player(&mut self) -> Result<PlayerId, JoinError> {
        if self.players.len() >= self.tuning.max_players {
            return Err(JoinError::RoomFull {
                capacity: self.tuning.max_players,
            });
        }
        let id = PlayerId(self.next_player);
        self.next_player += 1;
        self.players.insert(id);
        Ok(id)
    }

    /// 플레이어를 즉시 제거합니다. 없는 번호는 무시하므로 같은 퇴장을 두 번 처리해도 안전합니다.
    pub fn remove_player(&mut self, id: PlayerId) {
        self.players.remove(&id);
    }

    /// 게임을 Tick 하나만큼 진행합니다.
    pub fn step(&mut self) {
        self.tick += 1;
        self.marker = marker_position(self.tick, self.tuning.tick_hz);
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn tuning(&self) -> &Tuning {
        &self.tuning
    }

    /// 번호가 작은 플레이어부터 차례로 돌려줍니다.
    pub fn players(&self) -> impl Iterator<Item = PlayerId> {
        self.players.iter().copied()
    }

    pub fn marker(&self) -> Vec2 {
        self.marker
    }
}

/// `tick`번째 Tick에서 표식이 있어야 하는 위치입니다.
/// 한 바퀴에 걸리는 시간을 초로 정했으므로, Tick 속도가 바뀌어도 화면에서 도는 속도는 같습니다.
fn marker_position(tick: u64, tick_hz: u32) -> Vec2 {
    let ticks_per_turn = MARKER_PERIOD_SECS * tick_hz as f32;
    let turn = (tick as f32 % ticks_per_turn) / ticks_per_turn;
    let angle = turn * TAU;
    Vec2::new(MARKER_RADIUS * angle.cos(), MARKER_RADIUS * angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close_to(actual: Vec2, expected: Vec2) -> bool {
        actual.distance(expected) < 1e-3
    }

    #[test]
    fn a_new_room_starts_at_tick_zero_and_each_step_adds_one() {
        let mut sim = Sim::new(Tuning::default());
        assert_eq!(sim.tick(), 0);
        sim.step();
        sim.step();
        assert_eq!(sim.tick(), 2);
    }

    #[test]
    fn players_get_increasing_ids_until_the_room_is_full() {
        let tuning = Tuning {
            max_players: 2,
            ..Tuning::default()
        };
        let mut sim = Sim::new(tuning);
        assert_eq!(sim.add_player(), Ok(PlayerId(1)));
        assert_eq!(sim.add_player(), Ok(PlayerId(2)));
        assert_eq!(sim.add_player(), Err(JoinError::RoomFull { capacity: 2 }));
        assert_eq!(
            sim.players().collect::<Vec<_>>(),
            [PlayerId(1), PlayerId(2)]
        );
    }

    #[test]
    fn a_removed_player_frees_a_seat_but_its_id_is_not_reused() {
        let tuning = Tuning {
            max_players: 2,
            ..Tuning::default()
        };
        let mut sim = Sim::new(tuning);
        let first = sim.add_player().unwrap();
        sim.add_player().unwrap();
        sim.remove_player(first);
        sim.remove_player(first);
        assert_eq!(sim.add_player(), Ok(PlayerId(3)));
        assert_eq!(
            sim.players().collect::<Vec<_>>(),
            [PlayerId(2), PlayerId(3)]
        );
    }

    #[test]
    fn the_marker_circles_the_origin_once_every_four_seconds() {
        let tuning = Tuning::default();
        let ticks_per_second = tuning.tick_hz as u64;
        let mut sim = Sim::new(tuning);
        assert!(close_to(sim.marker(), Vec2::new(4.0, 0.0)));

        let mut seen_quarter_turn = false;
        for tick in 1..=4 * ticks_per_second {
            let before = sim.marker();
            sim.step();
            assert_ne!(sim.marker(), before, "the marker moves on tick {tick}");
            if tick == ticks_per_second {
                assert!(close_to(sim.marker(), Vec2::new(0.0, 4.0)));
                seen_quarter_turn = true;
            }
        }
        assert!(seen_quarter_turn);
        assert!(close_to(sim.marker(), Vec2::new(4.0, 0.0)));
    }
}
