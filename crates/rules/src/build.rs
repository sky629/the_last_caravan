//! 건설의 조건입니다. 감시탑을 놓을 수 있는지 판정하는 곳은 이 파일의 `check_site` 하나입니다.
//!
//! 건설을 시작할 때와 완료할 때 같은 함수를 부릅니다. 시작할 때는 거기에 보관함의 자원을 확인하는 일(`start_builds`)이 더해집니다.
//! 어느 타일에 놓을지는 플레이어의 위치와 조준 방향에서 서버가 정합니다(`front_tile`). 클라이언트는 타일의 좌표를 보내지 않습니다.
//! 클라이언트는 같은 함수로 놓일 자리를 미리 보여 주므로, 「앞 타일」의 뜻이 서버와 화면에서 어긋나지 않습니다.

use crate::collision::move_circle;
use crate::ids::{PlayerId, TowerId};
use crate::kind::StructureKind;
use crate::math::{Rect, Vec2};
use crate::player::{Interaction, Life};
use crate::structures::TowerState;
use crate::tuning::Tuning;
use crate::world::{Ctx, Tile, TileUse, World};

/// 건설 키가 세우는 시설입니다.
const BUILT_KIND: StructureKind = StructureKind::Watchtower;

/// 건설이 거절되는 이유입니다. 이유가 여럿이면 아래 순서의 첫 번째입니다. 비싼 검사가 뒤에 있습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CannotBuild {
    /// 보관함의 자원이 건설 비용보다 적습니다. 건설을 시작할 때만 나옵니다.
    NotEnoughStash,
    /// 서 있는 감시탑과 짓는 중인 감시탑의 수가 상한에 닿았습니다.
    TowerLimit,
    /// 맵 밖이거나 마차에서 너무 멉니다(마을의 범위 밖입니다).
    OutsideVillage,
    /// 그 타일에 장애물, 마차, 감시탑이 있거나, 짓는 사람이 아닌 플레이어나 몬스터가 걸쳐 있습니다.
    TileTaken,
    /// 그 자리에 놓으면 걸어서 마차에 갈 수 없는 곳이 생깁니다.
    WouldBlockPath,
}

/// 플레이어가 건설 키를 누르고 있지만 세울 수 없는 상태입니다. 이벤트가 아니라 상태이므로, 스냅샷을 하나 놓쳐도 화면이 같은 안내를 그립니다.
/// 건설 키를 누르고 있는 동안 Tick마다 다시 정해지고, 키를 놓으면 사라집니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildRefusal {
    /// 놓으려던 타일입니다. 앞 타일이 맵 밖이면 플레이어가 서 있는 타일입니다.
    pub tile: Tile,
    pub why: CannotBuild,
}

/// 가로와 세로가 `map_size`(타일 수)인 맵에서, `pos`에 서서 `facing`을 바라보는 플레이어의 앞 타일입니다.
/// 서 있는 타일에서 바라보는 쪽으로 이웃한 타일이고, 이웃은 여덟 곳(상하좌우와 대각선)입니다.
/// 바라보는 방향의 두 성분에 1.2를 곱해서 반올림한 값(-1, 0, 1)이 이웃을 정합니다. 길이 1인 방향은 성분 하나가 항상 0.7 이상이므로 (0, 0)이 나오지 않습니다.
/// 서 있는 위치가 같은 타일 안에서 조금 달라도 같은 타일이 정해지므로, 감시탑을 줄지어 세우기 쉽습니다. 맵 밖이면 `None`입니다.
pub fn front_tile(pos: Vec2, facing: Vec2, map_size: (u16, u16)) -> Option<Tile> {
    let (cols, rows) = map_size;
    let here = Tile::containing(pos, cols, rows);
    let target = here.step(
        (facing.x * 1.2).round() as i32,
        (facing.y * 1.2).round() as i32,
    )?;
    (target.x < cols && target.y < rows).then_some(target)
}

/// `tile`에 감시탑을 놓을 수 있는지 판정합니다. 자원은 보지 않습니다.
///
/// `own`이 `None`이면 아직 지어지지 않은 새 자리를 검사합니다. 타일이 비어 있어야 하고, 상한에 하나 더 들어가고,
/// 연결 검사는 이 타일을 막힌 것으로 보고 합니다.
/// `own`이 `Some(id)`이면 이미 짓는 중인 감시탑 `id`가 이 타일을 차지하고 있으므로, 그 감시탑을 뺀 나머지에 대해 검사합니다.
/// 짓는 중인 다른 감시탑은 타일 지도에 이미 막힌 타일로 들어 있으므로 따로 합치지 않습니다.
///
/// `builder`의 몸은 건설할 타일에 걸쳐 있어도 거절 이유가 아닙니다. 건설을 시작할 때 타일 밖으로 밀려납니다.
pub(crate) fn check_site(
    world: &World,
    t: &Tuning,
    builder: PlayerId,
    tile: Tile,
    own: Option<TowerId>,
) -> Result<(), CannotBuild> {
    let others = world.towers.len() - usize::from(own.is_some());
    if others >= t.village.tower_limit as usize {
        return Err(CannotBuild::TowerLimit);
    }
    if !world.tiles.contains(tile)
        || world.wagon.rect.distance_to(tile.center()) > t.village.build_range
    {
        return Err(CannotBuild::OutsideVillage);
    }
    let expected = match own {
        Some(id) => TileUse::Tower(id),
        None => TileUse::Free,
    };
    if world.tiles.use_at(tile) != Some(expected) || body_on(world, t, builder, tile.rect()) {
        return Err(CannotBuild::TileTaken);
    }
    let wagon_tiles = world
        .tiles
        .engage_tiles(world.wagon.rect, t.monsters.longest_engage_distance());
    let extra_block = own.is_none().then_some(tile);
    if !world.tiles.is_fully_connected(&wagon_tiles, extra_block) {
        return Err(CannotBuild::WouldBlockPath);
    }
    Ok(())
}

