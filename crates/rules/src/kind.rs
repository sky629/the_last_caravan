//! 몬스터와 시설의 종류입니다. 종류마다 수치는 `Tuning`의 종류별 표에서 읽습니다.
//!
//! 종류를 추가하려면 변형과 `ALL`의 항목을 추가하고, 수치표(`MonsterTable`, `StructureTable`)에 필드와 `get`의 분기를 추가합니다.
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
