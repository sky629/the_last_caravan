//! 화면 표현에 쓰는 수치입니다. 게임 규칙의 수치는 서버가 입장할 때 알려 주므로 여기에는 모양, 크기, 색만 있습니다.
//! 길이의 단위는 화면 픽셀이고, 지면의 길이(타일)에서 정해지는 크기는 `Welcome`의 값에서 계산합니다.

use bevy::prelude::*;

// 카메라. 세로로 보이는 범위(월드 픽셀)입니다. 가로는 창의 비율에 따라 달라집니다.
pub const VIEW_HEIGHT_PX: f32 = 800.0;

// 마차 표시. 화면 가장자리에서 이만큼(월드 픽셀) 안쪽에 놓입니다.
pub const WAGON_MARKER_INSET_PX: f32 = 90.0;
pub const WAGON_MARKER_SIZE: f32 = 20.0;
pub const WAGON_MARKER_WIDTH: f32 = 140.0;

// 깊이. 지면 타일은 맨 뒤에 있고, 그 위의 모든 것은 발 위치로 순서를 정합니다.
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
pub const NODE_HEIGHT: f32 = 22.0;
/// 무기 상자입니다. 채집 지점보다 낮고 작은 상자처럼 보입니다.
pub const CHEST_HEIGHT: f32 = 16.0;
/// 짓는 중인 감시탑(공사터)입니다.
pub const SITE_HEIGHT: f32 = 24.0;
/// 야영지의 무기 진열대입니다. 무기 상자보다 높아서 서 있는 선반처럼 보입니다.
pub const RACK_HEIGHT: f32 = 26.0;

// 캐릭터의 몸통입니다. 발 위치에서 위로 서 있는 직사각형이고, 그림자는 규칙이 쓰는 충돌 원을 지면에 그린 타원입니다.
pub const PLAYER_BODY: Vec2 = Vec2::new(14.0, 30.0);
pub const MONSTER_BODY: Vec2 = Vec2::new(12.0, 20.0);
/// 야영지의 허수아비입니다. 몬스터보다 키가 크고 막대기에 걸린 모습이라 가늘어서, 몬스터와 한눈에 구분됩니다.
pub const SCARECROW_BODY: Vec2 = Vec2::new(10.0, 34.0);
/// 쓰러진 플레이어는 누워 있는 것처럼 낮게 그립니다.
pub const DOWNED_BODY_HEIGHT: f32 = 10.0;
pub const DEAD_BODY_HEIGHT: f32 = 5.0;
/// 바라보는 방향을 알려 주는 작은 원의 반지름입니다.
pub const NOSE_RADIUS: f32 = 3.0;
/// 땅에 떨어진 자원을 나타내는 납작한 원의 가로와 세로 반지름입니다.
pub const PILE_RADII: Vec2 = Vec2::new(9.0, 4.5);
/// 바닥에 놓인 무기를 나타내는 납작한 원의 가로와 세로 반지름입니다. 자원 더미보다 크게 그려서 멀리서도 구분됩니다.
pub const GROUND_WEAPON_RADII: Vec2 = Vec2::new(13.0, 6.5);
/// 진열대 위에 진열된 무기의 이름이 떠 있는 높이(픽셀)입니다. 글자 크기와 색은 바닥의 무기와 같습니다.
pub const RACK_LABEL_LIFT: f32 = RACK_HEIGHT + 14.0;
/// 진열대는 1.2타일 간격이라 이름이 서로 닿으므로, 번호가 홀수인 진열대는 이름을 위가 아니라 아래(진열대의 바닥 중심에서 이만큼 아래)에 둡니다.
pub const RACK_LABEL_BELOW: f32 = -14.0;
/// 진열대의 이름을 진열대의 발 위치보다 앞에 그리려고 더하는 z입니다. 이웃한 진열대(1.2타일 앞)의 블록이 이름을 가리지 않습니다.
pub const RACK_LABEL_Z_LIFT: f32 = 2.0;
/// 허수아비가 맞은 피해의 숫자입니다. 맞은 Tick부터 이 시간 동안 허수아비 위로 떠오르며 사라집니다.
pub const STRIKE_TICKS: f64 = 24.0;
/// 숫자가 떠오르는 높이(픽셀)와 글자 크기, 처음의 색입니다. 숫자가 나타나는 처음 높이는 허수아비의 키보다 조금 위입니다.
pub const STRIKE_RISE: f32 = 28.0;
pub const STRIKE_SIZE: f32 = 22.0;
/// 숫자를 허수아비의 발 위치보다 앞에 그리려고 더하는 z입니다. 발 위치가 타일 둘 앞에 있는 개체까지 숫자 뒤로 갑니다.
pub const STRIKE_Z_LIFT: f32 = 2.0;
pub const STRIKE: Color = Color::srgb(1.00, 0.92, 0.45);
/// 바닥에 놓인 무기의 이름이 원 위에 떠 있는 높이(픽셀)와 글자 크기입니다.
pub const GROUND_WEAPON_LABEL_LIFT: f32 = 16.0;
pub const GROUND_WEAPON_LABEL_SIZE: f32 = 13.0;
pub const GROUND_WEAPON_LABEL: Color = Color::srgb(0.95, 0.95, 0.95);

// HP 막대입니다. 몸통 위에 떠 있습니다.
pub const BAR_SIZE: Vec2 = Vec2::new(28.0, 5.0);
pub const BAR_GAP: f32 = 7.0;
/// 막대의 바깥 테두리 두께입니다.
pub const BAR_BORDER: f32 = 1.0;

