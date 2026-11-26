//! 몬스터의 종류와 걸려 있는 효과를 1바이트에 담는 전송 형식입니다.
//!
//! 몬스터는 수백 마리일 수 있어서 몬스터 하나의 크기가 곧 스냅샷의 크기입니다. 종류는 16가지까지여서 4비트면 되므로, 같은 바이트의 남는 비트에 효과를 싣습니다.
//! 그래서 효과가 늘어도 몬스터 하나는 그대로 9바이트입니다.
//!
//! ```text
//!   비트  7        6        5       4       3 2 1 0
//!         (비움)   출혈     감속    스턴    종류의 순서(0에서 15)
//! ```
//!
//! 담을 수 있는 것은 몬스터 종류 16개와 효과 3개이고, 비트 하나가 남습니다. 종류나 효과가 더 필요하면 이 파일의 배치를 바꾸고 `PROTOCOL_VERSION`을 올립니다.
//! 호출하는 쪽은 비트를 보지 않고 `kind()`와 `effects()`로 읽습니다. 받은 바이트가 이 배치에 맞지 않으면(없는 종류, 비운 비트가 켜져 있음) 해석에 실패합니다.

use caravan_rules::{MonsterEffects, MonsterKind};
use serde::{Deserialize, Serialize};

use crate::snapshot::WireMonsterKind;

/// 종류의 순서를 담는 비트의 수입니다.
const KIND_BITS: u32 = 4;
/// 한 바이트에 담을 수 있는 몬스터 종류의 수입니다. 입장 안내의 상한(`welcome_check`)도 이 값입니다.
pub(crate) const MONSTER_KIND_CAPACITY: usize = 1 << KIND_BITS;
const KIND_MASK: u8 = (1 << KIND_BITS) - 1;
const STUNNED: u8 = 1 << KIND_BITS;
const SLOWED: u8 = STUNNED << 1;
const BLEEDING: u8 = SLOWED << 1;
const UNUSED: u8 = BLEEDING << 1;

// 몬스터 종류가 담을 수 있는 수보다 많아지면 빌드가 깨집니다. 그때는 이 파일의 배치를 바꿉니다.
const _: () = assert!(MonsterKind::ALL.len() <= MONSTER_KIND_CAPACITY);

/// 몬스터에게 걸려 있는 효과 가운데 화면에 보이는 것입니다. 스턴이 끝난 뒤의 면역은 보이지 않으므로 싣지 않습니다.
/// 남은 시간은 싣지 않으므로 화면은 효과가 걸려 있는지만 압니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireMonsterEffects {
    pub stunned: bool,
    pub slowed: bool,
    pub bleeding: bool,
}

impl WireMonsterEffects {
    pub const NONE: WireMonsterEffects = WireMonsterEffects {
        stunned: false,
        slowed: false,
        bleeding: false,
    };

    pub fn of(effects: &MonsterEffects) -> WireMonsterEffects {
        WireMonsterEffects {
            stunned: effects.is_stunned(),
            slowed: effects.is_slowed(),
            bleeding: effects.is_bleeding(),
        }
    }
}

/// 몬스터의 종류와 걸려 있는 효과입니다. 전송할 때 1바이트이고, 이 파일의 맨 위에 적힌 배치를 따릅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub struct WireMonsterStatus {
    kind: WireMonsterKind,
    effects: WireMonsterEffects,
}

impl WireMonsterStatus {
    pub fn new(kind: WireMonsterKind, effects: WireMonsterEffects) -> WireMonsterStatus {
        WireMonsterStatus { kind, effects }
    }

    /// 효과가 하나도 없는 몬스터입니다.
    pub fn plain(kind: WireMonsterKind) -> WireMonsterStatus {
        WireMonsterStatus::new(kind, WireMonsterEffects::NONE)
    }

    pub fn kind(self) -> WireMonsterKind {
        self.kind
    }

    pub fn effects(self) -> WireMonsterEffects {
        self.effects
    }
}

impl From<WireMonsterStatus> for u8 {
    fn from(status: WireMonsterStatus) -> u8 {
        let effects = status.effects;
        let flag = |on: bool, bit: u8| if on { bit } else { 0 };
        status.kind.index()
            | flag(effects.stunned, STUNNED)
            | flag(effects.slowed, SLOWED)
            | flag(effects.bleeding, BLEEDING)
    }
}

impl TryFrom<u8> for WireMonsterStatus {
    type Error = &'static str;

    fn try_from(byte: u8) -> Result<WireMonsterStatus, &'static str> {
        if byte & UNUSED != 0 {
            return Err("a monster status with the unused bit set");
        }
        let kind = WireMonsterKind::from_index(byte & KIND_MASK)
            .ok_or("a monster kind that does not exist")?;
        Ok(WireMonsterStatus {
            kind,
            effects: WireMonsterEffects {
                stunned: byte & STUNNED != 0,
                slowed: byte & SLOWED != 0,
                bleeding: byte & BLEEDING != 0,
            },
        })
    }
}
