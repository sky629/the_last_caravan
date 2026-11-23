//! 마차와 시설입니다. 둘 다 HP를 가진 단단한 사각형입니다.

use crate::ids::Team;
use crate::math::Rect;
use crate::tuning::Tuning;

/// 마차입니다. HP가 0이 되면 원정이 실패합니다 (D43).
/// 같은 Tick에 원정이 끝나므로 「파괴된 상태」는 따로 없습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Wagon {
    pub team: Team,
    pub rect: Rect,
    pub hp: u32,
}

impl Wagon {
    pub(crate) fn new(t: &Tuning) -> Wagon {
        Wagon {
            team: Team::CARAVAN,
            rect: t.map.wagon,
            hp: t.wagon.max_hp,
        }
    }
}
