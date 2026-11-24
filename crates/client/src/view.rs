//! 고정된 세계(지면, 장애물, 마차, 감시탑)를 한 번 만들고, 감시탑의 상태를 프레임마다 반영합니다.
//!
//! 세계의 모양은 입장할 때 받은 `Welcome`에서 오므로, 서버가 다른 맵으로 실행되어도 클라이언트를 고치지 않습니다.
//! `Session`이 입장할 때 맵의 크기와 장애물의 수에 상한을 확인했으므로, 여기서는 받은 값만큼 개체를 만들어도 됩니다.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use caravan_client_core as core;
use caravan_protocol::{MonsterKindInfo, WireMonsterKind, WireTowerState};
use caravan_rules::{Team, Vec2 as Ground};

use crate::config;
use crate::convert;
use crate::net::{ClientSession, CurrentFrame};
use crate::palette::Palette;
use crate::parts::{self, Bar, PartQuery};

/// 입장한 뒤에 만든 세계에서 캐릭터를 그릴 때 필요한 값입니다. 세계를 만들면 한 번 들어갑니다.
#[derive(Resource)]
pub struct Scene {
    pub you: u32,
    /// 내 진영입니다. 같은 진영의 플레이어만 같은 편으로 그립니다.
    pub team: Team,
    pub player: Footprint,
    monsters: Vec<MonsterLook>,
    towers: Vec<TowerPlace>,
}

struct TowerPlace {
    id: u32,
    center: Ground,
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

    /// 감시탑이 서 있는 지면 위치입니다. 파괴되어도 같은 자리에 잔해가 남습니다.
    pub fn tower_center(&self, id: u32) -> Option<Ground> {
        self.towers
            .iter()
            .find(|place| place.id == id)
            .map(|place| place.center)
    }
}

#[derive(Component)]
pub struct TowerView {
    id: u32,
    bar: Bar,
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

/// 입장한 직후 한 번만 세계를 만들고 카메라를 맞춥니다.
///
/// 큰 상자(긴 벽, 마차)는 타일마다 상자 하나로 나눕니다. 상자마다 자기 중심으로 그리는 순서를 정해야,
/// 캐릭터가 긴 벽의 한쪽 끝 옆에 서도 벽의 앞과 뒤가 맞게 가려집니다 (C12).
pub fn spawn_world(
    mut commands: Commands,
    session: Res<ClientSession>,
    palette: Res<Palette>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Some(joined) = session.0.joined() else {
        return;
    };
    let welcome = &joined.welcome;
    let world = &welcome.world;
    let map_size = world.size.to_tiles();

    for y in 0..map_size.y as u32 {
        for x in 0..map_size.x as u32 {
            let center = Ground::new(x as f32 + 0.5, y as f32 + 0.5);
            let material = if (x + y) % 2 == 0 {
                &palette.ground_a
            } else {
                &palette.ground_b
            };
            commands.spawn((
                Mesh2d(palette.tile.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_translation(convert::screen_pos(center).extend(config::GROUND_Z)),
            ));
        }
    }
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
    let mut towers = Vec::new();
    for site in &world.towers {
        let center = site.rect.to_rect().center();
        let bar_y = config::TOWER_HEIGHT + config::BAR_GAP;
        let bar = parts::spawn_bar(
            &mut commands,
            &palette,
            &palette.bar_tower,
            bar_y,
            Some(1.0),
        );
        let tower = spawn_block(&mut commands, &palette.tower_block, &palette.tower, center);
        commands
            .entity(tower)
            .insert(TowerView { id: site.id, bar })
            .add_children(&bar.entities());
        towers.push(TowerPlace {
            id: site.id,
            center,
        });
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
        monsters,
        towers,
    });

    // 카메라는 맵 전체가 들어오는 자리에 고정합니다. 모든 클라이언트가 같은 화면을 보므로 동기화를 눈으로 비교하기 쉽습니다.
    let view = core::map_view(map_size);
    let (mut transform, mut projection) = camera.into_inner();
    transform.translation.x = view.center.x;
    transform.translation.y = view.center.y;
    *projection = Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::AutoMin {
            min_width: view.size.x,
            min_height: view.size.y,
        },
        ..OrthographicProjection::default_2d()
    });
}

/// 감시탑이 서 있는지 파괴되었는지에 따라 모양과 색을 바꾸고, HP 막대를 맞춥니다 (C21).
pub fn sync_structures(
    frame: Res<CurrentFrame>,
    palette: Res<Palette>,
    mut towers: Query<(&TowerView, &mut Mesh2d, &mut MeshMaterial2d<ColorMaterial>)>,
    mut part_query: PartQuery,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    for (view, mut mesh, mut material) in &mut towers {
        let Some(tower) = frame.towers.iter().find(|tower| tower.id == view.id) else {
            continue;
        };
        let (block, paint, hp_fraction) = match tower.state {
            WireTowerState::Standing { hp, .. } => (
                &palette.tower_block,
                &palette.tower,
                Some(hp as f32 / tower.max_hp as f32),
            ),
            WireTowerState::Destroyed => (&palette.rubble_block, &palette.rubble, None),
        };
        if mesh.0 != *block {
            mesh.0 = block.clone();
        }
        if material.0 != *paint {
            material.0 = paint.clone();
        }
        view.bar.place(
            &mut part_query,
            config::TOWER_HEIGHT + config::BAR_GAP,
            hp_fraction,
        );
    }
}
