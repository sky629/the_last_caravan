//! 지면 좌표를 나타내는 2차원 벡터와 축에 정렬된 사각형입니다. 단위는 타일이고, x는 동쪽, y는 북쪽입니다.
//!
//! 외부 수학 크레이트 대신 직접 정의합니다. 규칙 크레이트가 다른 크레이트의 타입을 노출하지 않게 하기 위해서입니다.
//! 삼각함수는 쓰지 않습니다. 사칙연산과 제곱근은 어느 플랫폼에서나 같은 결과를 내기 때문입니다.

use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Vec2 {
        Vec2 { x, y }
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn distance(self, other: Vec2) -> f32 {
        (self - other).length()
    }

    pub fn dot(self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// 길이가 1인 같은 방향의 벡터입니다. 길이가 0이거나 유한하지 않은 벡터는 `ZERO`로 바꿉니다.
    pub fn normalize_or_zero(self) -> Vec2 {
        let length = self.length();
        if length.is_finite() && length > 0.0 {
            self * (1.0 / length)
        } else {
            Vec2::ZERO
        }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Vec2 {
    type Output = Vec2;

    fn add(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;

    fn sub(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;

    fn mul(self, k: f32) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}

/// 축에 정렬된 사각형입니다. 장애물, 마차, 감시탑, 맵 경계가 모두 이 타입입니다.
/// `min.x <= max.x`이고 `min.y <= max.y`여야 합니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Rect {
        Rect {
            min: Vec2::new(min_x, min_y),
            max: Vec2::new(max_x, max_y),
        }
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    /// 사각형 안에서 `p`와 가장 가까운 점입니다. `p`가 사각형 안에 있으면 `p` 자신입니다.
    pub fn closest_point(self, p: Vec2) -> Vec2 {
        Vec2::new(
            p.x.clamp(self.min.x, self.max.x),
            p.y.clamp(self.min.y, self.max.y),
        )
    }

    /// `p`에서 사각형까지의 거리입니다. `p`가 사각형 안에 있으면 0입니다.
    pub fn distance_to(self, p: Vec2) -> f32 {
        p.distance(self.closest_point(p))
    }

    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// 네 변을 `by`만큼 안쪽으로 옮긴 사각형입니다. 원의 중심이 맵 경계에서 반지름만큼 떨어져 있어야 할 때 씁니다.
    pub fn shrink(self, by: f32) -> Rect {
        Rect::new(
            self.min.x + by,
            self.min.y + by,
            self.max.x - by,
            self.max.y - by,
        )
    }
}
