//! 플레이어와 몬스터를 화면의 도형으로 만들고 갱신합니다.
//!
//! 서버의 개체 번호에서 Bevy 개체로 가는 표(`Registry`)를 두고, 새 번호만 만들고 사라진 번호만 지웁니다.
//! 프레임마다 모든 개체를 다시 만들지 않으므로 몬스터가 수백 마리여도 매 프레임 하는 일은 위치와 막대를 맞추는 것뿐입니다.

use std::collections::HashMap;

use bevy::prelude::*;
use caravan_client_core::{MonsterView, PlayerView};
use caravan_protocol::WireLife;
use caravan_rules::Vec2 as Ground;

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::palette::Palette;
use crate::parts::{self, PartQuery, Parts};
use crate::view::{Footprint, Scene};

/// 번호에서 개체로 가는 표입니다. 한 프레임에 한 번 보인 번호를 기억해서, 보이지 않은 번호의 개체를 지웁니다.
#[derive(Default)]
struct Registry {
    entities: HashMap<u32, Tracked>,
    frame: u64,
}

struct Tracked {
    entity: Entity,
    seen_in_frame: u64,
}

impl Registry {
    fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// 번호의 개체가 이미 있으면 이번 프레임에도 보였다고 표시하고 돌려줍니다.
    fn find(&mut self, id: u32) -> Option<Entity> {
        let tracked = self.entities.get_mut(&id)?;
        tracked.seen_in_frame = self.frame;
        Some(tracked.entity)
    }

    fn insert(&mut self, id: u32, entity: Entity) {
        self.entities.insert(
            id,
            Tracked {
                entity,
                seen_in_frame: self.frame,
            },
        );
    }

    /// 이번 프레임에 보이지 않은 번호의 개체를 지웁니다. 몬스터가 죽거나 플레이어가 나갔을 때입니다 (C5, C15).
    fn despawn_unseen(&mut self, commands: &mut Commands) {
        let frame = self.frame;
        self.entities.retain(|_, tracked| {
            let seen = tracked.seen_in_frame == frame;
            if !seen {
                commands.entity(tracked.entity).despawn();
            }
            seen
        });
    }
}

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
    // 협동 모드에서는 내가 아닌 플레이어가 모두 같은 편입니다.
    let alive_color = if player.id == scene.you {
        &palette.you
    } else {
        &palette.ally
    };
    let (body, height, bar) = match player.life {
        WireLife::Alive { hp, max_hp, .. } => (
            alive_color,
            config::PLAYER_BODY.y,
            Some(hp as f32 / max_hp as f32),
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
        bar: (monster.hp < max_hp).then(|| monster.hp as f32 / max_hp as f32),
    }
}

/// 프레임 순서 5: 플레이어와 몬스터의 도형을 이번 프레임의 모습에 맞춥니다 (C2, C5, C12, C15, C18).
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
