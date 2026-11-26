//! 개체가 이번 Tick에 쓰는 이동 속도, 공격 피해량, 쿨다운입니다.
//!
//! 플레이어와 몬스터는 각자 `stats` 함수 하나로 이 세 값을 읽고, 규칙의 다른 곳은 수치표를 직접 읽지 않습니다.
//! 수치에 배율을 곱하는 효과는 그 함수에서 곱합니다. 몬스터의 감속이 `Monster::stats`에서 이동 속도에 곱해지고,
//! 플레이어의 버프가 `Player::stats`에서 이동 속도(이동 속도 버프)와 쿨다운(공격 속도 버프)에 곱해집니다.
//! 행동을 막는 효과(스턴)와 피해를 주는 효과(출혈)는 이 값들을 바꾸지 않습니다(`effect.rs`).

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UnitStats {
    /// 초당 타일 수입니다. 쓰러졌거나 상호작용 중이어서 이동할 수 없는 플레이어는 0입니다.
    pub move_speed: f32,
    pub attack_damage: u32,
    pub attack_cooldown_ticks: u32,
}
