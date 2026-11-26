//! 고정된 세계(지면, 장애물, 마차)를 한 번 만듭니다. 감시탑은 건설과 파괴로 바뀌므로 `towers`가, 채집 지점과 떨어진 자원은 `supplies`가 프레임마다 맞추고, 카메라는 `camera`가 맡습니다.
//!
//! 세계의 모양은 입장할 때 받은 `Welcome`에서 오므로, 서버가 다른 맵으로 실행되어도 클라이언트를 고치지 않습니다.
//! `Session`이 입장할 때 맵의 크기와 장애물의 수에 상한을 확인했으므로, 여기서는 받은 값만큼 개체를 만들어도 됩니다.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use caravan_client_core as core;
use caravan_protocol::{
    MonsterKindInfo, NodeInfo, RackInfo, WeaponInfo, WireDelivery, WireMonsterKind, WireWeaponKind,
};
use caravan_rules::{Team, Vec2 as Ground};

use crate::camera::{FollowedSetting, WagonMarkerNode};
use crate::camp::StrikeRegistry;
use crate::characters::Spawned;
use crate::config;
use crate::convert;
use crate::loot::LootRegistry;
use crate::net::{ClientFlow, CurrentFrame};
use crate::palette::Palette;
use crate::registry::RoomEntity;
use crate::supplies::SupplyRegistry;
use crate::towers::TowerRegistry;

/// 입장한 뒤에 만든 세계에서 캐릭터를 그릴 때 필요한 값입니다. 세계를 만들면 한 번 들어갑니다.
#[derive(Resource)]
pub struct Scene {
    pub you: u32,
    /// 내 진영입니다. 같은 진영의 플레이어만 같은 편으로 그립니다.
    pub team: Team,
    pub player: Footprint,
    /// 마차의 중심입니다. 마차가 화면 밖에 있을 때 방향을 알려 주는 표시가 이 위치를 향합니다.
    pub wagon_center: Ground,
    /// 맵의 가로와 세로 타일 수입니다. 감시탑이 놓일 타일을 미리 보여 줄 때 맵 밖인지 가립니다.
    pub map_size: (u16, u16),
    /// 채집 지점의 위치입니다. 스냅샷에는 남은 양만 있습니다.
    pub nodes: Vec<NodeInfo>,
    /// 채집 지점이 가득 찼을 때의 양입니다. 지점의 막대를 그리는 기준입니다.
    pub node_amount: u32,
    /// 야영지의 무기 진열대입니다. 진열대는 야영지의 스냅샷일 때만 그립니다.
    pub racks: Vec<RackInfo>,
    /// 무기마다 공격이 전달되는 방식입니다. 근접 효과를 그리는 거리와 각도가 여기에서 옵니다.
    weapons: Vec<WeaponInfo>,
    monsters: Vec<MonsterLook>,
}

/// 캐릭터가 지면에 닿는 곳입니다. 규칙의 충돌 원을 지면에 그린 타원이 그림자가 됩니다.
pub struct Footprint {
    pub radius: f32,
    pub shadow: Handle<Mesh>,
}

pub struct MonsterLook {
    pub kind: WireMonsterKind,
    pub max_hp: u32,
    pub footprint: Footprint,
}

impl Scene {
    pub fn monster(&self, kind: WireMonsterKind) -> Option<&MonsterLook> {
        self.monsters.iter().find(|look| look.kind == kind)
    }

    /// `weapon`의 공격이 전달되는 방식입니다. 입장 안내에 없는 무기이면 없습니다.
    pub fn delivery(&self, weapon: WireWeaponKind) -> Option<WireDelivery> {
        self.weapons
            .iter()
            .find(|info| info.kind == weapon)
            .map(|info| info.delivery)
    }
}

fn footprint(meshes: &mut Assets<Mesh>, radius: f32) -> Footprint {
    let radii = core::circle_to_screen_radii(radius);
    Footprint {
        radius,
        shadow: meshes.add(Ellipse::new(radii.x, radii.y)),
    }
}

