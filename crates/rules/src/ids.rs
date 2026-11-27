//! 개체를 구분하는 번호와 개체가 속한 진영입니다.
//!
//! 번호는 종류마다 별도의 타입으로 감쌉니다. 몬스터 번호를 플레이어 번호 자리에 넣는 실수를 컴파일러가 막습니다.
//! `Sim`은 번호를 하나씩 늘려서 나누어 주고 다시 사용하지 않습니다.
//! 그래서 이미 사라진 개체를 가리키는 번호(부활 대상, 포탑의 사격 대상)가 새 개체를 가리킬 수 없습니다.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonsterId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TowerId(pub u32);

/// 채집 지점의 번호입니다. 맵에 적힌 순서대로 1부터 붙고 원정 동안 바뀌지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

/// 날아가는 탄환 하나의 번호입니다. 번호는 `World::launch`가 1씩 늘려서 나누어 주고 다시 사용하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectileId(pub u32);

/// 땅에 떨어진 자원 한 더미의 번호입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PileId(pub u32);

/// 무기 상자의 번호입니다. 상자가 놓일 때마다 1씩 늘려서 나누어 주고 다시 사용하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChestId(pub u32);

/// 바닥에 놓인 무기 하나의 번호입니다. 무기가 바닥에 놓일 때마다 1씩 늘려서 나누어 주고 다시 사용하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroundWeaponId(pub u32);

/// 개체가 편들어 싸우는 쪽입니다.
///
/// 「누가 누구를 공격할 수 있는가」와 「누가 누구를 도울 수 있는가」는 모두 `is_hostile_to`와 `is_allied_with`로 판정합니다.
/// 규칙은 「플레이어인가 몬스터인가」를 직접 묻지 않습니다.
/// 따라서 팀 대전을 추가할 때는 새 플레이어에게 줄 진영(`Sim::team_for_new_player`)만 바꾸면 됩니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Team(pub u8);

impl Team {
    /// 협동 모드의 플레이어, 마차, 포탑이 속한 진영입니다.
    pub const CARAVAN: Team = Team(0);
    /// 몬스터가 속한 진영입니다.
    pub const HORDE: Team = Team(1);

    pub fn is_hostile_to(self, other: Team) -> bool {
        self != other
    }

    pub fn is_allied_with(self, other: Team) -> bool {
        self == other
    }
}
