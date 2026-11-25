//! 무기 상자, 바닥에 놓인 무기, 야영지의 무기 진열대를 화면의 도형으로 만들고 갱신합니다.
//!
//! 스냅샷의 번호 목록과 Bevy 개체를 맞춥니다(감시탑, 채집 지점과 같은 방식입니다).
//! 상자는 타일 하나 위에 서 있는 황금색 상자입니다. 움직이지 않고 열면 사라지므로 만들 때 한 번만 놓습니다.
//! 상자 안의 무기는 스냅샷에 없으므로 화면에도 나타나지 않습니다.
//! 바닥의 무기는 무기마다 색이 다른 납작한 원이고 위에 이름이 떠 있습니다. 다른 플레이어가 줍고 맞바꾸면 번호와 자리는 그대로이고 무기만 바뀌므로,
//! 개체를 새로 만들지 않고 색과 이름을 바꿉니다.
//! 진열대는 무기마다 색이 다른 상자이고 위에 무기의 이름이 떠 있습니다. 위치와 무기는 입장 안내에 있고, 야영지의 스냅샷일 때만 보입니다.
//! 집어도 진열은 그대로이므로 개체를 만들 때 한 번만 놓습니다.

use bevy::prelude::*;
use caravan_client_core::weapon_name;
use caravan_protocol::{WirePhase, WireWeaponKind};

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::palette::Palette;
use crate::registry::Registry;
use crate::view::Scene;

#[derive(Resource, Default)]
pub struct LootRegistry {
    chests: Registry,
    weapons: Registry,
    racks: Registry,
}

/// 무기 상자의 개체입니다.
#[derive(Component)]
pub struct ChestView;

/// 야영지의 무기 진열대의 개체입니다.
#[derive(Component)]
pub struct RackView;

/// 바닥에 놓인 무기의 개체입니다. 지금 어떤 무기로 그리고 있는지와, 색을 가진 원과 이름을 띄운 글자를 기억합니다.
/// 원은 납작하게 늘려서 그리므로 글자와 같은 개체에 둘 수 없어서, 둘 다 이 개체의 자식입니다.
#[derive(Component)]
pub struct GroundWeaponView {
    kind: WireWeaponKind,
    disc: Entity,
    label: Entity,
}

/// 스냅샷의 무기 상자에 맞게 상자를 만들고 지웁니다. 낮이 끝나거나 상자를 열면 스냅샷에서 사라져서 지워집니다.
pub fn sync_chests(
    frame: Res<CurrentFrame>,
    palette: Res<Palette>,
    mut loot: ResMut<LootRegistry>,
    mut commands: Commands,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    loot.chests.begin_frame();
    for chest in &frame.chests {
        if loot.chests.find(chest.id).is_some() {
            continue;
        }
        let entity = commands
            .spawn((
                Mesh2d(palette.chest_block.clone()),
                MeshMaterial2d(palette.chest.clone()),
                Transform::from_translation(convert::placement(chest.tile.to_tile().center())),
                ChestView,
            ))
            .id();
        loot.chests.insert(chest.id, entity);
    }
    loot.chests.despawn_unseen(&mut commands);
}

/// `index`번째 진열대의 이름이 놓이는 높이입니다. 이웃한 진열대의 이름이 번갈아 위와 아래에 놓여서 서로 겹쳐 보이지 않습니다.
fn rack_label_lift(index: usize) -> f32 {
    if index.is_multiple_of(2) {
        config::RACK_LABEL_LIFT
    } else {
        config::RACK_LABEL_BELOW
    }
}

