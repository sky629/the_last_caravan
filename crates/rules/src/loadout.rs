//! 플레이어가 가진 무기입니다. 무기 칸은 두 개이고, 든 칸은 언제나 차 있습니다.
//!
//! 「칸이 둘 다 빈 상태」와 「든 칸이 빈 상태」는 타입으로 표현할 수 없습니다. 든 무기(`held`)는 `Option`이 아니고, 비어 있을 수 있는 것은 다른 칸(`spare`)뿐입니다.
//! 필드가 비공개이므로 상태를 바꾸는 방법은 이 파일의 함수뿐입니다.
//!
//! ```text
//!   [든 무기, 다른 칸] --(Q)--> [다른 칸이던 무기, 이전에 든 무기]
//!   [든 무기, 빈 칸] --(Q)--> 그대로
//! ```

use crate::kind::WeaponKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loadout {
    held: WeaponKind,
    spare: Option<WeaponKind>,
}

impl Loadout {
    pub const fn new(held: WeaponKind, spare: Option<WeaponKind>) -> Loadout {
        Loadout { held, spare }
    }

    /// 지금 드는 무기입니다. 공격은 이 무기로 합니다.
    pub fn held(&self) -> WeaponKind {
        self.held
    }

    /// 들지 않은 칸의 무기입니다. 빈 칸이면 `None`입니다.
    pub fn spare(&self) -> Option<WeaponKind> {
        self.spare
    }

    /// Q입니다. 두 칸이 모두 차 있으면 든 칸을 바꾸고 `true`를 돌려줍니다. 한 칸만 차 있으면 아무것도 바꾸지 않고 `false`입니다.
    pub(crate) fn swap(&mut self) -> bool {
        match self.spare.take() {
            Some(other) => {
                self.spare = Some(std::mem::replace(&mut self.held, other));
                true
            }
            None => false,
        }
    }
}
