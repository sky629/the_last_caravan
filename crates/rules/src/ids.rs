//! 개체를 구분하는 번호입니다. 서로 다른 종류의 번호를 섞어 쓰지 않도록 각각 별도의 타입으로 감쌉니다.

/// 방 안에서 플레이어 한 명을 가리킵니다. 한 방 안에서는 다시 사용하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(pub u32);
