//! 플레이어가 가진 무기입니다. 무기 칸은 두 개이고, 든 칸은 언제나 차 있습니다.
//!
//! 「칸이 둘 다 빈 상태」와 「든 칸이 빈 상태」는 타입으로 표현할 수 없습니다. 든 무기(`held`)는 `Option`이 아니고, 비어 있을 수 있는 것은 다른 칸(`spare`)뿐입니다.
//! 필드가 비공개이므로 상태를 바꾸는 방법은 이 파일의 `swap`(Q)과 `take`(F로 줍기) 둘뿐입니다.
//!
//! ```text
//!   [든 무기, 빈 칸] --(F: 바닥의 무기)--> [든 무기, 줍은 무기]                    바닥의 무기는 사라짐
//!   [든 무기, 다른 칸] --(F: 바닥의 무기)--> [줍은 무기, 다른 칸]                  이전에 든 무기가 바닥의 그 자리에 놓임
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

    /// 가진 무기를 든 무기부터 차례로 돌려줍니다. 플레이어가 방에서 나갈 때 바닥에 놓을 무기를 고르는 데 씁니다.
    pub fn weapons(&self) -> impl Iterator<Item = WeaponKind> {
        std::iter::once(self.held).chain(self.spare)
    }

    /// Q입니다. 두 칸이 모두 차 있으면 든 칸을 바꿉니다. 한 칸만 차 있으면 아무것도 바꾸지 않습니다.
    pub(crate) fn swap(&mut self) {
        if let Some(other) = self.spare.take() {
            self.spare = Some(std::mem::replace(&mut self.held, other));
        }
    }

    /// F로 바닥의 무기 `found`를 줍습니다. 빈 칸이 있으면 빈 칸에 넣고 `None`을 돌려줍니다.
    /// 빈 칸이 없으면 든 무기와 바꾸고, 이전에 든 무기를 돌려줍니다. 부르는 쪽이 그 무기를 줍는 바닥의 자리에 놓습니다.
    /// 그래서 줍는 일은 무기를 만들거나 없애지 않습니다.
    pub(crate) fn take(&mut self, found: WeaponKind) -> Option<WeaponKind> {
        match self.spare {
            None => {
                self.spare = Some(found);
                None
            }
            Some(_) => Some(std::mem::replace(&mut self.held, found)),
        }
    }
}
