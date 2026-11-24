//! 플레이어와 몬스터를 화면의 도형으로 만들고 갱신합니다.
//!
//! 서버의 개체 번호에서 Bevy 개체로 가는 표(`Registry`)를 두고, 새 번호만 만들고 사라진 번호만 지웁니다.
//! 프레임마다 모든 개체를 다시 만들지 않으므로 몬스터가 수백 마리여도 매 프레임 하는 일은 위치와 막대를 맞추는 것뿐입니다.

use bevy::prelude::*;
use caravan_client_core::{MonsterView, PlayerView, bar_fraction};
use caravan_protocol::WireLife;
use caravan_rules::Vec2 as Ground;

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::palette::Palette;
use crate::parts::{self, PartQuery, Parts};
use crate::registry::Registry;
use crate::view::{Footprint, Scene};

#[derive(Resource, Default)]
pub struct Spawned {
    players: Registry,
    monsters: Registry,
}

/// 이번 프레임에 캐릭터 하나가 어떻게 보이는지입니다. 플레이어와 몬스터가 같은 도형을 쓰므로 이 값 하나로 둘을 그립니다.
struct Look {
    foot: Ground,
    facing: Ground,
    radius: f32,
    body: Handle<ColorMaterial>,
    body_size: Vec2,
    /// HP 막대의 채움 비율입니다. 없으면 막대를 숨깁니다.
    bar: Option<f32>,
}

/// 캐릭터를 처음 만들 때 한 번 정하는 값입니다.
struct Kit<'a> {
    shadow: &'a Handle<Mesh>,
    nose: &'a Handle<ColorMaterial>,
    bar_fill: &'a Handle<ColorMaterial>,
}

/// 뿌리(그림자)의 자식 도형들이 놓일 자리입니다.
struct Layout {
    body: Transform,
    nose: Transform,
}

impl Look {
    fn layout(&self) -> Layout {
        let step = config::PART_Z_STEP;
        let body = Transform::from_xyz(0.0, self.body_size.y / 2.0, step)
            .with_scale(self.body_size.extend(1.0));
        // 바라보는 방향의 점은 지면에서 방향으로 반지름만큼 떨어진 자리의 화면 위치에, 몸통의 가슴 높이로 놓습니다.
        let toward = convert::screen_pos(Ground::new(
            self.facing.x * self.radius * 1.1,
            self.facing.y * self.radius * 1.1,
        ));
        let nose = Transform::from_xyz(toward.x, toward.y + self.body_size.y * 0.55, 2.0 * step)
            .with_scale(Vec3::new(config::NOSE_RADIUS, config::NOSE_RADIUS, 1.0));
        Layout { body, nose }
    }

    fn bar_y(&self) -> f32 {
        self.body_size.y + config::BAR_GAP
    }
}

fn spawn_character(commands: &mut Commands, palette: &Palette, kit: &Kit, look: &Look) -> Entity {
    let layout = look.layout();
    let body = commands
        .spawn((
            Mesh2d(palette.square.clone()),
            MeshMaterial2d(look.body.clone()),
            layout.body,
        ))
        .id();
    let nose = commands
        .spawn((
            Mesh2d(palette.circle.clone()),
            MeshMaterial2d(kit.nose.clone()),
            layout.nose,
        ))
        .id();
    let bar = parts::spawn_bar(commands, palette, kit.bar_fill, look.bar_y(), look.bar);
    let [bar_back, bar_fill] = bar.entities();
    commands
        .spawn((
            Mesh2d(kit.shadow.clone()),
            MeshMaterial2d(palette.shadow.clone()),
            Transform::from_translation(convert::placement(look.foot)),
            Parts { body, nose, bar },
        ))
        .add_children(&[body, nose, bar_back, bar_fill])
        .id()
}