/// 스냅샷이 야영지일 때 입장 안내의 진열대마다 개체를 만들고, 야영지가 아니면 모두 지웁니다.
/// 진열대의 번호는 입장 안내의 순서(1부터)입니다.
pub fn sync_racks(
    frame: Res<CurrentFrame>,
    scene: Res<Scene>,
    palette: Res<Palette>,
    mut loot: ResMut<LootRegistry>,
    mut commands: Commands,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    loot.racks.begin_frame();
    if matches!(frame.phase, WirePhase::Camp { .. }) {
        for (index, rack) in scene.racks.iter().enumerate() {
            let id = index as u32 + 1;
            if loot.racks.find(id).is_some() {
                continue;
            }
            let label = commands
                .spawn((
                    Text2d::new(weapon_name(rack.kind)),
                    TextFont {
                        font_size: FontSize::Px(config::GROUND_WEAPON_LABEL_SIZE),
                        ..default()
                    },
                    TextColor(config::GROUND_WEAPON_LABEL),
                    Transform::from_xyz(0.0, rack_label_lift(index), config::RACK_LABEL_Z_LIFT),
                ))
                .id();
            let entity = commands
                .spawn((
                    Mesh2d(palette.rack_block.clone()),
                    MeshMaterial2d(palette.weapons.of(rack.kind)),
                    Transform::from_translation(convert::placement(rack.pos.to_tiles())),
                    RackView,
                ))
                .add_child(label)
                .id();
            loot.racks.insert(id, entity);
        }
    }
    loot.racks.despawn_unseen(&mut commands);
}

