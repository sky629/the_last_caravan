//! 지면 좌표를 나타내는 2차원 벡터입니다. 단위는 타일이고, x는 동쪽, y는 북쪽입니다.
//!
//! 외부 수학 크레이트 대신 직접 정의합니다. 규칙 크레이트가 다른 크레이트의 타입을 노출하지 않게 하기 위해서입니다.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Vec2 {
        Vec2 { x, y }
    }

    pub fn distance(self, other: Vec2) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}
