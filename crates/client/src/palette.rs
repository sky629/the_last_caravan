//! 모든 모양(메시)과 색(재질)을 시작할 때 한 번 만들어 두는 곳입니다.
//! 몬스터가 수백 마리여도 같은 핸들을 복제해서 쓰므로, 개체를 만들 때마다 메시와 재질을 새로 만들지 않습니다.
//!
//! 색이 모두 불투명하므로 Bevy가 불투명 단계로 그립니다. 이 단계는 같은 메시와 재질의 개체를 한꺼번에 그리고,
//! 앞뒤는 깊이 버퍼(개체의 z)로 가립니다. 그래서 개체의 수가 늘어도 그리기 호출은 모양과 색의 가짓수만큼입니다.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use caravan_client_core::{TILE_HEIGHT_PX, TILE_WIDTH_PX};
use caravan_protocol::WireWeaponKind;

use crate::config;

#[derive(Resource)]
pub struct Palette {
    /// 반지름 1인 원과 한 변이 1인 정사각형입니다. `Transform`의 크기로 늘려서 씁니다.
    pub circle: Handle<Mesh>,
    pub square: Handle<Mesh>,
    pub obstacle_block: Handle<Mesh>,
    pub wagon_block: Handle<Mesh>,
    pub tower_block: Handle<Mesh>,
    pub site_block: Handle<Mesh>,
    pub node_block: Handle<Mesh>,
    pub chest_block: Handle<Mesh>,

    /// 지면 메시의 꼭짓점 색이 그대로 보이게 하는 흰색 재질입니다.
    pub ground: Handle<ColorMaterial>,
    pub obstacle: Handle<ColorMaterial>,
    pub wagon: Handle<ColorMaterial>,
    pub tower: Handle<ColorMaterial>,
    pub site: Handle<ColorMaterial>,
    pub node: Handle<ColorMaterial>,
    pub node_empty: Handle<ColorMaterial>,
    pub chest: Handle<ColorMaterial>,
    pub pile: Handle<ColorMaterial>,
    pub weapons: WeaponPaints,
    pub shadow: Handle<ColorMaterial>,
    pub you: Handle<ColorMaterial>,
    pub ally: Handle<ColorMaterial>,
    pub hostile_player: Handle<ColorMaterial>,
    pub downed: Handle<ColorMaterial>,
    pub dead: Handle<ColorMaterial>,
    pub monster: Handle<ColorMaterial>,
    pub nose_player: Handle<ColorMaterial>,
    pub nose_monster: Handle<ColorMaterial>,
    pub bar_back: Handle<ColorMaterial>,
    pub bar_player: Handle<ColorMaterial>,
    pub bar_monster: Handle<ColorMaterial>,
    pub bar_tower: Handle<ColorMaterial>,
    pub bar_site: Handle<ColorMaterial>,
    pub bar_node: Handle<ColorMaterial>,
}

/// 바닥에 놓인 무기의 재질입니다. 무기마다 색이 다릅니다.
pub struct WeaponPaints {
    sword: Handle<ColorMaterial>,
    bow: Handle<ColorMaterial>,
    greatsword: Handle<ColorMaterial>,
    spear: Handle<ColorMaterial>,
    crossbow: Handle<ColorMaterial>,
    rifle: Handle<ColorMaterial>,
}

impl WeaponPaints {
    /// `match`가 모든 무기를 다루므로, 무기를 추가하면 여기서 컴파일 오류가 납니다.
    pub fn of(&self, kind: WireWeaponKind) -> Handle<ColorMaterial> {
        match kind {
            WireWeaponKind::Sword => self.sword.clone(),
            WireWeaponKind::Bow => self.bow.clone(),
            WireWeaponKind::Greatsword => self.greatsword.clone(),
            WireWeaponKind::Spear => self.spear.clone(),
            WireWeaponKind::Crossbow => self.crossbow.clone(),
            WireWeaponKind::Rifle => self.rifle.clone(),
        }
    }
}

