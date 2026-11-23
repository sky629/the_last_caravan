//! 규칙의 상태를 전송 형식으로 바꾸는 곳입니다.
//!
//! 전송 형식의 타입은 규칙의 타입과 따로 정의합니다. 규칙의 내부 표현이 바뀌어도 전송 형식은 `from_sim` 한 곳에서만 고칩니다.

use caravan_rules::{Sim, Vec2};
use serde::{Deserialize, Serialize};

/// Tick 하나가 끝난 뒤의 방 전체 상태입니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 서버의 Tick 번호입니다. 같은 방의 모든 클라이언트는 같은 Tick에 같은 상태를 받습니다.
    pub tick: u64,
    /// 방에 있는 플레이어의 번호입니다. 작은 번호부터 정렬되어 있습니다.
    pub players: Vec<u32>,
    pub marker: WirePos,
}

impl Snapshot {
    pub fn from_sim(sim: &Sim) -> Snapshot {
        Snapshot {
            tick: sim.tick(),
            players: sim.players().map(|id| id.0).collect(),
            marker: WirePos::from_tiles(sim.marker()),
        }
    }
}

/// 정수로 양자화한 지면 좌표입니다. 단위는 1/100 타일입니다.
/// 실수 대신 작은 정수로 보내면 메시지가 짧아집니다 (postcard는 작은 정수를 1~3바이트로 기록합니다).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePos {
    pub x: i16,
    pub y: i16,
}

impl WirePos {
    pub const UNITS_PER_TILE: f32 = 100.0;

    /// Rust의 `as` 변환은 i16의 범위를 넘는 값을 최댓값이나 최솟값으로 고정합니다.
    /// 따라서 맵 밖의 좌표도 오류 없이 경계 값이 됩니다.
    pub fn from_tiles(position: Vec2) -> WirePos {
        WirePos {
            x: (position.x * Self::UNITS_PER_TILE).round() as i16,
            y: (position.y * Self::UNITS_PER_TILE).round() as i16,
        }
    }

    pub fn to_tiles(self) -> Vec2 {
        Vec2::new(
            f32::from(self.x) / Self::UNITS_PER_TILE,
            f32::from(self.y) / Self::UNITS_PER_TILE,
        )
    }
}