// 효과. 시간은 서버 Tick으로 셉니다.
pub const SWING_TICKS: f64 = 4.0;
pub const SHOT_TICKS: f64 = 3.0;
/// 소총의 사격 선이 보이는 시간입니다. 쿨다운(1.5초)이 길어서 선이 사라진 뒤에 다음 사격이 오므로, 부채꼴보다 오래 남겨서 쏜 자리를 알아볼 수 있게 합니다.
pub const TRACE_TICKS: f64 = 6.0;
pub const SWING_ARC_STEPS: usize = 10;
/// 탄환의 선에서 꼬리의 길이(타일)입니다. 머리는 탄환의 위치이고, 꼬리는 날아온 쪽으로 이만큼 늘어납니다.
pub const BULLET_TAIL_TILES: f32 = 0.6;

pub const BACKGROUND: Color = Color::srgb(0.07, 0.06, 0.06);
pub const GROUND_A: Color = Color::srgb(0.20, 0.27, 0.18);
pub const GROUND_B: Color = Color::srgb(0.23, 0.31, 0.21);
pub const OBSTACLE: Color = Color::srgb(0.58, 0.58, 0.62);
pub const WAGON: Color = Color::srgb(0.68, 0.44, 0.22);
pub const TOWER: Color = Color::srgb(0.38, 0.58, 0.90);
pub const SITE: Color = Color::srgb(0.62, 0.55, 0.38);
/// 채집 지점입니다. 자원이 다 떨어지면 어두운 색이 됩니다.
pub const NODE: Color = Color::srgb(0.25, 0.72, 0.66);
pub const NODE_EMPTY: Color = Color::srgb(0.30, 0.34, 0.34);
/// 무기 상자입니다. 채집 지점(청록)과 감시탑(파랑)과 구분되는 황금색입니다.
pub const CHEST: Color = Color::srgb(0.86, 0.64, 0.16);
/// 땅에 떨어진 자원입니다.
pub const PILE: Color = Color::srgb(1.00, 0.85, 0.30);
/// 바닥에 놓인 무기의 색입니다. 무기마다 달라서, 이름을 읽기 전에도 어떤 무기인지 알 수 있습니다.
pub const WEAPON_SWORD: Color = Color::srgb(0.85, 0.88, 0.92);
pub const WEAPON_BOW: Color = Color::srgb(0.55, 0.85, 0.40);
pub const WEAPON_GREATSWORD: Color = Color::srgb(0.95, 0.45, 0.30);
pub const WEAPON_SPEAR: Color = Color::srgb(0.95, 0.80, 0.35);
pub const WEAPON_CROSSBOW: Color = Color::srgb(0.30, 0.78, 0.85);
pub const WEAPON_RIFLE: Color = Color::srgb(0.88, 0.45, 0.82);
pub const SHADOW: Color = Color::srgb(0.07, 0.09, 0.07);
pub const YOU: Color = Color::srgb(1.00, 0.84, 0.25);
pub const ALLY: Color = Color::srgb(0.35, 0.65, 1.00);
/// 다른 편 플레이어의 몸통 색입니다. 같은 편(파랑)과 몬스터(빨강) 어느 쪽과도 구분됩니다.
pub const HOSTILE_PLAYER: Color = Color::srgb(0.72, 0.38, 0.92);
pub const DOWNED: Color = Color::srgb(0.95, 0.45, 0.15);
pub const DEAD: Color = Color::srgb(0.40, 0.40, 0.42);
pub const MONSTER: Color = Color::srgb(0.85, 0.22, 0.28);
/// 허수아비는 몬스터(빨강)와 달리 밀짚색이어서, 때려도 되는 과녁임을 알 수 있습니다.
pub const SCARECROW: Color = Color::srgb(0.82, 0.70, 0.40);
pub const NOSE_PLAYER: Color = Color::srgb(1.00, 1.00, 1.00);
pub const NOSE_MONSTER: Color = Color::srgb(1.00, 0.80, 0.30);
pub const BAR_BACK: Color = Color::srgb(0.06, 0.06, 0.06);
pub const BAR_PLAYER: Color = Color::srgb(0.30, 0.85, 0.35);
pub const BAR_MONSTER: Color = Color::srgb(0.95, 0.35, 0.30);
pub const BAR_TOWER: Color = Color::srgb(0.45, 0.70, 1.00);
pub const BAR_SITE: Color = Color::srgb(0.95, 0.80, 0.25);
pub const BAR_NODE: Color = Color::srgb(0.45, 0.90, 0.82);
pub const SWING: Color = Color::srgb(1.00, 1.00, 0.80);
pub const WAGON_MARKER: Color = Color::srgb(1.00, 0.72, 0.38);
pub const SHOT: Color = Color::srgb(1.00, 0.90, 0.30);
/// 활과 석궁의 탄환입니다.
pub const BULLET: Color = Color::srgb(1.00, 0.97, 0.75);
/// 소총의 사격 선을 굵게 보이게 하려고 겹쳐 그리는 선 둘이 중심선에서 양옆으로(선의 방향에 수직으로) 벗어난 픽셀 수입니다.
pub const TRACE_SPREAD_PX: f32 = 2.0;
/// 소총의 사격 선입니다. 활의 탄환(연한 노랑)과 구분되는 밝은 흰색입니다.
pub const TRACE: Color = Color::srgb(1.00, 1.00, 1.00);
/// 건설이 거절된 타일의 테두리입니다.
pub const REFUSAL: Color = Color::srgb(1.00, 0.35, 0.30);
/// 건설 키를 누르면 감시탑이 놓일 타일의 테두리입니다.
pub const PREVIEW: Color = Color::srgb(0.80, 0.95, 1.00);
/// 효과 선의 굵기(픽셀)입니다.
pub const EFFECT_LINE_WIDTH: f32 = 3.0;
