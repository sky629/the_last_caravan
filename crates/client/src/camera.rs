//! 카메라가 내 캐릭터를 따라가고, 마차가 화면 밖에 있으면 마차가 있는 쪽의 화면 가장자리에 마차까지의 거리를 표시합니다.
//!
//! 계산은 `caravan_client_core::camera`가 합니다. 이 파일은 그 계산에 Bevy의 카메라, 창, UI 노드를 연결합니다.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::text::{Justify, TextLayout};
use bevy::window::PrimaryWindow;
use caravan_client_core::camera::{self, ViewRect};

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::view::Scene;

/// 카메라가 마지막으로 따라간 원정의 번호입니다. 번호가 바뀌면 새 원정이므로 카메라를 곧바로 옮깁니다.
#[derive(Default)]
pub struct FollowedExpedition(Option<u32>);

/// 화면 가장자리의 마차 표시입니다.
#[derive(Component)]
pub struct WagonMarkerNode;

/// 카메라와 마차 표시를 만듭니다. 카메라는 세로로 `config::VIEW_HEIGHT_PX`만큼 보여 주고, 창의 가로는 비율에 맞춰 달라집니다.
pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: config::VIEW_HEIGHT_PX,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(config::WAGON_MARKER_SIZE),
            ..default()
        },
        TextColor(config::WAGON_MARKER),
        TextShadow::default(),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            width: px(config::WAGON_MARKER_WIDTH),
            margin: UiRect {
                left: px(-config::WAGON_MARKER_WIDTH / 2.0),
                top: px(-config::WAGON_MARKER_SIZE / 2.0),
                ..default()
            },
            ..default()
        },
        Visibility::Hidden,
        WagonMarkerNode,
    ));
}

/// 카메라의 중심을 내 캐릭터의 화면 위치 쪽으로 옮깁니다. 첫 프레임과 새 원정이 시작된 프레임에는 곧바로 옮깁니다.
pub fn follow(
    time: Res<Time>,
    frame: Res<CurrentFrame>,
    scene: Res<Scene>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
    mut followed: Local<FollowedExpedition>,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    let Some(me) = frame.player(scene.you) else {
        return;
    };
    // 발이 아니라 몸통의 가운데가 화면의 가운데에 오게 합니다.
    let target = convert::screen_pos(me.pos) + Vec2::new(0.0, config::PLAYER_BODY.y * 0.5);
    let current = (followed.0 == Some(frame.expedition))
        .then(|| convert::to_rules(camera.translation.truncate()));
    followed.0 = Some(frame.expedition);

    let center = convert::from_rules(camera::follow(
        current,
        convert::to_rules(target),
        time.delta_secs(),
    ));
    camera.translation.x = center.x;
    camera.translation.y = center.y;
}

