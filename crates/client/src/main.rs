//! Bevy 클라이언트입니다. 이 워크스페이스에서 Bevy를 사용하는 크레이트는 이것 하나입니다.
//!
//! PC: `cargo run -p caravan_client` (시작 화면), `cargo run -p caravan_client -- --name KANG --room k7m2q --server ws://127.0.0.1:7777` (곧바로 입장)
//! 웹: `cd crates/client && trunk serve` 뒤에 `http://127.0.0.1:8080/` 또는 `http://127.0.0.1:8080/?room=k7m2q` 열기(서버는 따로 `caravan-dedicated`로 켭니다)
//! 웹(서버가 내준 페이지): `sh scripts/playtest.sh`, 또는 `caravan-dedicated --web-dir <trunk build의 출력 폴더>` 뒤에 `http://127.0.0.1:7777/` 열기
//!
//! 시작 화면에서 이름과 방 코드를 넣고 입장하면, 서버가 Tick마다 보내는 방 전체의 상태를 받아서 쿼터뷰로 그립니다.
//! 읽는 순서는 `main`(한 프레임의 순서), `net`, `start`, `storage`, `view`, `camera`, `towers`, `supplies`, `loot`, `camp`, `characters`입니다.

// Bevy 시스템은 매개변수가 많고 쿼리 타입이 길어지는 것이 정상입니다.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod camera;
mod camp;
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
mod start;
mod storage;
mod supplies;
#[cfg(test)]
mod testing;
mod towers;
mod view;

use bevy::prelude::*;
use caravan_client_core::{Flow, LaunchError, LaunchOptions, Page};

fn main() {
    let page = page();
    let launch = read_launch_options(page.as_ref());
    let debug_net = launch.as_ref().is_ok_and(|launch| launch.debug_net);
    let profile = launch
        .as_ref()
        .ok()
        .and_then(|launch| launch.profile.clone());
    let (flow, first) = Flow::new(launch, storage::load(profile.as_deref()), page);
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
    net::install(&mut app, flow, first, debug_net, profile);
    app.insert_resource(ClearColor(config::BACKGROUND))
        .init_resource::<input::HeldControls>()
        .init_resource::<characters::Spawned>()
        .init_resource::<towers::TowerRegistry>()
        .init_resource::<supplies::SupplyRegistry>()
        .init_resource::<loot::LootRegistry>()
        .init_resource::<camp::StrikeRegistry>()
        .init_resource::<camera::FollowedSetting>()
        .add_systems(
            Startup,
            (
                palette::setup_palette,
                effects::configure_gizmos,
                hud::setup,
                camera::setup,
                start::setup,
            ),
        )
        // 한 프레임에 하는 일을 순서대로 모두 나열한 곳입니다. 앞 단계가 쓴 값을 뒤 단계가 읽습니다.
        // 시작 화면에서는 세계를 그리지 않고 방에 들어가면 시작 화면을 숨기며, 방을 떠나면 세계를 지웁니다.
        .add_systems(
            Update,
            (
                (
                    net::keys_then_network(),
                    start::draw_form,
                    net::advance_playback,
                    view::clear_world
                        .run_if(not(start::in_room).and_then(resource_exists::<view::Scene>)),
                    view::spawn_world
                        .run_if(start::in_room.and_then(not(resource_exists::<view::Scene>))),
                    camera::follow.run_if(resource_exists::<view::Scene>),
                    camera::mark_the_wagon.run_if(resource_exists::<view::Scene>),
                    towers::sync_towers.run_if(resource_exists::<view::Scene>),
                    supplies::sync_nodes.run_if(resource_exists::<view::Scene>),
                    supplies::sync_piles.run_if(resource_exists::<view::Scene>),
                )
                    .chain(),
                (
                    loot::sync_chests.run_if(resource_exists::<view::Scene>),
                    loot::sync_ground_weapons.run_if(resource_exists::<view::Scene>),
                    loot::sync_racks.run_if(resource_exists::<view::Scene>),
                    camp::sync_strikes.run_if(resource_exists::<view::Scene>),
                    characters::sync_characters.run_if(resource_exists::<view::Scene>),
                    characters::sync_name_labels.run_if(resource_exists::<view::Scene>),
                    effects::draw_effects.run_if(resource_exists::<view::Scene>),
                    hud::build_roster,
                    hud::update,
                    input::read_controls.run_if(start::in_room),
                    net::read_room_keys.run_if(start::playing),
                    net::send_input,
                    diagnostics::report,
                )
                    .chain(),
            )
                .chain(),
        )
        .run();
}

#[cfg(not(target_arch = "wasm32"))]
fn read_launch_options(_page: Option<&Page>) -> Result<LaunchOptions, LaunchError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    LaunchOptions::from_args(&args)
}

/// 페이지 주소의 `?room=k7m2q&name=KANG&server=ws://...&debug-net`을 읽습니다. `?server=`가 없으면 페이지가 정한 서버에 접속합니다.
#[cfg(target_arch = "wasm32")]
fn read_launch_options(page: Option<&Page>) -> Result<LaunchOptions, LaunchError> {
    let query = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| web_sys::UrlSearchParams::new_with_str(&search).ok());
    let get = |name: &str| query.as_ref().and_then(|params| params.get(name));
    let debug_net = query.as_ref().is_some_and(|params| params.has("debug-net"));
    let (room, name, server) = (get("room"), get("name"), get("server"));
    match page {
        Some(page) => LaunchOptions::from_page(room, name, server, debug_net, page),
        None => LaunchOptions::from_values(room, name, server, debug_net),
    }
}

/// 웹 페이지가 열린 곳입니다. 게임 서버가 내준 페이지인지와 서버의 공개 주소는 html의 `<meta name="caravan-public-url">`가 알려 줍니다.
#[cfg(target_arch = "wasm32")]
fn page() -> Option<Page> {
    let window = web_sys::window()?;
    let location = window.location();
    let marker = window
        .document()
        .and_then(|document| {
            document
                .query_selector("meta[name=caravan-public-url]")
                .ok()
                .flatten()
        })
        .and_then(|meta| meta.get_attribute("content"));
    Some(Page::open(
        &location.origin().ok()?,
        &location.pathname().ok()?,
        marker.as_deref(),
    ))
}

/// PC에는 페이지가 없으므로 방 코드만 복사합니다.
#[cfg(not(target_arch = "wasm32"))]
fn page() -> Option<Page> {
    None
}
