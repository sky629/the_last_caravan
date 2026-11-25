//! 플레이어가 한 Tick 동안 요청하는 조작입니다. 의도만 담고, 위치나 피해량은 담지 않습니다.
//!
//! 클라이언트가 보낸 값은 믿을 수 없으므로 `MoveDir::new`와 `AimDir::new`가 값을 검사하고 고칩니다.
//! 이 두 생성자를 거친 값만 규칙에 들어오므로, 규칙은 입력을 다시 검사하지 않습니다.

use std::collections::BTreeMap;

use crate::ids::PlayerId;
use crate::math::Vec2;

/// 플레이어마다 한 Tick 분량의 입력입니다. 목록에 없는 플레이어는 아무것도 누르지 않은 것으로 봅니다.
/// 번호 순서로 순회하므로 같은 입력은 항상 같은 순서로 처리됩니다.
pub type PlayerInputs = BTreeMap<PlayerId, PlayerInput>;

/// 한 플레이어의 한 Tick 분량 조작입니다.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerInput {
    /// 클라이언트가 붙인 순서 번호입니다. 규칙은 이 입력을 처리한 Tick에 `Player::last_input_seq`에 복사합니다.
    /// 클라이언트 예측을 추가하면 서버가 어디까지 처리했는지 알려 주는 용도로 씁니다.
    pub seq: u32,
    pub move_dir: MoveDir,
    pub aim: AimDir,
    /// 공격 버튼을 누르고 있습니다.
    pub attack: bool,
    /// 상호작용 키(F)를 누르고 있습니다.
    pub interact: bool,
    /// 상호작용 키(F)를 누른 순간입니다. 누르고 있는 상태가 아니라 이번 Tick에 한 번 눌렀다는 뜻이고, 바닥의 무기를 줍는 데 쓰입니다.
    /// 입력이 늦는 Tick에 이전 입력을 다시 쓸 때는 `held`가 이 값을 지웁니다.
    pub interact_pressed: bool,
    /// 건설 키(B)를 누르고 있습니다. 어느 타일에 세울지는 서버가 플레이어의 위치와 조준 방향에서 정합니다.
    pub build: bool,
    /// 달리기 키(Shift)를 누르고 있습니다. 움직이고 있고 게이지가 남아 있어야 달리고, 키를 누르고 있는 동안에는 게이지가 차지 않습니다.
    pub sprint: bool,
    /// 무기 전환 키(Q)를 누른 순간입니다. 누르고 있는 상태가 아니라 이번 Tick에 한 번 눌렀다는 뜻이고,
    /// 입력이 늦는 Tick에 이전 입력을 다시 쓸 때는 `held`가 이 값을 지웁니다.
    pub switch_weapon: bool,
    /// 준비 키(R)를 누른 순간입니다. 야영지에서 준비를 켜고 끕니다. 다른 단계에서는 아무 일도 하지 않습니다.
    /// 입력이 늦는 Tick에 이전 입력을 다시 쓸 때는 `held`가 이 값을 지웁니다.
    pub toggle_ready: bool,
    /// 확인 키(Enter)를 누른 순간입니다. 결과 화면에서 확인을 켭니다. 한 번 켠 확인은 끌 수 없습니다. 다른 단계에서는 아무 일도 하지 않습니다.
    /// 입력이 늦는 Tick에 이전 입력을 다시 쓸 때는 `held`가 이 값을 지웁니다.
    pub confirm: bool,
}

impl PlayerInput {
    /// 입력이 늦는 Tick에 다시 쓰는 사본입니다. 누르고 있는 상태는 그대로 두고, 누른 순간인 조작(`switch_weapon`, `interact_pressed`, `toggle_ready`, `confirm`)은 지웁니다.
    /// 그래서 한 번 누른 키가 입력이 늦는 Tick마다 되풀이되지 않습니다. 누른 순간인 조작을 더하면 이 함수가 그 값도 지웁니다.
    pub fn held(self) -> PlayerInput {
        PlayerInput {
            switch_weapon: false,
            interact_pressed: false,
            toggle_ready: false,
            confirm: false,
            ..self
        }
    }
}

/// 이동 의도입니다. 길이는 항상 0 이상 1 이하이므로, 어떤 입력도 최고 속도보다 빠르게 움직일 수 없습니다.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveDir(Vec2);

impl MoveDir {
    /// 유한하지 않은 값(NaN, 무한대)은 정지로 바꾸고, 길이가 1보다 큰 값은 길이 1로 줄입니다.
    pub fn new(x: f32, y: f32) -> MoveDir {
        let raw = Vec2::new(x, y);
        if !raw.is_finite() {
            return MoveDir(Vec2::ZERO);
        }
        if raw.length() > 1.0 {
            MoveDir(raw.normalize_or_zero())
        } else {
            MoveDir(raw)
        }
    }

    pub fn get(self) -> Vec2 {
        self.0
    }
}

/// 조준 방향입니다. 길이 1인 방향이거나 「방향 없음」(지금 보는 방향을 유지)입니다.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AimDir(Option<Vec2>);

impl AimDir {
    /// 길이를 1로 맞춥니다. 길이가 0이거나 유한하지 않은 값은 「방향 없음」입니다.
    pub fn new(x: f32, y: f32) -> AimDir {
        let unit = Vec2::new(x, y).normalize_or_zero();
        if unit == Vec2::ZERO {
            AimDir(None)
        } else {
            AimDir(Some(unit))
        }
    }

    pub fn get(self) -> Option<Vec2> {
        self.0
    }
}