/// 마차가 화면 밖에 있으면 표시를 마차 쪽 가장자리에 놓고 「WAGON 23」처럼 내 캐릭터에서 마차까지의 타일 수를 적습니다.
pub fn mark_the_wagon(
    frame: Res<CurrentFrame>,
    scene: Res<Scene>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<&Transform, With<Camera2d>>,
    mut marker: Single<(&mut Node, &mut Text, &mut Visibility), With<WagonMarkerNode>>,
) {
    let (node, text, visibility) = &mut *marker;
    let me = frame
        .0
        .as_ref()
        .and_then(|frame| frame.player(scene.you))
        .map(|me| me.pos);
    let view = ViewRect {
        center: convert::to_rules(camera.translation.truncate()),
        size: convert::to_rules(Vec2::new(
            config::VIEW_HEIGHT_PX * window.width() / window.height().max(1.0),
            config::VIEW_HEIGHT_PX,
        )),
    };
    let shown = me.and_then(|me| {
        camera::wagon_marker(
            view,
            convert::to_rules(convert::screen_pos(scene.wagon_center)),
            me,
            scene.wagon_center,
            config::WAGON_MARKER_INSET_PX,
        )
    });
    let Some(shown) = shown else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    node.left = percent(shown.anchor.x * 100.0);
    node.top = percent(shown.anchor.y * 100.0);
    let label = format!("WAGON {}", shown.distance_tiles);
    if text.0 != label {
        text.0 = label;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::net::CurrentFrame;
    use crate::palette;
    use crate::testing::{YOU, joined_session};
    use crate::view;
    use bevy::time::TimeUpdateStrategy;
    use caravan_client_core::{Frame, PlayerView};
    use caravan_protocol::{WireLife, WirePhase, WireWagon, WireWeaponKind};
    use caravan_rules::{Team, Vec2 as Ground};

    /// 화면과 GPU 없이 시스템만 돌리는 앱입니다. 한 번 갱신할 때마다 0.05초가 흐릅니다.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .insert_resource(joined_session())
            .init_resource::<CurrentFrame>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .add_systems(Startup, (palette::setup_palette, setup))
            .add_systems(
                Update,
                (
                    view::spawn_world.run_if(not(resource_exists::<Scene>)),
                    follow.run_if(resource_exists::<Scene>),
                    mark_the_wagon.run_if(resource_exists::<Scene>),
                )
                    .chain(),
            );
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app
    }

    fn show(app: &mut App, expedition: u32, x: f32, y: f32) {
        app.world_mut().resource_mut::<CurrentFrame>().0 = Some(Frame {
            tick: 1.0,
            expedition,
            phase: WirePhase::Day {
                day: 1,
                secs_left: 90.0,
            },
            wagon: WireWagon {
                hp: 500,
                max_hp: 500,
            },
            stash: 0,
            towers: Vec::new(),
            nodes: Vec::new(),
            piles: Vec::new(),
            players: vec![PlayerView {
                id: YOU,
                team: Team::CARAVAN,
                pos: Ground::new(x, y),
                facing: Ground::new(1.0, 0.0),
                life: WireLife::Dead,
                last_attack: None,
                weapon: WireWeaponKind::Sword,
            }],
            monsters: Vec::new(),
            projectiles: Vec::new(),
        });
        app.update();
    }

    fn camera_at(app: &mut App) -> Vec2 {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&Transform, With<Camera2d>>();
        query.single(world).unwrap().translation.truncate()
    }

    fn marker(app: &mut App) -> (Visibility, String) {
        let world = app.world_mut();
        let mut query = world.query_filtered::<(&Visibility, &Text), With<WagonMarkerNode>>();
        let (visibility, text) = query.single(world).unwrap();
        (*visibility, text.0.clone())
    }

    fn body_center(x: f32, y: f32) -> Vec2 {
        convert::screen_pos(Ground::new(x, y)) + Vec2::new(0.0, config::PLAYER_BODY.y * 0.5)
    }

    #[test]
    fn the_camera_starts_on_my_character_and_eases_toward_it_as_it_moves() {
        let mut app = app();
        show(&mut app, 1, 30.0, 30.0);
        assert!(camera_at(&mut app).distance(body_center(30.0, 30.0)) < 1e-3);

        show(&mut app, 1, 33.0, 30.0);
        let eased = camera_at(&mut app);
        let (from, to) = (body_center(30.0, 30.0), body_center(33.0, 30.0));
        assert!(
            eased.x > from.x && eased.x < to.x,
            "moved toward {to:?} but not all the way: {eased:?}"
        );
    }

    #[test]
    fn a_new_expedition_puts_the_camera_on_the_character_at_once() {
        let mut app = app();
        show(&mut app, 1, 30.0, 30.0);

        show(&mut app, 2, 28.0, 30.0);

        assert!(camera_at(&mut app).distance(body_center(28.0, 30.0)) < 1e-3);
    }

    #[test]
    fn the_wagon_marker_shows_only_while_the_wagon_is_off_the_screen() {
        let mut app = app();
        show(&mut app, 1, 31.0, 29.0);
        assert_eq!(
            marker(&mut app).0,
            Visibility::Hidden,
            "the wagon is in view"
        );

        show(&mut app, 2, 5.0, 5.0);
        let (visibility, label) = marker(&mut app);
        assert_eq!(visibility, Visibility::Inherited);
        assert_eq!(label, "WAGON 38", "the wagon centre is 38 tiles away");

        show(&mut app, 3, 31.0, 29.0);
        assert_eq!(marker(&mut app).0, Visibility::Hidden, "back in view");
    }
}
