//! 몬스터, 시설, 무기의 종류입니다. 종류마다 수치는 `Tuning`의 종류별 표에서 읽습니다.
//!
//! 종류를 추가하려면 변형과 `ALL`의 항목을 추가하고, 수치표(`MonsterTable`, `StructureTable`, `WeaponTable`)에 필드와 `get`의 분기를 추가합니다.
//! `get`이 `match`이므로, 분기를 빠뜨리면 컴파일러가 알려 줍니다.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MonsterKind {
    /// 마차를 향해 걸어가서 근접 공격을 하는 기본 몬스터입니다.
    Grunt,
    /// 야영지에 서 있는 허수아비입니다. 무기를 시험하는 과녁이고, 움직이지 않고 공격하지 않습니다.
    /// 야영지에서는 아무것도 피해를 받지 않으므로 쓰러지지 않습니다.
    Scarecrow,
    /// 마지막 밤의 끝에 하나만 나타나는 보스입니다. 걷는 몸은 `Grunt`와 같은 크기이고 맞는 몸은 훨씬 큽니다.
    /// 밤의 무리와 낮의 몬스터로는 놓이지 않습니다.
    Boss,
}

impl MonsterKind {
    pub const ALL: [MonsterKind; 3] = [
        MonsterKind::Grunt,
        MonsterKind::Scarecrow,
        MonsterKind::Boss,
    ];
}

/// 플레이어가 짓는 포탑의 종류입니다. 종류마다 하는 일(`TowerAction`)과 수치는 `StructureTable`의 한 행에 있습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StructureKind {
    /// 범위 안의 가장 가까운 몬스터를 쏴서 피해를 줍니다.
    Watchtower,
    /// 범위 안의 모든 몬스터에게 감속을 겁니다. 피해는 없습니다.
    SlowTower,
    /// 범위 안에서 스턴에 걸릴 수 있는 가장 가까운 몬스터 한 마리에게 스턴을 겁니다. 피해는 없습니다.
    StunTower,
    /// 범위 안에 있는 살아 있는 플레이어에게 공격 속도 버프를 유지합니다.
    HasteTower,
    /// 범위 안에 있는 살아 있는 플레이어에게 이동 속도 버프를 유지합니다.
    SwiftTower,
}

impl StructureKind {
    /// 종류를 바꾸는 키(E)가 고르는 순서입니다.
    pub const ALL: [StructureKind; 5] = [
        StructureKind::Watchtower,
        StructureKind::SlowTower,
        StructureKind::StunTower,
        StructureKind::HasteTower,
        StructureKind::SwiftTower,
    ];

    /// 종류를 바꾸는 키(E)를 눌렀을 때 고르는 다음 종류입니다. `ALL`의 순서를 따르고, 마지막 다음은 처음입니다.
    pub fn next(self) -> StructureKind {
        match self {
            StructureKind::Watchtower => StructureKind::SlowTower,
            StructureKind::SlowTower => StructureKind::StunTower,
            StructureKind::StunTower => StructureKind::HasteTower,
            StructureKind::HasteTower => StructureKind::SwiftTower,
            StructureKind::SwiftTower => StructureKind::Watchtower,
        }
    }
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
    /// 쏜 순간 조준선 위의 처음 닿는 몬스터 한 마리를 맞히는 원거리 무기입니다. 날아가는 탄환이 없습니다.
    Rifle,
}

impl WeaponKind {
    pub const ALL: [WeaponKind; 6] = [
        WeaponKind::Sword,
        WeaponKind::Bow,
        WeaponKind::Greatsword,
        WeaponKind::Spear,
        WeaponKind::Crossbow,
        WeaponKind::Rifle,
    ];
}
