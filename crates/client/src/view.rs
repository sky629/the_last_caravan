//! 고정된 세계(지면, 장애물, 마차)를 한 번 만듭니다. 감시탑은 건설과 파괴로 바뀌므로 `towers`가, 채집 지점과 떨어진 자원은 `supplies`가 프레임마다 맞추고, 카메라는 `camera`가 맡습니다.
//!
//! 세계의 모양은 입장할 때 받은 `Welcome`에서 오므로, 서버가 다른 맵으로 실행되어도 클라이언트를 고치지 않습니다.
//! `Session`이 입장할 때 맵의 크기와 장애물의 수에 상한을 확인했으므로, 여기서는 받은 값만큼 개체를 만들어도 됩니다.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use caravan_client_core as core;
use caravan_protocol::{MonsterKindInfo, NodeInfo, WireMonsterKind};
use caravan_rules::{Team, Vec2 as Ground};

use crate::config;
use crate::convert;
use crate::net::ClientSession;
use crate::palette::Palette;

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
    session: Res<ClientSession>,
    palette: Res<Palette>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(joined) = session.0.joined() else {
        return;
    };
    let welcome = &joined.welcome;
    let world = &welcome.world;
    let map_size = world.size.to_tiles();

    commands.spawn((
        Mesh2d(meshes.add(ground_mesh(map_size.x as u32, map_size.y as u32))),
        MeshMaterial2d(palette.ground.clone()),
        Transform::from_xyz(0.0, 0.0, config::GROUND_Z),
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
        monsters,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

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
