//! 카메라가 내 캐릭터를 따라가고, 마차나 보스가 화면 밖에 있으면 그것이 있는 쪽의 화면 가장자리에 그것까지의 거리를 표시합니다.
//!
//! 계산은 `caravan_client_core::camera`가 합니다. 이 파일은 그 계산에 Bevy의 카메라, 창, UI 노드를 연결합니다.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::text::{Justify, TextLayout};
use bevy::window::PrimaryWindow;
use caravan_client_core::camera::{self, ViewRect};
use caravan_client_core::{Setting, boss_view};

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::view::Scene;

/// 카메라가 마지막으로 따라간 세계의 모습입니다. 값이 바뀌면 세계가 다시 놓여서 캐릭터가 등장 지점으로 돌아간 것이므로 카메라를 곧바로 옮깁니다.
/// 방을 떠나면 비웁니다(`view::clear_world`). 그래서 다음 방의 첫 프레임에도 카메라가 이전 방의 자리에서 미끄러져 오지 않습니다.
#[derive(Resource, Default)]
pub struct FollowedSetting(pub(crate) Option<Setting>);

/// 화면 가장자리의 표시가 가리키는 것입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marked {
    Wagon,
    /// 보스는 보스와 싸우는 동안에만 있으므로, 그 밖의 때에는 표시가 숨습니다.
    Boss,
}

impl Marked {
    const ALL: [Marked; 2] = [Marked::Wagon, Marked::Boss];

    fn name(self) -> &'static str {
        match self {
            Marked::Wagon => "WAGON",
            Marked::Boss => "BOSS",
        }
    }

    fn color(self) -> Color {
        match self {
            Marked::Wagon => config::WAGON_MARKER,
            Marked::Boss => config::BOSS_MARKER,
        }
    }
}

/// 화면 가장자리의 표시입니다. 가리키는 것마다 하나입니다.
#[derive(Component)]
pub struct EdgeMarkerNode(pub Marked);

/// 카메라와 가장자리의 표시를 만듭니다. 카메라는 세로로 `config::VIEW_HEIGHT_PX`만큼 보여 주고, 창의 가로는 비율에 맞춰 달라집니다.
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
    for marked in Marked::ALL {
        commands.spawn((
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(config::WAGON_MARKER_SIZE),
                ..default()
            },
            TextColor(marked.color()),
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
            EdgeMarkerNode(marked),
        ));
    }
}

/// 카메라의 중심을 내 캐릭터의 화면 위치 쪽으로 옮깁니다. 첫 프레임과 세계가 다시 놓인 프레임(원정의 시작, 야영지로 돌아옴)에는 곧바로 옮깁니다.
pub fn follow(
    time: Res<Time>,
    frame: Res<CurrentFrame>,
    scene: Res<Scene>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
    mut followed: ResMut<FollowedSetting>,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    let Some(me) = frame.player(scene.you) else {
        return;
    };
    // 발이 아니라 몸통의 가운데가 화면의 가운데에 오게 합니다.
    let target = convert::screen_pos(me.pos) + Vec2::new(0.0, config::PLAYER_BODY.y * 0.5);
    let current = (followed.0 == Some(frame.setting()))
        .then(|| convert::to_rules(camera.translation.truncate()));
    followed.0 = Some(frame.setting());

    let center = convert::from_rules(camera::follow(
        current,
        convert::to_rules(target),
        time.delta_secs(),
    ));
    camera.translation.x = center.x;
    camera.translation.y = center.y;
}

