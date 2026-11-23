//! Bevy 클라이언트입니다. 이 워크스페이스에서 Bevy를 사용하는 크레이트는 이것 하나입니다.
//!
//! PC: `cargo run -p caravan_client -- --room test --server ws://127.0.0.1:7777`
//! 웹: `cd crates/client && trunk serve` 뒤에 `http://127.0.0.1:8080/?room=test` 열기
//!
//! 서버가 Tick마다 보내는 스냅샷을 받아서 첫 번째 플레이어의 자리에 표식(사각형) 하나를 그리고, 화면 왼쪽 위에 접속 상태를 표시합니다.

// Bevy 시스템은 매개변수가 많고 쿼리 타입이 길어지는 것이 정상입니다.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod convert;
mod diagnostics;
mod input;
mod net;

use bevy::prelude::*;
use caravan_client_core::{LaunchError, LaunchOptions};

#[derive(Component)]
struct Marker;

#[derive(Component)]
struct StatusText;

fn main() {
    let launch = read_launch_options();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "THE LAST CARAVAN".to_string(),
            // 웹에서는 index.html의 이 캔버스에 그립니다. PC에서는 무시됩니다.
            canvas: Some("#bevy".to_string()),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }));
    // 로그는 `DefaultPlugins`가 설정하므로, 접속을 시작하는 로그를 남기려면 플러그인을 먼저 추가합니다.
    net::install(&mut app, launch);
    app.init_resource::<input::HeldControls>()
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (
                net::pump_network,
                net::advance_playback,
                draw_marker,
                show_status,
                input::read_controls,
                net::send_input,
                diagnostics::report,
            )
                .chain(),
        )
        .run();
}

#[cfg(not(target_arch = "wasm32"))]
fn read_launch_options() -> Result<LaunchOptions, LaunchError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    LaunchOptions::from_args(&args)
}

/// 페이지 주소의 `?room=test&server=ws://...&debug-net`을 읽습니다.
#[cfg(target_arch = "wasm32")]
fn read_launch_options() -> Result<LaunchOptions, LaunchError> {
    let query = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| web_sys::UrlSearchParams::new_with_str(&search).ok());
    let get = |name: &str| query.as_ref().and_then(|params| params.get(name));
    let debug_net = query.as_ref().is_some_and(|params| params.has("debug-net"));
    LaunchOptions::from_values(get("room"), get("server"), debug_net)
}

fn setup_scene(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        Sprite::from_color(Color::srgb(0.95, 0.75, 0.3), Vec2::splat(20.0)),
        Transform::default(),
        Visibility::Hidden,
        Marker,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
        StatusText,
    ));
}

fn draw_marker(
    frame: Res<net::CurrentFrame>,
    marker: Single<(&mut Transform, &mut Visibility), With<Marker>>,
) {
    let (mut transform, mut visibility) = marker.into_inner();
    let Some(first) = frame.0.as_ref().and_then(|frame| frame.players.first()) else {
        *visibility = Visibility::Hidden;
        return;
    };
    let position = convert::screen_pos(first.pos);
    transform.translation.x = position.x;
    transform.translation.y = position.y;
    *visibility = Visibility::Visible;
}

fn show_status(session: Res<net::ClientSession>, mut text: Single<&mut Text, With<StatusText>>) {
    let status = session.0.status_text();
    if text.0 != status {
        text.0 = status;
    }
}
