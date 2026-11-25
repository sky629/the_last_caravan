//! 야영지입니다. 원정 앞에 모이는 곳입니다.
//!
//! 야영지는 원정과 같은 지도입니다. 세계를 야영지의 모습으로 놓는 일은 `World::restart(Layout::Camp)`가 합니다.

#[derive(Clone, Debug, PartialEq)]
pub struct Camp {}

impl Camp {
    pub(crate) fn new() -> Camp {
        Camp {}
    }
}
