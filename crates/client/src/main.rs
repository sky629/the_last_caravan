//! Bevy 클라이언트입니다. 이 워크스페이스에서 Bevy를 사용하는 크레이트는 이것 하나입니다.
//!
//! PC: `cargo run -p caravan_client -- --room test --server ws://127.0.0.1:7777`
//! 웹: `cd crates/client && trunk serve` 뒤에 `http://127.0.0.1:8080/?room=test` 열기
//!
//! 서버가 Tick마다 보내는 방 전체의 상태를 받아서 쿼터뷰로 그립니다. 읽는 순서는 `main`(한 프레임의 순서), `net`, `view`, `camera`, `towers`, `supplies`, `loot`, `characters`입니다.

// Bevy 시스템은 매개변수가 많고 쿼리 타입이 길어지는 것이 정상입니다.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod camera;
mod characters;
mod config;
mod convert;
mod diagnostics;
mod effects;
mod hud;
mod input;
mod loot;
mod net;
mod palette;
mod parts;
mod registry;
mod supplies;
#[cfg(test)]
mod testing;
mod towers;
mod view;

use bevy::prelude::*;
use caravan_client_core::{LaunchError, LaunchOptions};

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
    app.insert_resource(ClearColor(config::BACKGROUND))
        .init_resource::<input::HeldControls>()
        .init_resource::<characters::Spawned>()
        .init_resource::<towers::TowerRegistry>()
        .init_resource::<supplies::SupplyRegistry>()
        .init_resource::<loot::LootRegistry>()
        .add_systems(
            Startup,
            (
                palette::setup_palette,
                effects::configure_gizmos,
                hud::setup,
                camera::setup,
            ),
        )
        // 한 프레임에 하는 일을 순서대로 모두 나열한 곳입니다. 앞 단계가 쓴 값을 뒤 단계가 읽습니다.
        .add_systems(
            Update,
            (
                net::pump_network,
                net::advance_playback,
                view::spawn_world.run_if(not(resource_exists::<view::Scene>)),
                camera::follow.run_if(resource_exists::<view::Scene>),
                camera::mark_the_wagon.run_if(resource_exists::<view::Scene>),
                towers::sync_towers.run_if(resource_exists::<view::Scene>),
                supplies::sync_nodes.run_if(resource_exists::<view::Scene>),
                supplies::sync_piles.run_if(resource_exists::<view::Scene>),
                loot::sync_chests.run_if(resource_exists::<view::Scene>),
                loot::sync_ground_weapons.run_if(resource_exists::<view::Scene>),
                characters::sync_characters.run_if(resource_exists::<view::Scene>),
                effects::draw_effects.run_if(resource_exists::<view::Scene>),
                hud::build_roster,
                hud::update,
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
