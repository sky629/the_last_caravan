//! 바닥에 놓인 무기를 화면의 도형으로 만들고 갱신합니다.
//!
//! 스냅샷의 번호 목록과 Bevy 개체를 맞춥니다(감시탑, 채집 지점과 같은 방식입니다).
//! 바닥의 무기는 무기마다 색이 다른 납작한 원이고 위에 이름이 떠 있습니다. 다른 플레이어가 줍고 맞바꾸면 번호와 자리는 그대로이고 무기만 바뀌므로,
//! 개체를 새로 만들지 않고 색과 이름을 바꿉니다.

use bevy::prelude::*;
use caravan_client_core::weapon_name;
use caravan_protocol::WireWeaponKind;

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::palette::Palette;
use crate::registry::Registry;

#[derive(Resource, Default)]
pub struct LootRegistry {
    weapons: Registry,
}

/// 바닥에 놓인 무기의 개체입니다. 지금 어떤 무기로 그리고 있는지와, 색을 가진 원과 이름을 띄운 글자를 기억합니다.
/// 원은 납작하게 늘려서 그리므로 글자와 같은 개체에 둘 수 없어서, 둘 다 이 개체의 자식입니다.
#[derive(Component)]
pub struct GroundWeaponView {
    kind: WireWeaponKind,
    disc: Entity,
    label: Entity,
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
    use caravan_protocol::{WireGroundWeapon, WirePhase, WirePos, WireWagon};
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
                    sync_ground_weapons.run_if(resource_exists::<Scene>),
                )
                    .chain(),
            );
        app
    }

    fn show(app: &mut App, ground_weapons: Vec<WireGroundWeapon>) {
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
            nodes: Vec::new(),
            piles: Vec::new(),
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
        show(&mut app, vec![lying(1, WireWeaponKind::Spear)]);
        let kept = app
            .world()
            .resource::<LootRegistry>()
            .weapons
            .entity_of(1)
            .unwrap();

        show(&mut app, vec![lying(1, WireWeaponKind::Bow)]);

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
        show(&mut app, vec![lying(1, WireWeaponKind::Spear)]);
        assert_eq!(count::<Text2d>(&mut app), 1);

        show(&mut app, Vec::new());

        assert_eq!(count::<GroundWeaponView>(&mut app), 0);
        assert_eq!(count::<Text2d>(&mut app), 0);
    }
}