/// 이미 있는 캐릭터를 이번 프레임의 모습에 맞춥니다.
fn update_character(
    look: &Look,
    parts: Parts,
    transform: &mut Transform,
    part_query: &mut PartQuery,
    materials: &mut Query<&mut MeshMaterial2d<ColorMaterial>>,
) {
    transform.translation = convert::placement(look.foot);
    let layout = look.layout();
    parts::set_part(part_query, parts.body, layout.body, true);
    parts::set_part(part_query, parts.nose, layout.nose, true);
    parts.bar.place(part_query, look.bar_y(), look.bar);
    if let Ok(mut material) = materials.get_mut(parts.body)
        && material.0 != look.body
    {
        material.0 = look.body.clone();
    }
}

fn player_look(player: &PlayerView, scene: &Scene, palette: &Palette) -> Look {
    let alive_color = if player.id == scene.you {
        &palette.you
    } else if player.team.is_allied_with(scene.team) {
        &palette.ally
    } else {
        &palette.hostile_player
    };
    let (body, height, bar) = match player.life {
        WireLife::Alive { hp, max_hp, .. } => (
            alive_color,
            config::PLAYER_BODY.y,
            Some(bar_fraction(hp, max_hp)),
        ),
        WireLife::Downed { .. } => (&palette.downed, config::DOWNED_BODY_HEIGHT, None),
        WireLife::Dead => (&palette.dead, config::DEAD_BODY_HEIGHT, None),
    };
    Look {
        foot: player.pos,
        facing: player.facing,
        radius: scene.player.radius,
        body: body.clone(),
        body_size: Vec2::new(config::PLAYER_BODY.x, height),
        bar,
    }
}

/// 몬스터의 HP 막대는 피해를 입었을 때만 보입니다. 수백 마리가 막대를 모두 달고 있으면 화면이 복잡해집니다.
fn monster_look(
    monster: &MonsterView,
    footprint: &Footprint,
    max_hp: u32,
    palette: &Palette,
) -> Look {
    Look {
        foot: monster.pos,
        facing: monster.facing,
        radius: footprint.radius,
        body: palette.monster.clone(),
        body_size: config::MONSTER_BODY,
        bar: (monster.hp < max_hp).then(|| bar_fraction(monster.hp, max_hp)),
    }
}

