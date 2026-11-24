//! 달리기 게이지입니다. 달리면 Tick마다 줄고, 달리기 키를 떼고 있으면 Tick마다 찹니다.
//!
//! 게이지는 정수 점수로 셉니다. 가득 찬 값은 「가득 찬 게이지로 달릴 수 있는 Tick 수 곱하기 빈 게이지가 가득 차는 데 걸리는 Tick 수」이고,
//! 달리는 Tick마다 「채우는 Tick 수」만큼 줄고 쉬는 Tick마다 「달릴 수 있는 Tick 수」만큼 찹니다.
//! 그래서 가득 찬 게이지는 정확히 달릴 수 있는 시간에 비고, 빈 게이지는 정확히 채우는 시간에 차며, 소수의 오차가 쌓이지 않습니다.
//! 기본 수치(4초, 10초, 20 Hz)에서는 가득 찬 값이 80 곱하기 200인 16000이고, 달리면 Tick마다 200이 줄고 쉬면 Tick마다 80이 찹니다.

use crate::tuning::Tuning;

/// 살아 있는 플레이어의 달리기 게이지입니다. 쓰러졌거나 사망한 플레이어는 게이지가 없으므로 `Life::Alive`만 가집니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamina(u32);

impl Stamina {
    /// 가득 찬 게이지입니다. 플레이어가 입장할 때, 새 원정과 새 날이 시작될 때, 부활한 직후에 이 값이 됩니다.
    pub(crate) fn full(t: &Tuning) -> Stamina {
        Stamina(full_points(t))
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// 달리는 Tick 하나만큼 줄입니다. 남은 점수가 모자라면 0이 됩니다.
    pub(crate) fn spend(&mut self, t: &Tuning) {
        self.0 = self.0.saturating_sub(refill_ticks(t));
    }

    /// 쉬는 Tick 하나만큼 채웁니다. 가득 찬 값을 넘지 않습니다.
    pub(crate) fn refill(&mut self, t: &Tuning) {
        self.0 = self.0.saturating_add(sprint_ticks(t)).min(full_points(t));
    }

    /// 가득 찬 값에 대한 백분율입니다. 가득 차면 100이고 비면 0입니다.
    pub fn percent(self, t: &Tuning) -> u8 {
        let percent = u64::from(self.0) * 100 / u64::from(full_points(t));
        percent.min(100) as u8
    }
}

/// 가득 찬 게이지로 달릴 수 있는 Tick 수입니다.
fn sprint_ticks(t: &Tuning) -> u32 {
    t.ticks(t.sprint.sprint_secs)
}

/// 빈 게이지가 가득 차는 데 걸리는 Tick 수입니다.
fn refill_ticks(t: &Tuning) -> u32 {
    t.ticks(t.sprint.refill_secs)
}

/// 두 Tick 수의 곱이 32비트 정수에 들어가는지는 `Tuning::validate`가 검사합니다.
fn full_points(t: &Tuning) -> u32 {
    sprint_ticks(t) * refill_ticks(t)
}
