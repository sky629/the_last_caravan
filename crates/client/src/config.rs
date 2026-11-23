//! 화면 표현에 쓰는 수치입니다. 게임 규칙의 수치는 서버가 입장할 때 알려 주므로 여기에는 모양, 크기, 색만 있습니다 (D15).
//! 길이의 단위는 화면 픽셀이고, 지면의 길이(타일)에서 정해지는 크기는 `Welcome`의 값에서 계산합니다.

use bevy::prelude::*;

// 깊이. 지면 타일은 맨 뒤에 있고, 그 위의 모든 것은 발 위치로 순서를 정합니다 (C12).
pub const GROUND_Z: f32 = 0.0;
/// 순서를 가지는 개체의 z는 `SORTED_Z_BASE`에서 발 위치의 `x + y`를 뺀 값입니다.
/// 카메라가 보는 깊이는 -1000에서 1000이므로, `x + y`가 500보다 큰 맵에서는 개체가 지면 아래로 내려갑니다.
pub const SORTED_Z_BASE: f32 = 500.0;
/// 한 개체 안에서 부품이 겹칠 때의 z 간격입니다. 발 위치가 한 칸의 1/128만큼 달라도 개체 사이의 순서는 바뀌지 않는 크기입니다.
pub const PART_Z_STEP: f32 = 0.001;

// 상자의 높이입니다. 타일 하나 위에 서 있습니다.
pub const OBSTACLE_HEIGHT: f32 = 28.0;
pub const WAGON_HEIGHT: f32 = 20.0;
pub const TOWER_HEIGHT: f32 = 64.0;
/// 파괴된 감시탑이 남기는 잔해입니다.
pub const RUBBLE_HEIGHT: f32 = 6.0;

// 캐릭터의 몸통입니다. 발 위치에서 위로 서 있는 직사각형이고, 그림자는 규칙이 쓰는 충돌 원을 지면에 그린 타원입니다.
pub const PLAYER_BODY: Vec2 = Vec2::new(14.0, 30.0);
pub const MONSTER_BODY: Vec2 = Vec2::new(12.0, 20.0);
/// 쓰러진 플레이어는 누워 있는 것처럼 낮게 그립니다.
pub const DOWNED_BODY_HEIGHT: f32 = 10.0;
pub const DEAD_BODY_HEIGHT: f32 = 5.0;
/// 바라보는 방향을 알려 주는 작은 원의 반지름입니다.
pub const NOSE_RADIUS: f32 = 3.0;

// HP 막대입니다. 몸통 위에 떠 있습니다.
pub const BAR_SIZE: Vec2 = Vec2::new(28.0, 5.0);
pub const BAR_GAP: f32 = 7.0;
/// 막대의 바깥 테두리 두께입니다.
pub const BAR_BORDER: f32 = 1.0;

// 효과. 시간은 서버 Tick으로 셉니다.
pub const SWING_TICKS: f64 = 4.0;
pub const SHOT_TICKS: f64 = 3.0;
/// 근접 공격 표시의 반지름(타일)과 부채꼴의 절반 각도(라디안)입니다. 서버의 공격 범위를 흉내 낸 표현이고, 판정은 서버가 합니다.
pub const SWING_REACH_TILES: f32 = 1.35;
pub const SWING_HALF_ANGLE: f32 = std::f32::consts::FRAC_PI_3;
pub const SWING_ARC_STEPS: usize = 10;

pub const BACKGROUND: Color = Color::srgb(0.07, 0.06, 0.06);
pub const GROUND_A: Color = Color::srgb(0.20, 0.27, 0.18);
pub const GROUND_B: Color = Color::srgb(0.23, 0.31, 0.21);
pub const OBSTACLE: Color = Color::srgb(0.58, 0.58, 0.62);
pub const WAGON: Color = Color::srgb(0.68, 0.44, 0.22);
pub const TOWER: Color = Color::srgb(0.38, 0.58, 0.90);
pub const RUBBLE: Color = Color::srgb(0.30, 0.27, 0.25);
pub const SHADOW: Color = Color::srgb(0.07, 0.09, 0.07);
pub const YOU: Color = Color::srgb(1.00, 0.84, 0.25);
pub const ALLY: Color = Color::srgb(0.35, 0.65, 1.00);
pub const DOWNED: Color = Color::srgb(0.95, 0.45, 0.15);
pub const DEAD: Color = Color::srgb(0.40, 0.40, 0.42);
pub const MONSTER: Color = Color::srgb(0.85, 0.22, 0.28);
pub const NOSE_PLAYER: Color = Color::srgb(1.00, 1.00, 1.00);
pub const NOSE_MONSTER: Color = Color::srgb(1.00, 0.80, 0.30);
pub const BAR_BACK: Color = Color::srgb(0.06, 0.06, 0.06);
pub const BAR_PLAYER: Color = Color::srgb(0.30, 0.85, 0.35);
pub const BAR_MONSTER: Color = Color::srgb(0.95, 0.35, 0.30);
pub const BAR_TOWER: Color = Color::srgb(0.45, 0.70, 1.00);
pub const SWING: Color = Color::srgb(1.00, 1.00, 0.80);
pub const SHOT: Color = Color::srgb(1.00, 0.90, 0.30);
/// 효과 선의 굵기(픽셀)입니다.
pub const EFFECT_LINE_WIDTH: f32 = 3.0;
