//! 키보드와 마우스를 읽습니다. WASD로 이동하고, 마우스로 조준하고, 왼쪽 버튼으로 공격하고, F를 누르고 있으면 부활이나 수리를 진행하고, B를 누르고 있으면 바라보는 쪽에 감시탑을 짓습니다.
//! 읽은 값을 입력 메시지로 바꾸고 보내는 일은 `caravan_client_core`와 `net`이 합니다.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use caravan_client_core::Controls;

use crate::convert;

/// 이번 프레임에 읽은 조작입니다.
#[derive(Resource, Default)]
pub struct HeldControls(pub Controls);

/// 키보드와 마우스의 상태를 읽습니다.
pub fn read_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut held: ResMut<HeldControls>,
) {
    let (camera, camera_transform) = *camera;
    // 커서가 가리키는 화면 위치를 지면 위의 위치로 되돌립니다. 지면은 캐릭터의 발이 닿는 평면입니다.
    let aim_at = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        .map(convert::ground_pos);
    held.0 = Controls {
        up: keys.pressed(KeyCode::KeyW),
        down: keys.pressed(KeyCode::KeyS),
        left: keys.pressed(KeyCode::KeyA),
        right: keys.pressed(KeyCode::KeyD),
        aim_at,
        // 눌렀다가 같은 프레임 안에 뗀 클릭도 공격으로 셉니다.
        attack: mouse.pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Left),
        interact: keys.pressed(KeyCode::KeyF),
        build: keys.pressed(KeyCode::KeyB),
    };
}