/// 플레이어와 몬스터의 도형을 이번 프레임의 모습에 맞춥니다.
pub fn sync_characters(
    frame: Res<CurrentFrame>,
    scene: Res<Scene>,
    palette: Res<Palette>,
    mut spawned: ResMut<Spawned>,
    mut commands: Commands,
    mut roots: Query<(&mut Transform, &Parts)>,
    mut part_query: PartQuery,
    mut materials: Query<&mut MeshMaterial2d<ColorMaterial>>,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    spawned.players.begin_frame();
    spawned.monsters.begin_frame();

    let player_kit = Kit {
        shadow: &scene.player.shadow,
        nose: &palette.nose_player,
        bar_fill: &palette.bar_player,
    };
    for player in &frame.players {
        let look = player_look(player, &scene, &palette);
        match spawned.players.find(player.id) {
            Some(entity) => {
                if let Ok((mut transform, parts)) = roots.get_mut(entity) {
                    update_character(
                        &look,
                        *parts,
                        &mut transform,
                        &mut part_query,
                        &mut materials,
                    );
                }
            }
            None => {
                let entity = spawn_character(&mut commands, &palette, &player_kit, &look);
                spawned.players.insert(player.id, entity);
            }
        }
    }

    for monster in &frame.monsters {
        let Some(kind) = scene.monster(monster.kind) else {
            continue;
        };
        let look = monster_look(monster, &kind.footprint, kind.max_hp, &palette);
        match spawned.monsters.find(monster.id) {
            Some(entity) => {
                if let Ok((mut transform, parts)) = roots.get_mut(entity) {
                    update_character(
                        &look,
                        *parts,
                        &mut transform,
                        &mut part_query,
                        &mut materials,
                    );
                }
            }
            None => {
                let kit = Kit {
                    shadow: &kind.footprint.shadow,
                    nose: &palette.nose_monster,
                    bar_fill: &palette.bar_monster,
                };
                let entity = spawn_character(&mut commands, &palette, &kit, &look);
                spawned.monsters.insert(monster.id, entity);
            }
        }
    }

    spawned.players.despawn_unseen(&mut commands);
    spawned.monsters.despawn_unseen(&mut commands);
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::testing::{YOU, joined_session};
    use crate::view;
    use caravan_client_core::Frame;
    use caravan_protocol::{WireMonsterKind, WirePhase, WireWagon};
    use caravan_rules::Team;

    /// 화면과 GPU 없이 시스템만 돌리는 앱입니다. 입장을 마친 세션을 가집니다.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .insert_resource(joined_session())
            .init_resource::<CurrentFrame>()
            .init_resource::<Spawned>()
            .add_systems(Startup, crate::palette::setup_palette)
            .add_systems(
                Update,
                (
                    view::spawn_world.run_if(not(resource_exists::<Scene>)),
                    sync_characters.run_if(resource_exists::<Scene>),
                )
                    .chain(),
            );
        app
    }

    fn alive(hp: u32) -> WireLife {
        WireLife::Alive {
            hp,
            max_hp: 100,
            carried: 0,
            interaction: None,
            build_refusal: None,
        }
    }

    fn player(id: u32, x: f32, y: f32, life: WireLife) -> PlayerView {
        PlayerView {
            id,
            team: Team::CARAVAN,
            pos: Ground::new(x, y),
            facing: Ground::new(1.0, 0.0),
            life,
            last_attack_tick: None,
        }
    }

    fn monster(id: u32, x: f32, y: f32, hp: u32) -> MonsterView {
        MonsterView {
            id,
            kind: WireMonsterKind::Grunt,
            pos: Ground::new(x, y),
            facing: Ground::new(0.0, 1.0),
            hp,
        }
    }

    fn frame(players: Vec<PlayerView>, monsters: Vec<MonsterView>) -> Frame {
        Frame {
            tick: 1.0,
            expedition: 1,
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
            players,
            monsters,
        }
    }

    fn show(app: &mut App, frame: Frame) {
        app.world_mut().resource_mut::<CurrentFrame>().0 = Some(frame);
        app.update();
    }

    fn shapes(app: &mut App) -> HashSet<Entity> {
        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, With<Parts>>();
        query.iter(world).collect()
    }

    fn monster_entity(app: &App, id: u32) -> Entity {
        app.world()
            .resource::<Spawned>()
            .monsters
            .entity_of(id)
            .unwrap()
    }

    #[test]
    fn a_monster_is_spawned_once_kept_while_it_lives_and_removed_when_it_is_gone() {
        let mut app = app();
        let me = player(YOU, 12.0, 9.0, alive(100));
        show(
            &mut app,
            frame(
                vec![me.clone()],
                vec![monster(1, 3.0, 3.0, 60), monster(2, 4.0, 3.0, 60)],
            ),
        );
        let first = shapes(&mut app);
        assert_eq!(first.len(), 3, "one player and two monsters");
        let kept = monster_entity(&app, 2);

        // 같은 상태를 다시 그려도 새로 만들지 않습니다.
        show(
            &mut app,
            frame(
                vec![me.clone()],
                vec![monster(1, 3.5, 3.0, 60), monster(2, 4.5, 3.0, 60)],
            ),
        );
        assert_eq!(shapes(&mut app), first);

        // 몬스터 1이 죽고 3이 나타납니다. 2는 같은 개체로 남습니다.
        show(
            &mut app,
            frame(
                vec![me],
                vec![monster(2, 5.0, 3.0, 60), monster(3, 6.0, 3.0, 60)],
            ),
        );
        let after = shapes(&mut app);
        assert_eq!(after.len(), 3);
        assert_eq!(
            after.intersection(&first).count(),
            2,
            "me and monster 2 stay"
        );
        assert_eq!(monster_entity(&app, 2), kept);
    }

    #[test]
    fn a_player_who_left_disappears() {
        let mut app = app();
        show(
            &mut app,
            frame(
                vec![
                    player(YOU, 12.0, 9.0, alive(100)),
                    player(2, 8.0, 12.0, alive(100)),
                ],
                vec![],
            ),
        );
        assert_eq!(shapes(&mut app).len(), 2);

        show(
            &mut app,
            frame(vec![player(YOU, 12.0, 9.0, alive(100))], vec![]),
        );
        assert_eq!(shapes(&mut app).len(), 1);
    }

    #[test]
    fn the_point_lower_on_the_screen_is_in_front() {
        let mut app = app();
        show(
            &mut app,
            frame(
                vec![],
                vec![monster(1, 10.0, 5.0, 60), monster(2, 10.0, 10.0, 60)],
            ),
        );
        let near = app
            .world()
            .get::<Transform>(monster_entity(&app, 1))
            .unwrap();
        let far = app
            .world()
            .get::<Transform>(monster_entity(&app, 2))
            .unwrap();
        assert!(
            near.translation.y < far.translation.y,
            "lower on the screen"
        );
        assert!(near.translation.z > far.translation.z, "and drawn in front");
    }

    #[test]
    fn you_an_ally_a_downed_and_a_dead_player_look_different() {
        let mut app = app();
        let downed = WireLife::Downed {
            bleed_secs_left: 20.0,
            revive_progress: None,
        };
        show(
            &mut app,
            frame(
                vec![
                    player(YOU, 12.0, 9.0, alive(100)),
                    player(2, 8.0, 12.0, alive(100)),
                    player(3, 9.0, 12.0, downed),
                    player(4, 10.0, 12.0, WireLife::Dead),
                ],
                vec![],
            ),
        );

        let palette = app.world().resource::<Palette>();
        let expected = [
            (YOU, palette.you.clone()),
            (2, palette.ally.clone()),
            (3, palette.downed.clone()),
            (4, palette.dead.clone()),
        ];
        for (id, material) in expected {
            let root = app
                .world()
                .resource::<Spawned>()
                .players
                .entity_of(id)
                .unwrap();
            let parts = app.world().get::<Parts>(root).unwrap();
            let body = app
                .world()
                .get::<MeshMaterial2d<ColorMaterial>>(parts.body)
                .unwrap();
            assert_eq!(body.0, material, "player {id}");
        }
    }

    #[test]
    fn a_bar_for_a_player_whose_max_hp_is_zero_is_empty_and_has_a_valid_position() {
        let mut app = app();
        let broken = WireLife::Alive {
            hp: 0,
            max_hp: 0,
            carried: 0,
            interaction: None,
            build_refusal: None,
        };
        show(
            &mut app,
            frame(vec![player(YOU, 12.0, 9.0, broken)], vec![]),
        );

        let root = app
            .world()
            .resource::<Spawned>()
            .players
            .entity_of(YOU)
            .unwrap();
        let parts = *app.world().get::<Parts>(root).unwrap();
        let [_, fill] = parts.bar.entities();
        let transform = app.world().get::<Transform>(fill).unwrap();
        assert!(transform.translation.is_finite() && transform.scale.is_finite());
    }

    #[test]
    fn a_player_on_another_team_is_drawn_differently_from_an_ally() {
        let mut app = app();
        let rival = PlayerView {
            team: Team(2),
            ..player(3, 9.0, 12.0, alive(100))
        };
        show(
            &mut app,
            frame(
                vec![
                    player(YOU, 12.0, 9.0, alive(100)),
                    player(2, 8.0, 12.0, alive(100)),
                    rival,
                ],
                vec![],
            ),
        );

        let palette = app.world().resource::<Palette>();
        assert_ne!(palette.hostile_player, palette.ally);
        let expected = [
            (2, palette.ally.clone()),
            (3, palette.hostile_player.clone()),
        ];
        for (id, material) in expected {
            let root = app
                .world()
                .resource::<Spawned>()
                .players
                .entity_of(id)
                .unwrap();
            let parts = app.world().get::<Parts>(root).unwrap();
            let body = app
                .world()
                .get::<MeshMaterial2d<ColorMaterial>>(parts.body)
                .unwrap();
            assert_eq!(body.0, material, "player {id}");
        }
    }
}