/// `builder`가 아닌 플레이어나 몬스터의 몸이 `rect`에 걸쳐 있는지 봅니다. 쓰러졌거나 사망한 플레이어도 몸이 있습니다.
/// 건설을 시도할 때만 부르므로 몬스터 격자 대신 목록 전체를 훑어도 비싸지 않습니다.
fn body_on(world: &World, t: &Tuning, builder: PlayerId, rect: Rect) -> bool {
    let player_on = world
        .players
        .values()
        .any(|player| player.id != builder && rect.distance_to(player.pos) < t.player.radius);
    player_on
        || world
            .monsters
            .iter()
            .any(|monster| rect.distance_to(monster.pos) < t.monsters.get(monster.kind).radius)
}

/// 이번 Tick에 건설 키를 눌러서 새 건설을 시작하려는 플레이어들의 요청을 번호 순서로 하나씩 처리합니다.
///
/// 요청을 한꺼번에 판정하지 않고 하나씩 적용하는 이유는 앞 요청이 놓은 자리가 뒤 요청의 판정에 들어가야 하기 때문입니다.
/// 두 플레이어가 같은 Tick에 시작해도 같은 타일이거나 둘이 합쳐서 길을 막는 자리이면 뒤 요청이 거절되고,
/// 보관함이 감시탑 하나 분량이면 뒤 요청이 자원이 모자라서 거절됩니다.
/// 「먼저 읽고 모은 뒤 적용한다」는 Tick의 원칙에서 벗어나는 유일한 곳이며, 이 함수 하나에만 있습니다.
///
/// 시작하면 보관함에서 비용을 빼고 `Building` 상태의 감시탑을 놓습니다. 짓는 사람의 몸이 그 타일에 걸쳐 있으면 밖으로 밀어냅니다.
/// 거절되면 그 플레이어에게 이유를 남깁니다.
pub(crate) fn start_builds(world: &mut World, ctx: &Ctx, requests: &[PlayerId]) {
    let t = ctx.t;
    let cost = t.structures.get(BUILT_KIND).build_cost;
    let mut scratch = Vec::new();
    for &id in requests {
        let Some(player) = world.players.get(&id) else {
            continue;
        };
        // 바라보는 방향은 이번 Tick의 조준입니다. `move_players`가 고치는 것은 이 판정보다 뒤입니다.
        let facing = ctx
            .inputs
            .get(&id)
            .and_then(|input| input.aim.get())
            .unwrap_or(player.facing);
        let here = world.tiles.tile_at(player.pos);
        let verdict = match front_tile(player.pos, facing, world.tiles.size()) {
            None => Err(BuildRefusal {
                tile: here,
                why: CannotBuild::OutsideVillage,
            }),
            Some(tile) => {
                let allowed = if world.stash < cost {
                    Err(CannotBuild::NotEnoughStash)
                } else {
                    check_site(world, t, id, tile, None)
                };
                allowed
                    .map(|()| tile)
                    .map_err(|why| BuildRefusal { tile, why })
            }
        };

        match verdict {
            Ok(tile) => {
                world.stash -= cost;
                let state = TowerState::Building {
                    builder: id,
                    paid: cost,
                    elapsed_ticks: 0,
                };
                let tower = world.add_tower(BUILT_KIND, tile, state);
                if let Some(player) = world.players.get_mut(&id) {
                    player.pos = move_circle(
                        player.pos,
                        t.player.radius,
                        Vec2::ZERO,
                        &world.tiles,
                        &mut scratch,
                    );
                    if let Life::Alive {
                        interaction,
                        build_refusal,
                        ..
                    } = &mut player.life
                    {
                        *interaction = Some(Interaction::Building { tower });
                        *build_refusal = None;
                    }
                }
            }
            Err(refusal) => {
                if let Some(Life::Alive { build_refusal, .. }) =
                    world.players.get_mut(&id).map(|player| &mut player.life)
                {
                    *build_refusal = Some(refusal);
                }
            }
        }
    }
}

/// `Interaction::Building`을 가진 플레이어가 가리키는 감시탑이 아직 그 플레이어가 짓는 중인지 확인합니다.
pub(crate) fn is_building(world: &World, tower: TowerId, by: PlayerId) -> bool {
    matches!(
        world.towers.get(&tower).map(|tower| &tower.state),
        Some(TowerState::Building { builder, .. }) if *builder == by
    )
}
