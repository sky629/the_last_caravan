//! Bevy의 `Vec2`와 게임 규칙의 `Vec2`는 서로 다른 크레이트의 타입입니다. 둘 사이를 바꾸는 곳은 이 파일 하나입니다.

use bevy::prelude::*;
use caravan_client_core as core;
use caravan_rules as rules;

use crate::config;

/// 지면 위치(타일)가 화면에서 놓이는 곳(픽셀)입니다.
pub fn screen_pos(ground: rules::Vec2) -> Vec2 {
    from_rules(core::ground_to_screen(ground))
}

/// 화면의 한 점(픽셀) 아래에 있는 지면 위치(타일)입니다.
pub fn ground_pos(screen: Vec2) -> rules::Vec2 {
    core::screen_to_ground(to_rules(screen))
}

/// 발이 지면 위치 `foot`에 있는 개체를 놓을 곳입니다. z는 발이 화면에서 아래쪽일수록 큽니다.
pub fn placement(foot: rules::Vec2) -> Vec3 {
    screen_pos(foot).extend(config::SORTED_Z_BASE + core::depth(foot))
}

/// 화면 좌표를 `caravan_client_core`의 계산이 받는 벡터로 바꿉니다.
pub fn to_rules(v: Vec2) -> rules::Vec2 {
    rules::Vec2::new(v.x, v.y)
}

/// `caravan_client_core`의 계산이 돌려준 벡터를 Bevy의 벡터로 바꿉니다.
pub fn from_rules(v: rules::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}