/// 마차나 보스가 화면 밖에 있으면 표시를 그쪽 가장자리에 놓고 「WAGON 23」이나 「BOSS 17」처럼 내 캐릭터에서 그것까지의 타일 수를 적습니다.
/// 보스의 자리는 스냅샷의 보스 몸에서 오므로, 보스가 없는 때에는 보스의 표시가 숨습니다.
pub fn mark_off_screen(
    frame: Res<CurrentFrame>,
    scene: Res<Scene>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<&Transform, With<Camera2d>>,
    mut markers: Query<(&EdgeMarkerNode, &mut Node, &mut Text, &mut Visibility)>,
) {
    let frame = frame.0.as_ref();
    let me = frame
        .and_then(|frame| frame.player(scene.you))
        .map(|me| me.pos);
    let boss = frame.and_then(boss_view).map(|boss| boss.body.pos);
    let view = ViewRect {
        center: convert::to_rules(camera.translation.truncate()),
        size: convert::to_rules(Vec2::new(
            config::VIEW_HEIGHT_PX * window.width() / window.height().max(1.0),
            config::VIEW_HEIGHT_PX,
        )),
    };
    for (EdgeMarkerNode(marked), mut node, mut text, mut visibility) in &mut markers {
        let ground = match marked {
            Marked::Wagon => Some(scene.wagon_center),
            Marked::Boss => boss,
        };
        let shown = me.zip(ground).and_then(|(me, ground)| {
            camera::edge_marker(
                view,
                convert::to_rules(convert::screen_pos(ground)),
                me,
                ground,
                config::WAGON_MARKER_INSET_PX,
            )
        });
        let Some(shown) = shown else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        node.left = percent(shown.anchor.x * 100.0);
        node.top = percent(shown.anchor.y * 100.0);
        let label = format!("{} {}", marked.name(), shown.distance_tiles);
        if text.0 != label {
            text.0 = label;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::net::CurrentFrame;
    use crate::palette;
    use crate::testing::{YOU, joined_flow};
    use crate::view;
    use bevy::time::TimeUpdateStrategy;
    use caravan_client_core::{Frame, MonsterView, PlayerView};
    use caravan_protocol::{
        WireAim, WireBoss, WireBossAct, WireLife, WireLoadout, WireMonsterEffects, WireMonsterKind,
        WireNightStep, WirePhase, WireStructureKind, WireWagon, WireWeaponKind,
    };
    use caravan_rules::{Team, Vec2 as Ground};

    /// 화면과 GPU 없이 시스템만 돌리는 앱입니다. 한 번 갱신할 때마다 0.05초가 흐릅니다.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .insert_resource(joined_flow())
            .init_resource::<CurrentFrame>()
            .init_resource::<FollowedSetting>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .add_systems(Startup, (palette::setup_palette, setup))
            .add_systems(
                Update,
                (
                    view::spawn_world.run_if(not(resource_exists::<Scene>)),
                    follow.run_if(resource_exists::<Scene>),
                    mark_off_screen.run_if(resource_exists::<Scene>),
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
            chests: Vec::new(),
            ground_weapons: Vec::new(),
            players: vec![PlayerView {
                id: YOU,
                team: Team::CARAVAN,
                pos: Ground::new(x, y),
                facing: Ground::new(1.0, 0.0),
                life: WireLife::Dead,
                last_attack: None,
                loadout: WireLoadout {
                    held: WireWeaponKind::Sword,
                    spare: None,
                },
                prompt: None,
                build_kind: WireStructureKind::Watchtower,
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

    fn marker(app: &mut App, marked: Marked) -> (Visibility, String) {
        let world = app.world_mut();
        let mut query = world.query::<(&EdgeMarkerNode, &Visibility, &Text)>();
        let (_, visibility, text) = query
            .iter(world)
            .find(|(node, _, _)| node.0 == marked)
            .unwrap();
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
    fn coming_back_to_the_camp_puts_the_camera_on_the_character_at_once() {
        let mut app = app();
        show(&mut app, 1, 40.0, 30.0);

        if let Some(frame) = &mut app.world_mut().resource_mut::<CurrentFrame>().0 {
            frame.phase = WirePhase::Camp {
                ready: Vec::new(),
                starting_in: None,
                struck: Vec::new(),
            };
            frame.players[0].pos = Ground::new(32.0, 30.0);
        }
        app.update();

        assert!(
            camera_at(&mut app).distance(body_center(32.0, 30.0)) < 1e-3,
            "the expedition number is the same, but the world was laid out again"
        );
    }

    #[test]
    fn a_room_drawn_anew_puts_the_camera_on_the_character_at_once_even_in_the_same_stage() {
        let mut app = app();
        show(&mut app, 1, 30.0, 30.0);

        // 방을 떠났다가 같은 단계의 다른 방에 들어간 것과 같습니다. 방을 떠나면 `view::clear_world`가 따라가던 세계의 모습을 잊습니다.
        // 두 방의 캐릭터가 가까워서(따라가는 거리 안이어서) 잊은 것만이 곧바로 옮기는 이유입니다.
        app.world_mut().insert_resource(FollowedSetting::default());
        show(&mut app, 1, 33.0, 30.0);

        assert!(
            camera_at(&mut app).distance(body_center(33.0, 30.0)) < 1e-3,
            "no sliding in from where the camera was in the other room"
        );
    }

    #[test]
    fn the_wagon_marker_shows_only_while_the_wagon_is_off_the_screen() {
        let mut app = app();
        show(&mut app, 1, 31.0, 29.0);
        assert_eq!(
            marker(&mut app, Marked::Wagon).0,
            Visibility::Hidden,
            "the wagon is in view"
        );

        show(&mut app, 2, 5.0, 5.0);
        let (visibility, label) = marker(&mut app, Marked::Wagon);
        assert_eq!(visibility, Visibility::Inherited);
        assert_eq!(label, "WAGON 38", "the wagon centre is 38 tiles away");

        show(&mut app, 3, 31.0, 29.0);
        assert_eq!(
            marker(&mut app, Marked::Wagon).0,
            Visibility::Hidden,
            "back in view"
        );
    }

    /// 보스가 `boss_at`에 서 있는 밤의 프레임을 지금 프레임에 덧씌웁니다.
    fn put_boss(app: &mut App, boss_at: Ground) {
        if let Some(frame) = &mut app.world_mut().resource_mut::<CurrentFrame>().0 {
            frame.phase = WirePhase::Night {
                day: 3,
                wave_count: 3,
                step: WireNightStep::Boss(WireBoss {
                    max_hp: 12000,
                    aim: WireAim::Wagon,
                    act: WireBossAct::Walking,
                }),
            };
            frame.monsters = vec![MonsterView {
                id: 9,
                kind: WireMonsterKind::Boss,
                effects: WireMonsterEffects::NONE,
                pos: boss_at,
                facing: Ground::new(1.0, 0.0),
                hp: 12000,
            }];
        }
        app.update();
    }

    #[test]
    fn the_boss_marker_shows_only_while_the_boss_is_off_the_screen() {
        let mut app = app();
        show(&mut app, 1, 31.0, 29.0);
        assert_eq!(
            marker(&mut app, Marked::Boss).0,
            Visibility::Hidden,
            "there is no boss yet"
        );

        put_boss(&mut app, Ground::new(31.0, 69.0));
        let (visibility, label) = marker(&mut app, Marked::Boss);
        assert_eq!(visibility, Visibility::Inherited);
        assert_eq!(label, "BOSS 40", "the boss is 40 tiles away");

        put_boss(&mut app, Ground::new(33.0, 29.0));
        assert_eq!(
            marker(&mut app, Marked::Boss).0,
            Visibility::Hidden,
            "the boss is in view"
        );
    }
}
