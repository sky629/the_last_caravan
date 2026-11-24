//! 감시탑을 화면의 상자로 만들고 갱신합니다.
//!
//! 감시탑은 건설과 파괴로 늘고 줄므로 스냅샷의 번호 목록과 Bevy 개체를 맞춥니다(캐릭터와 같은 방식입니다).
//! 짓는 중인 감시탑은 낮은 공사터로 그리고 막대가 건설의 진행을, 서 있는 감시탑은 막대가 HP를 나타냅니다.

use bevy::prelude::*;
use caravan_client_core::bar_fraction;
use caravan_protocol::{WireTower, WireTowerState};

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::palette::Palette;
use crate::parts::{self, Bar, PartQuery};
use crate::registry::Registry;

#[derive(Resource, Default)]
pub struct TowerRegistry(Registry);

#[derive(Component)]
pub struct TowerView {
    bar: Bar,
}

/// 감시탑이 이번 프레임에 어떻게 보이는지입니다.
struct Look {
    block: Handle<Mesh>,
    paint: Handle<ColorMaterial>,
    bar_fill: Handle<ColorMaterial>,
    /// 짓는 중이면 건설의 진행이고, 서 있으면 HP의 비율입니다.
    bar: f32,
    height: f32,
}

fn look_of(tower: &WireTower, palette: &Palette) -> Look {
    match tower.state {
        WireTowerState::Building { progress } => Look {
            block: palette.site_block.clone(),
            paint: palette.site.clone(),
            bar_fill: palette.bar_site.clone(),
            bar: progress,
            height: config::SITE_HEIGHT,
        },
        WireTowerState::Standing { hp, .. } => Look {
            block: palette.tower_block.clone(),
            paint: palette.tower.clone(),
            bar_fill: palette.bar_tower.clone(),
            bar: bar_fraction(hp, tower.max_hp),
            height: config::TOWER_HEIGHT,
        },
    }
}

