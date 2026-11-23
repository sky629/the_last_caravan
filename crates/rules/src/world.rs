//! 한 원정 동안 변하는 상태(`World`), 변하지 않는 맵 정보(`Statics`), 한 Tick 동안 규칙이 읽는 값(`Ctx`)입니다.
//!
//! Tick을 처리하는 함수는 모두 같은 순서로 일합니다.
//! 1. `World`를 읽기만 하면서 무엇을 할지 결정해서 `Vec`에 모읍니다.
//! 2. 결정을 `&mut World`로 적용합니다.
//!
//! 이렇게 하면 한 맵을 읽으면서 같은 맵을 고치려는 대여 충돌이 생기지 않고,
//! 개체를 어떤 순서로 방문해도 결과가 같습니다.

use std::collections::BTreeMap;

use crate::ids::PlayerId;
use crate::input::PlayerInputs;
use crate::math::Rect;
use crate::player::Player;
use crate::tuning::Tuning;

/// 원정 동안 변하는 상태 전체입니다. `Sim`이 하나만 가집니다.
/// 순회 순서가 번호 순서로 정해지도록 `HashMap` 대신 `BTreeMap`을 씁니다. 그래야 같은 입력이 항상 같은 결과를 냅니다.
pub(crate) struct World {
    pub players: BTreeMap<PlayerId, Player>,
}

/// `Sim::new`에서 한 번 만들고 그 뒤로 바뀌지 않는 맵 정보입니다.
pub(crate) struct Statics {
    pub bounds: Rect,
    /// 장애물, 마차, 시설입니다. 파괴된 시설도 잔해로 남아서 길을 막습니다.
    pub solids: Vec<Rect>,
}

/// 한 Tick 동안 규칙이 읽기만 하는 값을 묶은 것입니다.
pub(crate) struct Ctx<'a> {
    pub t: &'a Tuning,
    pub statics: &'a Statics,
    pub inputs: &'a PlayerInputs,
}