fn spawn_block(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    material: &Handle<ColorMaterial>,
    center: Ground,
) -> Entity {
    commands
        .spawn((
            Mesh2d(mesh.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_translation(convert::placement(center)),
            RoomEntity,
        ))
        .id()
}

/// `cols` x `rows` 타일의 지면을 메시 하나로 만듭니다. 타일마다 마름모 하나이고, 꼭짓점 색으로 두 색을 바둑판 모양으로 칠합니다.
/// 메시의 좌표는 화면 좌표(픽셀)이고 지면의 (0, 0)이 원점입니다. 재질이 흰색이므로 꼭짓점 색이 그대로 보입니다.
fn ground_mesh(cols: u32, rows: u32) -> Mesh {
    let half_w = core::TILE_WIDTH_PX / 2.0;
    let half_h = core::TILE_HEIGHT_PX / 2.0;
    let light = LinearRgba::from(config::GROUND_A).to_f32_array();
    let dark = LinearRgba::from(config::GROUND_B).to_f32_array();

    let tiles = (cols * rows) as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(tiles * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(tiles * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(tiles * 6);
    for y in 0..rows {
        for x in 0..cols {
            let center = convert::screen_pos(Ground::new(x as f32 + 0.5, y as f32 + 0.5));
            // 왼쪽, 아래, 오른쪽, 위 꼭짓점의 순서입니다.
            let first = positions.len() as u32;
            positions.extend([
                [center.x - half_w, center.y, 0.0],
                [center.x, center.y - half_h, 0.0],
                [center.x + half_w, center.y, 0.0],
                [center.x, center.y + half_h, 0.0],
            ]);
            colors.extend([if (x + y) % 2 == 0 { light } else { dark }; 4]);
            indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// 입장한 직후 한 번만 세계를 만듭니다.
///
/// 지면은 타일마다 색을 달리 칠한 메시 하나입니다. 큰 상자(긴 벽, 마차)는 타일마다 상자 하나로 나눕니다. 상자마다 자기 중심으로 그리는 순서를 정해야,
/// 캐릭터가 긴 벽의 한쪽 끝 옆에 서도 벽의 앞과 뒤가 맞게 가려집니다.
pub fn spawn_world(
    mut commands: Commands,
    flow: Res<ClientFlow>,
    palette: Res<Palette>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(joined) = flow.joined() else {
        return;
    };
    let welcome = &joined.welcome;
    let world = &welcome.world;
    let map_size = world.size.to_tiles();

    commands.spawn((
        Mesh2d(meshes.add(ground_mesh(map_size.x as u32, map_size.y as u32))),
        MeshMaterial2d(palette.ground.clone()),
        Transform::from_xyz(0.0, 0.0, config::GROUND_Z),
        RoomEntity,
    ));
    for obstacle in &world.obstacles {
        for center in core::tile_centers(obstacle.to_rect()) {
            spawn_block(
                &mut commands,
                &palette.obstacle_block,
                &palette.obstacle,
                center,
            );
        }
    }
    for center in core::tile_centers(world.wagon.to_rect()) {
        spawn_block(&mut commands, &palette.wagon_block, &palette.wagon, center);
    }
    let monsters = world
        .monster_kinds
        .iter()
        .map(|info: &MonsterKindInfo| MonsterLook {
            kind: info.kind,
            max_hp: info.max_hp,
            footprint: footprint(&mut meshes, info.radius),
        })
        .collect();
    commands.insert_resource(Scene {
        you: welcome.you,
        team: Team(welcome.team),
        player: footprint(&mut meshes, world.player_radius),
        wagon_center: world.wagon.to_rect().center(),
        map_size: (map_size.x as u16, map_size.y as u16),
        nodes: world.nodes.clone(),
        node_amount: world.node_amount,
        racks: world.racks.clone(),
        weapons: world.weapons.clone(),
        monsters,
    });
}

/// 방을 떠났을 때 방의 세계를 모두 지웁니다. 지형과 번호로 추적하는 개체를 지우고, 추적하는 표를 비우고, 그릴 모습과 `Scene`을 없애고,
/// 화면 가장자리의 마차 표시를 숨기고, 카메라가 따라가던 세계의 모습을 잊습니다. 다음 방에 들어가면 `spawn_world`가 처음부터 다시 만듭니다.
pub fn clear_world(
    mut commands: Commands,
    entities: Query<Entity, With<RoomEntity>>,
    mut wagon_marker: Query<&mut Visibility, With<WagonMarkerNode>>,
) {
    for mut visibility in &mut wagon_marker {
        *visibility = Visibility::Hidden;
    }
    commands.insert_resource(FollowedSetting::default());
    for entity in &entities {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<Scene>();
    commands.insert_resource(CurrentFrame::default());
    commands.insert_resource(Spawned::default());
    commands.insert_resource(TowerRegistry::default());
    commands.insert_resource(SupplyRegistry::default());
    commands.insert_resource(LootRegistry::default());
    commands.insert_resource(StrikeRegistry::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camp;
    use crate::characters;
    use crate::loot;
    use crate::supplies;
    use crate::testing::{YOU, joined_flow};
    use crate::towers;
    use bevy::mesh::VertexAttributeValues;
    use caravan_client_core::{Frame, MonsterView, PlayerView};
    use caravan_protocol::WireBuffs;
    use caravan_protocol::{
        WireChest, WireGroundWeapon, WireLife, WireLoadout, WireMonsterEffects, WireMonsterKind,
        WireNode, WirePhase, WirePile, WirePos, WireStruck, WireStructureKind, WireTile, WireTower,
        WireTowerState, WireWagon, WireWeaponKind,
    };

    /// 모든 종류의 개체가 한 장면에 있는 야영지의 모습입니다.
    /// 플레이어 둘, 허수아비 하나(맞은 기록이 있음), 감시탑, 채집 지점, 자원 더미, 상자, 바닥의 무기가 있고, 야영지이므로 진열대도 그려집니다.
    fn crowded_frame() -> Frame {
        let player = |id| PlayerView {
            id,
            team: caravan_rules::Team::CARAVAN,
            pos: Ground::new(10.0 + id as f32, 9.0),
            facing: Ground::new(1.0, 0.0),
            life: WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: None,
                build_refusal: None,
                buffs: WireBuffs::NONE,
            },
            last_attack: None,
            loadout: WireLoadout {
                held: WireWeaponKind::Sword,
                spare: None,
            },
            prompt: None,
            build_kind: WireStructureKind::Watchtower,
        };
        Frame {
            tick: 20.0,
            expedition: 0,
            phase: WirePhase::Camp {
                ready: vec![],
                starting_in: None,
                struck: vec![WireStruck {
                    monster: 7,
                    tick: 19,
                    amount: 40,
                }],
            },
            wagon: WireWagon {
                hp: 500,
                max_hp: 500,
            },
            stash: 0,
            towers: vec![WireTower {
                id: 1,
                tile: WireTile { x: 15, y: 11 },
                kind: WireStructureKind::Watchtower,
                state: WireTowerState::Standing {
                    hp: 250,
                    last_shot: None,
                },
            }],
            nodes: vec![WireNode { id: 1, amount: 9 }],
            piles: vec![WirePile {
                id: 1,
                pos: WirePos { x: 1600, y: 1400 },
                amount: 3,
            }],
            chests: vec![WireChest {
                id: 1,
                tile: WireTile { x: 20, y: 20 },
            }],
            ground_weapons: vec![WireGroundWeapon {
                id: 1,
                pos: WirePos { x: 1500, y: 1500 },
                kind: WireWeaponKind::Bow,
            }],
            players: vec![player(YOU), player(2)],
            monsters: vec![MonsterView {
                id: 7,
                kind: WireMonsterKind::Scarecrow,
                effects: WireMonsterEffects::NONE,
                pos: Ground::new(12.0, 12.0),
                facing: Ground::new(1.0, 0.0),
                hp: 60,
            }],
            projectiles: Vec::new(),
        }
    }

    /// 방을 그리고 지우는 시스템을 `main.rs`와 같은 순서와 실행 조건으로 놓은 앱입니다.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .insert_resource(joined_flow())
            .init_resource::<CurrentFrame>()
            .init_resource::<characters::Spawned>()
            .init_resource::<towers::TowerRegistry>()
            .init_resource::<supplies::SupplyRegistry>()
            .init_resource::<loot::LootRegistry>()
            .init_resource::<camp::StrikeRegistry>()
            .add_systems(Startup, crate::palette::setup_palette)
            .add_systems(
                Update,
                (
                    clear_world
                        .run_if(not(crate::start::in_room).and_then(resource_exists::<Scene>)),
                    spawn_world
                        .run_if(crate::start::in_room.and_then(not(resource_exists::<Scene>))),
                    towers::sync_towers.run_if(resource_exists::<Scene>),
                    supplies::sync_nodes.run_if(resource_exists::<Scene>),
                    supplies::sync_piles.run_if(resource_exists::<Scene>),
                    loot::sync_chests.run_if(resource_exists::<Scene>),
                    loot::sync_ground_weapons.run_if(resource_exists::<Scene>),
                    loot::sync_racks.run_if(resource_exists::<Scene>),
                    camp::sync_strikes.run_if(resource_exists::<Scene>),
                    characters::sync_characters.run_if(resource_exists::<Scene>),
                )
                    .chain(),
            );
        app
    }

    fn show(app: &mut App, frame: Option<Frame>) {
        app.world_mut().resource_mut::<CurrentFrame>().0 = frame;
        app.update();
    }

    /// 화면에 그려진 모양(도형이나 글자)을 가진 개체의 수입니다. 카메라와 HUD는 세지 않습니다.
    fn drawn(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, Or<(With<Mesh2d>, With<Text2d>)>>();
        query.iter(world).count()
    }

    #[test]
    fn leaving_the_room_clears_everything_that_was_drawn_for_it() {
        let mut app = app();
        show(&mut app, Some(crowded_frame()));
        let in_the_room = drawn(&mut app);
        assert!(
            in_the_room > 50,
            "the ground, blocks, players, a scarecrow, a tower, a node, a pile, a chest, a weapon, racks and a number: {in_the_room}"
        );
        assert!(app.world().contains_resource::<Scene>());
        let marker = app
            .world_mut()
            .spawn((WagonMarkerNode, Visibility::Inherited))
            .id();
        app.world_mut()
            .insert_resource(FollowedSetting(Some(crowded_frame().setting())));

        app.world_mut().resource_mut::<ClientFlow>().0.leave();
        // 같은 프레임에 이전 방의 스냅샷이 남아 있어도 그리지 않고 지웁니다.
        show(&mut app, Some(crowded_frame()));

        assert_eq!(drawn(&mut app), 0, "nothing of the room is left");
        assert!(!app.world().contains_resource::<Scene>());
        assert!(app.world().resource::<CurrentFrame>().0.is_none());
        assert_eq!(
            app.world().get::<Visibility>(marker),
            Some(&Visibility::Hidden),
            "the wagon marker at the edge of the screen does not stay on the start screen"
        );
        assert!(
            app.world().resource::<FollowedSetting>().0.is_none(),
            "the camera forgets the room it followed"
        );
    }

    #[test]
    fn a_new_room_is_drawn_from_scratch_after_the_old_one_was_cleared() {
        let mut app = app();
        show(&mut app, Some(crowded_frame()));
        let first_room = drawn(&mut app);
        app.world_mut().resource_mut::<ClientFlow>().0.leave();
        show(&mut app, None);

        app.world_mut().insert_resource(joined_flow());
        show(&mut app, Some(crowded_frame()));

        assert_eq!(drawn(&mut app), first_room, "the same scene, not doubled");
    }

    #[test]
    fn the_whole_ground_is_one_mesh_with_a_diamond_for_every_tile() {
        let mesh = ground_mesh(64, 64);

        assert_eq!(mesh.count_vertices(), 64 * 64 * 4);
        assert_eq!(mesh.indices().map(Indices::len), Some(64 * 64 * 6));
    }

    #[test]
    fn neighbouring_tiles_get_different_colours_in_a_checkerboard() {
        let mesh = ground_mesh(2, 2);
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("the ground carries a colour for every vertex");
        };
        // 타일은 (0, 0), (1, 0), (0, 1), (1, 1)의 순서이고 타일마다 꼭짓점이 넷입니다.
        let tile_colour = |tile: usize| colors[tile * 4];
        assert_ne!(tile_colour(0), tile_colour(1));
        assert_ne!(tile_colour(0), tile_colour(2));
        assert_eq!(tile_colour(0), tile_colour(3));
        assert_eq!(tile_colour(1), tile_colour(2));
        for tile in 0..4 {
            assert!(
                colors[tile * 4..tile * 4 + 4]
                    .iter()
                    .all(|vertex| *vertex == tile_colour(tile))
            );
        }
    }

    #[test]
    fn the_tile_at_the_origin_is_the_diamond_around_its_screen_position() {
        let mesh = ground_mesh(1, 1);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("the ground has positions");
        };
        let center = convert::screen_pos(Ground::new(0.5, 0.5));
        let xs: Vec<f32> = positions.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = positions.iter().map(|p| p[1]).collect();
        let span = |values: &[f32]| {
            (
                values.iter().copied().fold(f32::INFINITY, f32::min),
                values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            )
        };
        assert_eq!(span(&xs), (center.x - 24.0, center.x + 24.0));
        assert_eq!(span(&ys), (center.y - 12.0, center.y + 12.0));
    }
}