/// 시작할 때 한 번 실행해서 `Palette`를 넣습니다.
pub fn setup_palette(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut paint = |color: Color| materials.add(color);
    let palette = Palette {
        ground: paint(Color::WHITE),
        obstacle: paint(config::OBSTACLE),
        wagon: paint(config::WAGON),
        tower: paint(config::TOWER),
        site: paint(config::SITE),
        node: paint(config::NODE),
        node_empty: paint(config::NODE_EMPTY),
        chest: paint(config::CHEST),
        pile: paint(config::PILE),
        weapons: WeaponPaints {
            sword: paint(config::WEAPON_SWORD),
            bow: paint(config::WEAPON_BOW),
            greatsword: paint(config::WEAPON_GREATSWORD),
            spear: paint(config::WEAPON_SPEAR),
            crossbow: paint(config::WEAPON_CROSSBOW),
            rifle: paint(config::WEAPON_RIFLE),
        },
        shadow: paint(config::SHADOW),
        you: paint(config::YOU),
        ally: paint(config::ALLY),
        hostile_player: paint(config::HOSTILE_PLAYER),
        downed: paint(config::DOWNED),
        dead: paint(config::DEAD),
        monster: paint(config::MONSTER),
        nose_player: paint(config::NOSE_PLAYER),
        nose_monster: paint(config::NOSE_MONSTER),
        bar_back: paint(config::BAR_BACK),
        bar_player: paint(config::BAR_PLAYER),
        bar_monster: paint(config::BAR_MONSTER),
        bar_tower: paint(config::BAR_TOWER),
        bar_site: paint(config::BAR_SITE),
        bar_node: paint(config::BAR_NODE),

        circle: meshes.add(Circle::new(1.0)),
        square: meshes.add(Rectangle::new(1.0, 1.0)),
        obstacle_block: meshes.add(block_mesh(config::OBSTACLE_HEIGHT)),
        wagon_block: meshes.add(block_mesh(config::WAGON_HEIGHT)),
        tower_block: meshes.add(block_mesh(config::TOWER_HEIGHT)),
        site_block: meshes.add(block_mesh(config::SITE_HEIGHT)),
        node_block: meshes.add(block_mesh(config::NODE_HEIGHT)),
        chest_block: meshes.add(block_mesh(config::CHEST_HEIGHT)),
    };
    commands.insert_resource(palette);
}

/// 타일 하나 위에 서 있는 높이 `height`의 상자입니다. 원점은 바닥 마름모의 중심입니다.
///
/// 보이는 세 면(윗면, 왼쪽 앞면, 오른쪽 앞면)을 한 메시에 담고, 면마다 꼭짓점 색의 밝기를 달리해서 입체로 보이게 합니다.
/// 재질의 색이 꼭짓점 색에 곱해지므로, 같은 메시로 장애물, 마차, 감시탑, 공사터, 채집 지점, 무기 상자의 색을 모두 냅니다.
fn block_mesh(height: f32) -> Mesh {
    let half_w = TILE_WIDTH_PX / 2.0;
    let half_h = TILE_HEIGHT_PX / 2.0;
    let left = Vec2::new(-half_w, 0.0);
    let bottom = Vec2::new(0.0, -half_h);
    let right = Vec2::new(half_w, 0.0);
    let top = Vec2::new(0.0, half_h);
    let lift = Vec2::new(0.0, height);
    // 꼭짓점은 반시계 방향이고, 두 번째 값은 면의 밝기입니다.
    let faces = [
        ([left + lift, bottom + lift, right + lift, top + lift], 1.0),
        ([left, bottom, bottom + lift, left + lift], 0.78),
        ([bottom, right, right + lift, bottom + lift], 0.58),
    ];

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for (corners, brightness) in faces {
        let first = positions.len() as u32;
        for corner in corners {
            positions.push([corner.x, corner.y, 0.0]);
            colors.push([brightness, brightness, brightness, 1.0]);
        }
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}
