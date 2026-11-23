//! 개체가 이번 Tick에 쓰는 이동 속도, 공격 피해량, 쿨다운입니다.
//!
//! 플레이어와 몬스터는 각자 `stats` 함수 하나로 이 세 값을 읽고, 규칙의 다른 곳은 수치표를 직접 읽지 않습니다.
//! 상태 효과(불타는 상태, 느려짐 등)를 추가하면 그 함수에서 효과의 배율을 곱하면 됩니다.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UnitStats {
    /// 초당 타일 수입니다. 이동할 수 없는 상태는 0입니다.
    pub move_speed: f32,
    pub attack_damage: u32,
    pub attack_cooldown_ticks: u32,
}
