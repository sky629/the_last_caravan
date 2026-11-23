//! 게임의 모든 수치를 이 파일 한 곳에 둡니다.
//!
//! 서버는 `Tuning` 하나를 가지고 있고, 방을 만들 때마다 복사해서 넘깁니다.

#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    /// 서버가 1초에 실행하는 Tick의 수입니다. Tick 속도는 이 값 하나로만 정합니다.
    pub tick_hz: u32,
    /// 한 방에 들어올 수 있는 최대 인원입니다. `Sim::add_player`만 이 값을 읽습니다.
    pub max_players: usize,
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning {
            tick_hz: 20,
            max_players: 4,
        }
    }
}