/// 스냅샷의 감시탑에 맞게 개체를 만들고, 맞추고, 지웁니다.
pub fn sync_towers(
    frame: Res<CurrentFrame>,
    palette: Res<Palette>,
    mut registry: ResMut<TowerRegistry>,
    mut commands: Commands,
    views: Query<&TowerView>,
    mut meshes: Query<&mut Mesh2d>,
    mut materials: Query<&mut MeshMaterial2d<ColorMaterial>>,
    mut part_query: PartQuery,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    registry.0.begin_frame();
    for tower in &frame.towers {
        let look = look_of(tower, &palette);
        let bar_y = look.height + config::BAR_GAP;
        match registry.0.find(tower.id) {
            Some(entity) => {
                let Ok(view) = views.get(entity) else {
                    continue;
                };
                if let Ok(mut mesh) = meshes.get_mut(entity)
                    && mesh.0 != look.block
                {
                    mesh.0 = look.block;
                }
                if let Ok(mut material) = materials.get_mut(entity)
                    && material.0 != look.paint
                {
                    material.0 = look.paint;
                }
                view.bar.paint(&mut materials, &look.bar_fill);
                view.bar.place(&mut part_query, bar_y, Some(look.bar));
            }
            None => {
                let bar = parts::spawn_bar(
                    &mut commands,
                    &palette,
                    &look.bar_fill,
                    bar_y,
                    Some(look.bar),
                );
                let entity = commands
                    .spawn((
                        Mesh2d(look.block),
                        MeshMaterial2d(look.paint),
                        Transform::from_translation(convert::placement(
                            tower.tile.to_tile().center(),
                        )),
                        TowerView { bar },
                    ))
                    .add_children(&bar.entities())
                    .id();
                registry.0.insert(tower.id, entity);
            }
        }
    }
    registry.0.despawn_unseen(&mut commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_client_core::Frame;
    use caravan_protocol::{WirePhase, WireTile, WireWagon};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .init_resource::<CurrentFrame>()
            .init_resource::<TowerRegistry>()
            .add_systems(Startup, crate::palette::setup_palette)
            .add_systems(Update, sync_towers);
        app
    }

    fn tower(id: u32, x: u8, y: u8, state: WireTowerState) -> WireTower {
        WireTower {
            id,
            tile: WireTile { x, y },
            max_hp: 300,
            state,
        }
    }

    fn standing(hp: u32) -> WireTowerState {
        WireTowerState::Standing {
            hp,
            last_shot: None,
        }
    }

    fn show(app: &mut App, towers: Vec<WireTower>) {
        app.world_mut().resource_mut::<CurrentFrame>().0 = Some(Frame {
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
            towers,
            nodes: Vec::new(),
            piles: Vec::new(),
            players: Vec::new(),
            monsters: Vec::new(),
        });
        app.update();
    }

    fn entity_of(app: &App, id: u32) -> Entity {
        app.world()
            .resource::<TowerRegistry>()
            .0
            .entity_of(id)
            .expect("the tower has an entity")
    }

    fn look(app: &App, id: u32) -> (Handle<Mesh>, Handle<ColorMaterial>) {
        let entity = entity_of(app, id);
        let mesh = app.world().get::<Mesh2d>(entity).unwrap();
        let material = app
            .world()
            .get::<MeshMaterial2d<ColorMaterial>>(entity)
            .unwrap();
        (mesh.0.clone(), material.0.clone())
    }

    fn count(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query::<&TowerView>().iter(world).count()
    }

    #[test]
    fn a_tower_appears_with_the_snapshot_and_is_kept_while_it_stands() {
        let mut app = app();
        show(&mut app, vec![tower(1, 15, 11, standing(300))]);
        assert_eq!(count(&mut app), 1);
        let kept = entity_of(&app, 1);

        show(&mut app, vec![tower(1, 15, 11, standing(120))]);
        assert_eq!(count(&mut app), 1);
        assert_eq!(entity_of(&app, 1), kept, "the same entity is updated");
    }

    #[test]
    fn a_tower_is_placed_on_its_tile() {
        let mut app = app();
        show(&mut app, vec![tower(1, 15, 11, standing(300))]);

        let transform = app.world().get::<Transform>(entity_of(&app, 1)).unwrap();
        let expected = convert::placement(caravan_rules::Tile::new(15, 11).center());
        assert_eq!(transform.translation, expected);
    }

    #[test]
    fn a_site_looks_different_from_a_tower_and_becomes_one_when_finished() {
        let mut app = app();
        show(
            &mut app,
            vec![tower(1, 15, 11, WireTowerState::Building { progress: 0.5 })],
        );
        let site = look(&app, 1);
        let palette = app.world().resource::<Palette>();
        assert_eq!(site.0, palette.site_block);
        assert_eq!(site.1, palette.site);

        show(&mut app, vec![tower(1, 15, 11, standing(300))]);
        let finished = look(&app, 1);
        let palette = app.world().resource::<Palette>();
        assert_eq!(finished.0, palette.tower_block);
        assert_eq!(finished.1, palette.tower);
        assert_ne!(site.0, finished.0);
    }

    #[test]
    fn a_tower_that_is_gone_from_the_snapshot_disappears() {
        let mut app = app();
        show(
            &mut app,
            vec![
                tower(1, 15, 11, standing(300)),
                tower(2, 16, 11, WireTowerState::Building { progress: 0.2 }),
            ],
        );
        assert_eq!(count(&mut app), 2);

        show(
            &mut app,
            vec![tower(2, 16, 11, WireTowerState::Building { progress: 0.4 })],
        );

        assert_eq!(count(&mut app), 1);
        assert!(
            app.world()
                .resource::<TowerRegistry>()
                .0
                .entity_of(1)
                .is_none()
        );
    }

    #[test]
    fn the_bar_of_a_tower_whose_max_hp_is_zero_is_empty_and_has_a_valid_position() {
        let mut app = app();
        let mut broken = tower(1, 15, 11, standing(0));
        broken.max_hp = 0;
        show(&mut app, vec![broken]);

        let view = app.world().get::<TowerView>(entity_of(&app, 1)).unwrap();
        let [_, fill] = view.bar.entities();
        let transform = app.world().get::<Transform>(fill).unwrap();
        assert!(transform.translation.is_finite() && transform.scale.is_finite());
    }
}
