//! 채집 지점과 땅에 떨어진 자원을 화면의 도형으로 만들고 갱신합니다.
//!
//! 둘 다 스냅샷의 번호 목록과 Bevy 개체를 맞춥니다(감시탑, 캐릭터와 같은 방식입니다).
//! 채집 지점은 상자이고 위의 막대가 남은 자원을 보여 주며, 다 떨어지면 어두운 색이 됩니다.
//! 지점의 위치는 입장 안내에 있고 스냅샷에는 남은 양만 있으므로, 번호로 두 곳을 이어서 그립니다.
//! 떨어진 자원은 바닥에 놓인 납작한 원입니다.

use bevy::prelude::*;
use caravan_client_core::bar_fraction;
use caravan_protocol::WireNode;

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::palette::Palette;
use crate::parts::{self, Bar, PartQuery};
use crate::registry::Registry;
use crate::view::Scene;

#[derive(Resource, Default)]
pub struct SupplyRegistry {
    nodes: Registry,
    piles: Registry,
}

#[derive(Component)]
pub struct NodeView {
    bar: Bar,
}

/// 채집 지점의 상자 색입니다. 자원이 남아 있는 동안만 밝습니다.
fn node_paint(node: &WireNode, palette: &Palette) -> Handle<ColorMaterial> {
    if node.amount > 0 {
        palette.node.clone()
    } else {
        palette.node_empty.clone()
    }
}

/// 스냅샷의 채집 지점에 맞게 개체를 만들고 맞춥니다. 채집 지점은 사라지지 않으므로 지우는 일은 없습니다.
pub fn sync_nodes(
    frame: Res<CurrentFrame>,
    scene: Res<Scene>,
    palette: Res<Palette>,
    mut supplies: ResMut<SupplyRegistry>,
    mut commands: Commands,
    views: Query<&NodeView>,
    mut materials: Query<&mut MeshMaterial2d<ColorMaterial>>,
    mut part_query: PartQuery,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    supplies.nodes.begin_frame();
    for node in &frame.nodes {
        let Some(site) = scene.nodes.iter().find(|site| site.id == node.id) else {
            continue;
        };
        let bar = bar_fraction(node.amount, scene.node_amount);
        let bar_y = config::NODE_HEIGHT + config::BAR_GAP;
        match supplies.nodes.find(node.id) {
            Some(entity) => {
                let Ok(view) = views.get(entity) else {
                    continue;
                };
                let paint = node_paint(node, &palette);
                if let Ok(mut material) = materials.get_mut(entity)
                    && material.0 != paint
                {
                    material.0 = paint;
                }
                view.bar.place(&mut part_query, bar_y, Some(bar));
            }
            None => {
                let view_bar =
                    parts::spawn_bar(&mut commands, &palette, &palette.bar_node, bar_y, Some(bar));
                let entity = commands
                    .spawn((
                        Mesh2d(palette.node_block.clone()),
                        MeshMaterial2d(node_paint(node, &palette)),
                        Transform::from_translation(convert::placement(
                            site.tile.to_tile().center(),
                        )),
                        NodeView { bar: view_bar },
                    ))
                    .add_children(&view_bar.entities())
                    .id();
                supplies.nodes.insert(node.id, entity);
            }
        }
    }
    supplies.nodes.despawn_unseen(&mut commands);
}

