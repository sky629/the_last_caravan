//! 위치와 방향을 작은 정수로 바꾸는 변환입니다. 실수와 정수 사이의 변환은 이 파일에만 있습니다.
//!
//! 몬스터가 수백 마리인 스냅샷에서는 좌표 하나의 크기가 곧 전체 크기이므로, 실수(4바이트) 대신 정수를 보냅니다.
//! postcard는 정수를 가변 길이로 기록하므로 값이 작을수록 짧아집니다.

use std::f32::consts::{PI, TAU};

use caravan_rules::{Rect, Vec2};
use serde::{Deserialize, Serialize};

/// 1/128 타일 단위로 줄인 지면 좌표입니다. 한 축이 `u16`이고, postcard는 이 값을 최대 3바이트로 기록합니다.
///
/// 단위를 1/128로 고른 이유:
/// - 128은 2의 거듭제곱이라서 `f32`로 곱하고 나누어도 오차가 생기지 않습니다. 오차는 반올림에서만 생깁니다.
/// - 최대 오차가 1/256 타일(약 0.004 타일)이고, 쿼터뷰에서 타일 하나가 24픽셀 너비이므로 화면에서 0.1픽셀보다 작습니다.
/// - 맵 24 x 24 타일은 3072 단위입니다. 가변 길이 정수는 16383까지 2바이트로 기록하므로, 맵이 128 타일에 이르기 전까지 좌표 하나는 2바이트입니다.
///
/// 맵 안의 좌표만 다룹니다. 음수와 `u16`을 넘는 값은 경계 값으로 고정되고, 유한하지 않은 값은 0이 됩니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePos {
    pub x: u16,
    pub y: u16,
}

impl WirePos {
    pub const UNITS_PER_TILE: f32 = 128.0;
    /// 맵 안의 좌표를 보냈다가 되돌릴 때 생기는 한 축의 최대 오차(타일)입니다. 가장 가까운 눈금으로 반올림하므로 눈금 간격의 절반입니다.
    pub const MAX_ERROR_TILES: f32 = 0.5 / Self::UNITS_PER_TILE;

    /// Rust의 `as` 변환은 범위를 넘는 값을 최댓값이나 최솟값으로 고정하고 NaN을 0으로 바꾸므로 오류 없이 경계 값이 됩니다.
    pub fn from_tiles(position: Vec2) -> WirePos {
        WirePos {
            x: (position.x * Self::UNITS_PER_TILE).round() as u16,
            y: (position.y * Self::UNITS_PER_TILE).round() as u16,
        }
    }

    pub fn to_tiles(self) -> Vec2 {
        Vec2::new(
            f32::from(self.x) / Self::UNITS_PER_TILE,
            f32::from(self.y) / Self::UNITS_PER_TILE,
        )
    }
}

/// 축에 정렬된 사각형입니다. 장애물, 마차, 감시탑의 위치를 보낼 때 씁니다. 두 꼭짓점은 `WirePos`와 같은 눈금입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRect {
    pub min: WirePos,
    pub max: WirePos,
}

impl WireRect {
    pub fn from_rect(rect: Rect) -> WireRect {
        WireRect {
            min: WirePos::from_tiles(rect.min),
            max: WirePos::from_tiles(rect.max),
        }
    }

    pub fn to_rect(self) -> Rect {
        let min = self.min.to_tiles();
        let max = self.max.to_tiles();
        Rect::new(min.x, min.y, max.x, max.y)
    }
}

/// 256 단계로 줄인 방향입니다. 0이 동쪽(+x)이고, 값이 1 늘 때마다 반시계 방향(+y 쪽)으로 360/256도(약 1.4도) 돕니다.
/// 1바이트이므로 postcard도 1바이트로 기록합니다.
///
/// 최대 오차는 반 단계인 `MAX_ERROR_RAD`(약 0.7도)입니다. 몬스터와 플레이어를 어느 쪽으로 돌려 그릴지 정하는 용도로는 충분합니다.
///
/// 규칙은 삼각함수를 쓰지 않지만, 이 변환은 화면에 그리는 값만 만들기 때문에 `atan2`, `cos`, `sin`을 씁니다.
/// 플랫폼에 따라 마지막 비트가 달라도 눈에 보이지 않으며, 규칙의 계산에는 쓰이지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireAngle(pub u8);

impl WireAngle {
    const STEPS: u32 = 256;
    /// 방향을 보냈다가 되돌릴 때 생기는 최대 오차(라디안)입니다.
    pub const MAX_ERROR_RAD: f32 = PI / Self::STEPS as f32;

    /// 길이가 0이거나 유한하지 않은 방향은 동쪽(0)이 됩니다. 길이는 보지 않고 방향만 봅니다.
    pub fn from_direction(direction: Vec2) -> WireAngle {
        let turns = direction.y.atan2(direction.x) / TAU;
        // `rem_euclid`가 -0.5..0.5 바퀴를 0..1 바퀴로 옮깁니다. 1에 아주 가까운 값은 반올림해서 256이 되므로 나머지를 한 번 더 구합니다.
        let steps = (turns.rem_euclid(1.0) * Self::STEPS as f32).round() as u32;
        WireAngle((steps % Self::STEPS) as u8)
    }

    /// 길이가 1인 방향입니다.
    pub fn to_direction(self) -> Vec2 {
        let radians = f32::from(self.0) * TAU / Self::STEPS as f32;
        Vec2::new(radians.cos(), radians.sin())
    }
}
