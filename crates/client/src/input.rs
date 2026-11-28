//! 키보드와 마우스를 읽습니다. WASD로 이동하고, Shift를 누르고 있으면 달리고, 마우스로 조준하고, 왼쪽 버튼으로 들고 있는 무기로 공격하고, Q를 누르면 무기를 바꾸고, G를 누르면 든 무기를 버리고, F를 누르고 있으면 부활, 수리, 채집을 진행하고 F를 누른 순간에는 바닥이나 진열대의 무기를 줍고, B를 누르고 있으면 바라보는 쪽에 고른 종류의 포탑을 짓고, E를 누르면 지을 포탑의 종류를 바꾸고, R을 누르면 야영지에서 준비를 켜고 끄고, Enter를 누르면 결과 화면에서 확인합니다.
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
        // 누르기 시작한 프레임만 참입니다. 키를 누르고 있어도 바닥의 무기를 계속 줍지 않습니다.
        interact_pressed: keys.just_pressed(KeyCode::KeyF),
        build: keys.pressed(KeyCode::KeyB),
        sprint: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        // 누르기 시작한 프레임만 참입니다. 키를 누르고 있어도 무기가 계속 바뀌지 않습니다.
        switch_weapon: keys.just_pressed(KeyCode::KeyQ),
        // 누르기 시작한 프레임만 참입니다. 키를 누르고 있어도 준비가 계속 바뀌지 않습니다.
        toggle_ready: keys.just_pressed(KeyCode::KeyR),
        confirm: keys.just_pressed(KeyCode::Enter),
        // 누르기 시작한 프레임만 참입니다. 키를 누르고 있어도 포탑의 종류가 계속 바뀌지 않습니다.
        cycle_tower: keys.just_pressed(KeyCode::KeyE),
        // 누르기 시작한 프레임만 참입니다. 키를 누르고 있어도 무기를 계속 버리지 않습니다.
        drop_weapon: keys.just_pressed(KeyCode::KeyG),
    };
}