/// 스냅샷의 떨어진 자원에 맞게 바닥의 원을 만들고 지웁니다. 더미는 움직이지 않으므로 만들 때 한 번만 놓습니다.
pub fn sync_piles(
    frame: Res<CurrentFrame>,
    palette: Res<Palette>,
    mut supplies: ResMut<SupplyRegistry>,
    mut commands: Commands,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    supplies.piles.begin_frame();
    for pile in &frame.piles {
        if supplies.piles.find(pile.id).is_some() {
            continue;
        }
        let entity = commands
            .spawn((
                Mesh2d(palette.circle.clone()),
                MeshMaterial2d(palette.pile.clone()),
                Transform::from_translation(convert::placement(pile.pos.to_tiles()))
                    .with_scale(config::PILE_RADII.extend(1.0)),
            ))
            .id();
        supplies.piles.insert(pile.id, entity);
    }
    supplies.piles.despawn_unseen(&mut commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::joined_session;
    use crate::view;
    use caravan_client_core::Frame;
    use caravan_protocol::{WirePhase, WirePile, WirePos, WireWagon};
    use caravan_rules::{Tuning, Vec2 as Ground};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .insert_resource(joined_session())
            .init_resource::<CurrentFrame>()
            .init_resource::<SupplyRegistry>()
            .add_systems(Startup, crate::palette::setup_palette)
            .add_systems(
                Update,
                (
                    view::spawn_world.run_if(not(resource_exists::<Scene>)),
                    (sync_nodes, sync_piles).run_if(resource_exists::<Scene>),
                )
                    .chain(),
            );
        app
    }

    fn show(app: &mut App, nodes: Vec<WireNode>, piles: Vec<WirePile>) {
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
            towers: Vec::new(),
            nodes,
            piles,
            chests: Vec::new(),
            ground_weapons: Vec::new(),
            players: Vec::new(),
            monsters: Vec::new(),
            projectiles: Vec::new(),
        });
        app.update();
    }

    fn full_nodes() -> Vec<WireNode> {
        (1..=8).map(|id| WireNode { id, amount: 15 }).collect()
    }

    fn pile(id: u32, x: f32, y: f32) -> WirePile {
        WirePile {
            id,
            pos: WirePos::from_tiles(Ground::new(x, y)),
            amount: 5,
        }
    }

    fn count<T: Component>(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query::<&T>().iter(world).count()
    }

    fn node_entity(app: &App, id: u32) -> Entity {
        app.world()
            .resource::<SupplyRegistry>()
            .nodes
            .entity_of(id)
            .expect("the node has an entity")
    }

    #[test]
    fn every_node_gets_a_box_on_its_tile_and_keeps_the_same_entity() {
        let mut app = app();
        show(&mut app, full_nodes(), Vec::new());
        assert_eq!(count::<NodeView>(&mut app), 8);
        let kept = node_entity(&app, 3);

        show(&mut app, full_nodes(), Vec::new());

        assert_eq!(count::<NodeView>(&mut app), 8);
        assert_eq!(node_entity(&app, 3), kept);
        let tile = Tuning::default().map.nodes[2];
        let transform = app.world().get::<Transform>(kept).unwrap();
        assert_eq!(transform.translation, convert::placement(tile.center()));
    }

    #[test]
    fn a_node_that_ran_dry_looks_different() {
        let mut app = app();
        show(&mut app, full_nodes(), Vec::new());
        let paint = |app: &App| {
            app.world()
                .get::<MeshMaterial2d<ColorMaterial>>(node_entity(app, 1))
                .unwrap()
                .0
                .clone()
        };
        let full = paint(&app);
        assert_eq!(full, app.world().resource::<Palette>().node);

        let mut nodes = full_nodes();
        nodes[0].amount = 0;
        show(&mut app, nodes, Vec::new());

        assert_eq!(paint(&app), app.world().resource::<Palette>().node_empty);
        assert_ne!(paint(&app), full);
    }

    #[test]
    fn a_node_the_welcome_does_not_place_is_not_drawn() {
        let mut app = app();
        show(&mut app, vec![WireNode { id: 99, amount: 15 }], Vec::new());

        assert_eq!(count::<NodeView>(&mut app), 0);
    }

    #[test]
    fn a_pile_appears_once_and_disappears_when_it_is_picked_up() {
        let mut app = app();
        let piles = |app: &mut App| {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, (With<Mesh2d>, Without<NodeView>, Without<ChildOf>)>()
                .iter(world)
                .count()
        };
        show(&mut app, full_nodes(), Vec::new());
        let without = piles(&mut app);

        show(&mut app, full_nodes(), vec![pile(1, 10.5, 10.5)]);
        assert_eq!(piles(&mut app), without + 1, "a pile appeared");
        let kept = app.world().resource::<SupplyRegistry>().piles.entity_of(1);

        show(&mut app, full_nodes(), vec![pile(1, 10.5, 10.5)]);
        assert_eq!(piles(&mut app), without + 1);
        assert_eq!(
            app.world().resource::<SupplyRegistry>().piles.entity_of(1),
            kept,
            "the same entity is kept"
        );

        show(&mut app, full_nodes(), Vec::new());
        assert_eq!(piles(&mut app), without, "the pile is gone");
    }
}