/// 스냅샷의 바닥에 놓인 무기에 맞게 원을 만들고 지우고, 무기가 바뀌었으면 색과 이름을 바꿉니다.
pub fn sync_ground_weapons(
    frame: Res<CurrentFrame>,
    palette: Res<Palette>,
    mut loot: ResMut<LootRegistry>,
    mut commands: Commands,
    mut views: Query<&mut GroundWeaponView>,
    mut discs: Query<&mut MeshMaterial2d<ColorMaterial>>,
    mut labels: Query<&mut Text2d>,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    loot.weapons.begin_frame();
    for weapon in &frame.ground_weapons {
        match loot.weapons.find(weapon.id) {
            Some(entity) => {
                let Ok(mut view) = views.get_mut(entity) else {
                    continue;
                };
                if view.kind == weapon.kind {
                    continue;
                }
                view.kind = weapon.kind;
                if let Ok(mut material) = discs.get_mut(view.disc) {
                    material.0 = palette.weapons.of(weapon.kind);
                }
                if let Ok(mut text) = labels.get_mut(view.label) {
                    text.0 = weapon_name(weapon.kind).to_string();
                }
            }
            None => {
                let disc = commands
                    .spawn((
                        Mesh2d(palette.circle.clone()),
                        MeshMaterial2d(palette.weapons.of(weapon.kind)),
                        Transform::from_scale(config::GROUND_WEAPON_RADII.extend(1.0)),
                    ))
                    .id();
                let label = commands
                    .spawn((
                        Text2d::new(weapon_name(weapon.kind)),
                        TextFont {
                            font_size: FontSize::Px(config::GROUND_WEAPON_LABEL_SIZE),
                            ..default()
                        },
                        TextColor(config::GROUND_WEAPON_LABEL),
                        Transform::from_xyz(
                            0.0,
                            config::GROUND_WEAPON_LABEL_LIFT,
                            config::PART_Z_STEP,
                        ),
                    ))
                    .id();
                let entity = commands
                    .spawn((
                        Transform::from_translation(convert::placement(weapon.pos.to_tiles())),
                        Visibility::default(),
                        GroundWeaponView {
                            kind: weapon.kind,
                            disc,
                            label,
                        },
                    ))
                    .add_children(&[disc, label])
                    .id();
                loot.weapons.insert(weapon.id, entity);
            }
        }
    }
    loot.weapons.despawn_unseen(&mut commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::joined_session;
    use crate::view::{self, Scene};
    use caravan_client_core::Frame;
    use caravan_protocol::{WireChest, WireGroundWeapon, WirePhase, WirePos, WireTile, WireWagon};
    use caravan_rules::Vec2 as Ground;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .insert_resource(joined_session())
            .init_resource::<CurrentFrame>()
            .init_resource::<LootRegistry>()
            .add_systems(Startup, crate::palette::setup_palette)
            .add_systems(
                Update,
                (
                    view::spawn_world.run_if(not(resource_exists::<Scene>)),
                    sync_chests.run_if(resource_exists::<Scene>),
                    sync_ground_weapons.run_if(resource_exists::<Scene>),
                    sync_racks.run_if(resource_exists::<Scene>),
                )
                    .chain(),
            );
        app
    }

    fn show(app: &mut App, chests: Vec<WireChest>, ground_weapons: Vec<WireGroundWeapon>) {
        let day = WirePhase::Day {
            day: 1,
            secs_left: 90.0,
        };
        show_in(app, day, chests, ground_weapons);
    }

    fn show_in(
        app: &mut App,
        phase: WirePhase,
        chests: Vec<WireChest>,
        ground_weapons: Vec<WireGroundWeapon>,
    ) {
        app.world_mut().resource_mut::<CurrentFrame>().0 = Some(Frame {
            tick: 1.0,
            expedition: 1,
            phase,
            wagon: WireWagon {
                hp: 500,
                max_hp: 500,
            },
            stash: 0,
            towers: Vec::new(),
            nodes: Vec::new(),
            piles: Vec::new(),
            chests,
            ground_weapons,
            players: Vec::new(),
            monsters: Vec::new(),
            projectiles: Vec::new(),
        });
        app.update();
    }

    fn lying(id: u32, kind: WireWeaponKind) -> WireGroundWeapon {
        WireGroundWeapon {
            id,
            pos: WirePos::from_tiles(Ground::new(20.5, 20.5)),
            kind,
        }
    }

    fn count<T: Component>(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query::<&T>().iter(world).count()
    }

    #[test]
    fn a_weapon_on_the_ground_shows_its_name_and_its_own_color() {
        let mut app = app();
        show(
            &mut app,
            Vec::new(),
            vec![
                lying(1, WireWeaponKind::Spear),
                lying(2, WireWeaponKind::Greatsword),
            ],
        );

        assert_eq!(count::<GroundWeaponView>(&mut app), 2);
        let palette = app.world().resource::<Palette>();
        assert_ne!(
            palette.weapons.of(WireWeaponKind::Spear),
            palette.weapons.of(WireWeaponKind::Greatsword)
        );
        let world = app.world_mut();
        let mut names: Vec<String> = world
            .query::<&Text2d>()
            .iter(world)
            .map(|text| text.0.clone())
            .collect();
        names.sort();
        assert_eq!(names, ["GREATSWORD", "SPEAR"]);
    }

    #[test]
    fn a_swap_changes_the_color_and_the_name_and_keeps_the_entity() {
        let mut app = app();
        show(&mut app, Vec::new(), vec![lying(1, WireWeaponKind::Spear)]);
        let kept = app
            .world()
            .resource::<LootRegistry>()
            .weapons
            .entity_of(1)
            .unwrap();

        show(&mut app, Vec::new(), vec![lying(1, WireWeaponKind::Bow)]);

        assert_eq!(
            app.world().resource::<LootRegistry>().weapons.entity_of(1),
            Some(kept)
        );
        let view = app.world().get::<GroundWeaponView>(kept).unwrap();
        assert_eq!(view.kind, WireWeaponKind::Bow);
        let (disc, label) = (view.disc, view.label);
        let bow_color = app
            .world()
            .resource::<Palette>()
            .weapons
            .of(WireWeaponKind::Bow);
        assert_eq!(
            app.world()
                .get::<MeshMaterial2d<ColorMaterial>>(disc)
                .unwrap()
                .0,
            bow_color
        );
        assert_eq!(app.world().get::<Text2d>(label).unwrap().0, "BOW");
    }

    #[test]
    fn a_weapon_that_is_picked_up_disappears_with_its_name() {
        let mut app = app();
        show(&mut app, Vec::new(), vec![lying(1, WireWeaponKind::Spear)]);
        assert_eq!(count::<Text2d>(&mut app), 1);

        show(&mut app, Vec::new(), Vec::new());

        assert_eq!(count::<GroundWeaponView>(&mut app), 0);
        assert_eq!(count::<Text2d>(&mut app), 0);
    }

    fn chest(id: u32, x: u8, y: u8) -> WireChest {
        WireChest {
            id,
            tile: WireTile { x, y },
        }
    }

    #[test]
    fn a_chest_stands_on_its_tile_and_is_removed_when_the_snapshot_no_longer_has_it() {
        let mut app = app();

        show(
            &mut app,
            vec![chest(1, 47, 32), chest(2, 17, 32)],
            Vec::new(),
        );

        assert_eq!(count::<ChestView>(&mut app), 2);
        let world = app.world_mut();
        let mut query = world.query_filtered::<&Transform, With<ChestView>>();
        let mut spots: Vec<Vec2> = query
            .iter(world)
            .map(|transform| transform.translation.truncate())
            .collect();
        spots.sort_by(|a, b| a.x.total_cmp(&b.x));
        let expected =
            |x: u8, y: u8| convert::placement(WireTile { x, y }.to_tile().center()).truncate();
        assert!(spots[0].distance(expected(17, 32)) < 1e-3);
        assert!(spots[1].distance(expected(47, 32)) < 1e-3);

        show(&mut app, vec![chest(2, 17, 32)], Vec::new());
        assert_eq!(count::<ChestView>(&mut app), 1, "one chest was opened");

        show(&mut app, Vec::new(), Vec::new());
        assert_eq!(count::<ChestView>(&mut app), 0, "night fell");
    }

    fn camp() -> WirePhase {
        WirePhase::Camp {
            ready: vec![],
            starting_in: None,
            struck: vec![],
        }
    }

    #[test]
    fn the_camp_has_a_rack_for_each_weapon_at_the_spot_the_welcome_gave_and_the_expedition_has_none()
     {
        let mut app = app();

        show_in(&mut app, camp(), Vec::new(), Vec::new());

        assert_eq!(
            count::<RackView>(&mut app),
            caravan_rules::WeaponKind::ALL.len()
        );
        let world = app.world_mut();
        let mut names: Vec<String> = world
            .query::<&Text2d>()
            .iter(world)
            .map(|text| text.0.clone())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["BOW", "CROSSBOW", "GREATSWORD", "RIFLE", "SPEAR", "SWORD"]
        );
        let expected: Vec<Vec2> = app
            .world()
            .resource::<Scene>()
            .racks
            .iter()
            .map(|rack| convert::placement(rack.pos.to_tiles()).truncate())
            .collect();
        let world = app.world_mut();
        let mut query = world.query_filtered::<&Transform, With<RackView>>();
        let spots: Vec<Vec2> = query
            .iter(world)
            .map(|transform| transform.translation.truncate())
            .collect();
        for spot in &expected {
            assert!(
                spots.iter().any(|found| found.distance(*spot) < 1e-3),
                "no rack at {spot:?} in {spots:?}"
            );
        }

        let world = app.world_mut();
        let mut labels =
            world.query_filtered::<(&Text2d, &Transform, &ChildOf), Without<RackView>>();
        let mut rises: Vec<(String, f32)> = labels
            .iter(world)
            .map(|(text, transform, _)| (text.0.clone(), transform.translation.y))
            .collect();
        rises.sort_by(|a, b| a.0.cmp(&b.0));
        let (above, below) = (config::RACK_LABEL_LIFT, config::RACK_LABEL_BELOW);
        let lifts: Vec<(&str, f32)> = rises.iter().map(|(name, y)| (name.as_str(), *y)).collect();
        assert_eq!(
            lifts,
            [
                ("BOW", below),
                ("CROSSBOW", above),
                ("GREATSWORD", above),
                ("RIFLE", below),
                ("SPEAR", below),
                ("SWORD", above),
            ],
            "neighbouring names alternate above and below the racks"
        );

        show(&mut app, Vec::new(), Vec::new());
        assert_eq!(count::<RackView>(&mut app), 0, "the expedition started");
        assert_eq!(count::<Text2d>(&mut app), 0, "and the names went with them");
    }

    #[test]
    fn a_rack_is_made_once_and_kept_for_as_long_as_the_camp_goes_on() {
        let mut app = app();
        show_in(&mut app, camp(), Vec::new(), Vec::new());
        let kept = app
            .world()
            .resource::<LootRegistry>()
            .racks
            .entity_of(1)
            .unwrap();

        show_in(&mut app, camp(), Vec::new(), Vec::new());

        assert_eq!(
            app.world().resource::<LootRegistry>().racks.entity_of(1),
            Some(kept)
        );
        assert_eq!(
            count::<RackView>(&mut app),
            caravan_rules::WeaponKind::ALL.len()
        );
    }

    #[test]
    fn a_chest_shows_no_name_because_the_snapshot_does_not_say_what_is_in_it() {
        let mut app = app();

        show(&mut app, vec![chest(1, 47, 32)], Vec::new());

        assert_eq!(count::<Text2d>(&mut app), 0);
    }
}
