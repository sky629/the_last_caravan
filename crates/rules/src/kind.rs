//! 몬스터, 시설, 무기의 종류입니다. 종류마다 수치는 `Tuning`의 종류별 표에서 읽습니다.
//!
//! 종류를 추가하려면 변형과 `ALL`의 항목을 추가하고, 수치표(`MonsterTable`, `StructureTable`, `WeaponTable`)에 필드와 `get`의 분기를 추가합니다.
//! `get`이 `match`이므로, 분기를 빠뜨리면 컴파일러가 알려 줍니다.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MonsterKind {
    /// 마차를 향해 걸어가서 근접 공격을 하는 기본 몬스터입니다.
    Grunt,
}

impl MonsterKind {
    pub const ALL: [MonsterKind; 1] = [MonsterKind::Grunt];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StructureKind {
    /// 범위 안의 가장 가까운 몬스터를 자동으로 공격하는 시설입니다.
    Watchtower,
}

impl StructureKind {
    pub const ALL: [StructureKind; 1] = [StructureKind::Watchtower];
}

/// 플레이어가 드는 무기의 종류입니다. 종류마다 수치와 공격이 전달되는 방식은 `WeaponTable`에 있습니다.
///
/// 이름은 무기의 이름이고 전달 방식의 이름이 아닙니다. 같은 전달 방식의 무기가 둘 이상 생겨도 종류만 늘고 방식은 그대로입니다.
/// 처음 가지는 무기는 종류가 아니라 수치(`PlayerTuning::start_loadout`)가 정합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WeaponKind {
    /// 조준한 방향의 부채꼴 안에 있는 몬스터를 모두 베는 근접 무기입니다.
    Sword,
    /// 조준한 방향으로 탄환을 쏘는 원거리 무기입니다.
    Bow,
    /// 느리지만 넓은 부채꼴을 크게 베는 근접 무기입니다.
    Greatsword,
    /// 좁은 부채꼴이지만 멀리 닿는 근접 무기입니다.
    Spear,
    /// 한 줄로 늘어선 몬스터를 여러 마리 꿰뚫는 탄환을 쏘는 원거리 무기입니다.
    Crossbow,
}

impl WeaponKind {
    pub const ALL: [WeaponKind; 5] = [
        WeaponKind::Sword,
        WeaponKind::Bow,
        WeaponKind::Greatsword,
        WeaponKind::Spear,
        WeaponKind::Crossbow,
    ];
}
