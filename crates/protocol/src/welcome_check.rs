//! 입장 안내(`Welcome`)가 믿고 쓸 만한 값인지 확인하는 상한입니다. 클라이언트와 서버가 이 파일의 정의 하나를 함께 씁니다.
//!
//! 이 값들은 터무니없는 값을 보내는 서버(고장 났거나 악의가 있는 서버)가 클라이언트의 메모리와 시간을 끝없이 쓰지 못하게 막는 점검용 상한이고,
//! 게임 수치가 아닙니다. 실제 정원, Tick 속도, 맵의 크기는 언제나 서버의 `Tuning`이 정하며 클라이언트에는 어떤 값도 고정되어 있지 않습니다.
//! 상한은 지금의 값(정원 4, 20Hz, 64 x 64 맵, 포탑 40개, 채집 지점 8곳)보다 훨씬 크게 잡았으므로, 정상적인 서버가 수치를 바꿔도 걸리지 않습니다.
//!
//! 클라이언트는 `Welcome`을 받아서 처음 믿기 전에 한 번 확인합니다. 서버는 시작할 때 자기 설정으로 보낼 `Welcome`에 같은 확인을 해 보므로,
//! 모든 클라이언트가 거절할 설정으로는 서버가 시작되지 않습니다.

use caravan_rules::Rect;

use crate::monster_status::MONSTER_KIND_CAPACITY;
use crate::units::WirePos;
use crate::welcome::{Welcome, WireDelivery};

/// 한 방의 정원입니다. HUD가 동료 막대를 정원만큼 만듭니다.
const MAX_CAPACITY: u32 = 16;
/// 서버의 초당 Tick 수입니다. 재생 시각과 입력 간격이 이 값으로 계산됩니다.
const MAX_TICK_HZ: u32 = 240;
/// 맵 한 변의 타일 수입니다. 지면 타일을 하나씩 만드므로 한 변의 제곱만큼의 개체가 생깁니다.
const MAX_MAP_SIDE_TILES: f32 = 128.0;
// 서버는 이 상한으로 맵 좌표가 전송 범위(`WirePos`)를 넘어 잘리는 설정까지 거절합니다. 그러려면 상한이 전송 범위 안에 있어야 합니다.
const _: () = assert!(MAX_MAP_SIDE_TILES <= WirePos::MAX_TILES);
// 타일의 좌표(`WireTile`)는 한 축이 `u8`이므로, 상한이 `u8`의 범위 안에 있어야 합니다.
const _: () = assert!(MAX_MAP_SIDE_TILES <= 256.0);
const MAX_OBSTACLES: usize = 256;
/// 포탑의 수의 상한입니다. 스냅샷이 포탑 하나당 10바이트 안팎이므로 이 값이 스냅샷 크기의 상한이기도 합니다.
const MAX_TOWER_LIMIT: u32 = 256;
/// 포탑 종류의 수의 상한입니다. 클라이언트가 종류마다 표의 항목과 색을 하나씩 가집니다.
const MAX_STRUCTURE_KINDS: usize = 16;
/// 무기 종류의 수의 상한입니다. 클라이언트가 무기마다 표의 항목 하나를 가집니다.
const MAX_WEAPON_KINDS: usize = 16;
/// 채집 지점의 수의 상한입니다. 스냅샷이 지점 하나당 몇 바이트이고, 클라이언트가 지점마다 상자와 막대를 하나씩 만듭니다.
const MAX_GATHER_NODES: usize = 64;
/// 장애물, 마차, 채집 지점이 차지하는 타일의 합입니다. 상자를 타일마다 하나씩 만듭니다.
const MAX_SOLID_TILES: usize = 4096;
/// 캐릭터를 그리는 원의 반지름(타일)입니다.
const MAX_RADIUS_TILES: f32 = 8.0;
/// 포탑이 작용하는 거리(타일)의 상한입니다. 맵 한 변보다 길면 범위의 원이 맵을 모두 덮습니다.
const MAX_TOWER_RANGE_TILES: f32 = MAX_MAP_SIDE_TILES;

/// 상한을 넘거나 말이 되지 않는 값이 있으면 그 이유를 영어 문장으로 돌려줍니다. 클라이언트는 이 문장을 화면에 그대로 표시하고, 서버는 시작 오류로 그대로 출력합니다.
pub fn check_welcome(welcome: &Welcome) -> Result<(), String> {
    let world = &welcome.world;

    check_range("capacity", welcome.capacity, 1, MAX_CAPACITY)?;
    check_range("tick rate", welcome.tick_hz, 1, MAX_TICK_HZ)?;
    check_count("obstacles", world.obstacles.len(), MAX_OBSTACLES)?;
    check_range("tower limit", world.tower_limit, 1, MAX_TOWER_LIMIT)?;
    check_count("gather nodes", world.nodes.len(), MAX_GATHER_NODES)?;
    check_count(
        "monster kinds",
        world.monster_kinds.len(),
        MONSTER_KIND_CAPACITY,
    )?;
    check_count("weapon kinds", world.weapons.len(), MAX_WEAPON_KINDS)?;
    check_count("weapon racks", world.racks.len(), MAX_WEAPON_KINDS)?;
    check_count(
        "tower kinds",
        world.structure_kinds.len(),
        MAX_STRUCTURE_KINDS,
    )?;

    // 최대 HP가 0이면 막대의 비율을 정할 수 없으므로, 몬스터 종류는 최대 HP가 1 이상이어야 합니다.
    if world.monster_kinds.iter().any(|kind| kind.max_hp == 0) {
        return Err("a monster kind has a max HP of 0".to_string());
    }

    // 최대 HP가 0이면 막대의 비율을 정할 수 없으므로, 포탑 종류도 최대 HP가 1 이상이어야 합니다.
    if world.structure_kinds.iter().any(|kind| kind.max_hp == 0) {
        return Err("a tower kind has a max HP of 0".to_string());
    }
    for kind in &world.structure_kinds {
        if !(kind.range > 0.0 && kind.range <= MAX_TOWER_RANGE_TILES) {
            return Err(format!(
                "a tower range of {} tiles is outside 0 to {MAX_TOWER_RANGE_TILES} tiles",
                kind.range
            ));
        }
    }

    for weapon in &world.weapons {
        if let WireDelivery::Arc {
            reach,
            half_angle_deg,
        } = weapon.delivery
            && !((0.0..=MAX_RADIUS_TILES).contains(&reach)
                && (0.0..=180.0).contains(&half_angle_deg))
        {
            return Err(format!(
                "a weapon arc of {reach} tiles and {half_angle_deg} degrees is outside 0 to {MAX_RADIUS_TILES} tiles and 0 to 180 degrees"
            ));
        }
    }

    let size = world.size.to_tiles();
    let side = 1.0..=MAX_MAP_SIDE_TILES;
    if !side.contains(&size.x) || !side.contains(&size.y) {
        return Err(format!(
            "map size {} x {} tiles is outside 1 to {MAX_MAP_SIDE_TILES} tiles per side",
            size.x, size.y
        ));
    }

    let radii = std::iter::once(world.player_radius)
        .chain(world.monster_kinds.iter().map(|kind| kind.radius));
    for radius in radii {
        if !(radius > 0.0 && radius <= MAX_RADIUS_TILES) {
            return Err(format!(
                "a body radius of {radius} tiles is outside 0 to {MAX_RADIUS_TILES} tiles"
            ));
        }
    }

    let map = Rect::new(0.0, 0.0, size.x, size.y);
    let solids = world.obstacles.iter().copied().chain([world.wagon]);
    let mut solid_tiles = 0;
    for solid in solids {
        let rect = solid.to_rect();
        if !is_inside(rect, map) {
            return Err("an obstacle or the wagon lies outside the map".to_string());
        }
        solid_tiles += rect.tile_count();
    }
    for node in &world.nodes {
        if !is_inside(node.tile.to_tile().rect(), map) {
            return Err("a gather node lies outside the map".to_string());
        }
    }
    for rack in &world.racks {
        if !map.contains(rack.pos.to_tiles()) {
            return Err("a weapon rack lies outside the map".to_string());
        }
    }
    check_count(
        "solid tiles",
        solid_tiles + world.nodes.len(),
        MAX_SOLID_TILES,
    )
}

fn is_inside(rect: Rect, map: Rect) -> bool {
    rect.min.x <= rect.max.x
        && rect.min.y <= rect.max.y
        && map.contains(rect.min)
        && map.contains(rect.max)
}

fn check_range(name: &str, value: u32, min: u32, max: u32) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} {value} is outside {min} to {max}"))
    }
}

fn check_count(name: &str, value: usize, max: usize) -> Result<(), String> {
    if value <= max {
        Ok(())
    } else {
        Err(format!("{value} {name} is above the limit {max}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MonsterKindInfo, StructureKindInfo, WireRect};
    use caravan_rules::{Seed, Sim, Tuning};

    /// 기본 수치에서 서버가 보내는 입장 안내입니다.
    fn normal() -> Welcome {
        let mut sim = Sim::new(Tuning::default(), Seed(1));
        let first = sim.add_player().unwrap();
        let room = crate::RoomCode::parse("test").unwrap();
        Welcome::from_sim(
            &sim,
            sim.player(first).unwrap(),
            &room,
            crate::SeatToken([7; 16]),
        )
    }

    fn rect(min_x: u16, min_y: u16, max_x: u16, max_y: u16) -> WireRect {
        WireRect {
            min: WirePos { x: min_x, y: min_y },
            max: WirePos { x: max_x, y: max_y },
        }
    }

    fn tiles(count: u16) -> u16 {
        count * 128
    }

    fn refused(welcome: &Welcome) -> String {
        check_welcome(welcome).expect_err("the welcome should be refused")
    }

    #[test]
    fn a_normal_welcome_is_accepted() {
        assert_eq!(check_welcome(&normal()), Ok(()));
    }

    #[test]
    fn a_welcome_at_every_limit_is_still_accepted() {
        let mut welcome = normal();
        welcome.capacity = MAX_CAPACITY;
        welcome.tick_hz = MAX_TICK_HZ;
        welcome.world.size = WirePos {
            x: tiles(128),
            y: tiles(128),
        };
        welcome.world.obstacles = vec![rect(0, 0, tiles(1), tiles(1)); MAX_OBSTACLES];
        assert_eq!(check_welcome(&welcome), Ok(()));
    }

    #[test]
    fn a_capacity_that_is_zero_or_too_big_is_refused() {
        let mut welcome = normal();
        welcome.capacity = MAX_CAPACITY + 1;
        assert!(refused(&welcome).contains("capacity"), "too big");
        welcome.capacity = 0;
        assert!(refused(&welcome).contains("capacity"), "zero");
        welcome.capacity = u32::MAX;
        assert!(refused(&welcome).contains("capacity"), "huge");
    }

    #[test]
    fn a_tick_rate_that_is_zero_or_too_high_is_refused() {
        let mut welcome = normal();
        welcome.tick_hz = 0;
        assert!(refused(&welcome).contains("tick rate"), "zero");
        welcome.tick_hz = MAX_TICK_HZ + 1;
        assert!(refused(&welcome).contains("tick rate"), "too high");
    }

    #[test]
    fn a_tower_limit_that_is_zero_or_too_big_is_refused() {
        let mut welcome = normal();
        welcome.world.tower_limit = 0;
        assert!(refused(&welcome).contains("tower limit"), "zero");
        welcome.world.tower_limit = MAX_TOWER_LIMIT + 1;
        assert!(refused(&welcome).contains("tower limit"), "too big");
        welcome.world.tower_limit = MAX_TOWER_LIMIT;
        assert_eq!(check_welcome(&welcome), Ok(()), "the limit itself is fine");
    }

    #[test]
    fn too_many_gather_nodes_or_a_node_off_the_map_are_refused() {
        let mut welcome = normal();
        let node = welcome.world.nodes[0];
        welcome.world.nodes = vec![node; MAX_GATHER_NODES];
        assert_eq!(check_welcome(&welcome), Ok(()), "the limit itself is fine");
        welcome.world.nodes = vec![node; MAX_GATHER_NODES + 1];
        assert!(refused(&welcome).contains("gather nodes"));

        let mut welcome = normal();
        welcome.world.nodes[0].tile = crate::WireTile { x: 64, y: 3 };
        assert!(refused(&welcome).contains("gather node"), "off the map");
    }

    #[test]
    fn too_many_weapon_racks_or_a_rack_off_the_map_are_refused() {
        let mut welcome = normal();
        let rack = welcome.world.racks[0];
        welcome.world.racks = vec![rack; MAX_WEAPON_KINDS];
        assert_eq!(check_welcome(&welcome), Ok(()), "the limit itself is fine");
        welcome.world.racks = vec![rack; MAX_WEAPON_KINDS + 1];
        assert!(refused(&welcome).contains("weapon racks"));

        let mut welcome = normal();
        welcome.world.racks[0].pos = WirePos {
            x: tiles(65),
            y: tiles(3),
        };
        assert!(refused(&welcome).contains("weapon rack"), "off the map");
    }

    #[test]
    fn a_map_that_is_empty_or_too_big_is_refused() {
        let mut welcome = normal();
        welcome.world.size = WirePos {
            x: tiles(129),
            y: tiles(24),
        };
        assert!(refused(&welcome).contains("map size"), "too wide");
        welcome.world.size = WirePos {
            x: tiles(24),
            y: u16::MAX,
        };
        assert!(refused(&welcome).contains("map size"), "too tall");
        welcome.world.size = WirePos { x: 0, y: tiles(24) };
        assert!(refused(&welcome).contains("map size"), "empty");
    }

    #[test]
    fn too_many_obstacles_or_monster_kinds_are_refused() {
        let mut welcome = normal();
        welcome.world.obstacles = vec![rect(1, 1, tiles(2), tiles(2)); MAX_OBSTACLES + 1];
        assert!(refused(&welcome).contains("obstacles"));

        let mut welcome = normal();
        let kind = welcome.world.monster_kinds[0];
        welcome.world.monster_kinds = vec![kind; MONSTER_KIND_CAPACITY + 1];
        assert!(refused(&welcome).contains("monster kinds"));
    }

    #[test]
    fn too_many_weapon_kinds_are_refused() {
        let mut welcome = normal();
        let weapon = welcome.world.weapons[0];
        welcome.world.weapons = vec![weapon; MAX_WEAPON_KINDS];
        assert_eq!(check_welcome(&welcome), Ok(()), "the limit itself is fine");

        welcome.world.weapons = vec![weapon; MAX_WEAPON_KINDS + 1];
        assert!(refused(&welcome).contains("weapon kinds"));
    }

    #[test]
    fn a_weapon_arc_that_is_not_a_real_reach_and_angle_is_refused() {
        for (reach, half_angle_deg) in [
            (-1.0, 60.0),
            (MAX_RADIUS_TILES + 1.0, 60.0),
            (f32::NAN, 60.0),
            (1.0, -1.0),
            (1.0, 181.0),
            (1.0, f32::NAN),
        ] {
            let mut welcome = normal();
            welcome.world.weapons[0].delivery = WireDelivery::Arc {
                reach,
                half_angle_deg,
            };
            assert!(
                refused(&welcome).contains("weapon arc"),
                "{reach} tiles, {half_angle_deg} degrees"
            );
        }
    }

    #[test]
    fn solids_that_cover_too_many_tiles_or_leave_the_map_are_refused() {
        let mut welcome = normal();
        welcome.world.size = WirePos {
            x: tiles(128),
            y: tiles(128),
        };
        welcome.world.obstacles = vec![rect(0, 0, tiles(64), tiles(64)); 2];
        assert!(refused(&welcome).contains("solid tiles"), "too many tiles");

        let mut welcome = normal();
        welcome.world.obstacles = vec![rect(tiles(60), tiles(20), tiles(80), tiles(21))];
        assert!(refused(&welcome).contains("outside the map"), "outside");

        let mut welcome = normal();
        welcome.world.wagon = rect(tiles(14), tiles(13), tiles(10), tiles(11));
        assert!(refused(&welcome).contains("outside the map"), "backwards");
    }

    #[test]
    fn a_monster_kind_with_zero_max_hp_is_refused() {
        let mut welcome = normal();
        welcome.world.monster_kinds[0] = MonsterKindInfo {
            max_hp: 0,
            ..welcome.world.monster_kinds[0]
        };
        assert!(refused(&welcome).contains("max HP"));
    }

    #[test]
    fn too_many_tower_kinds_are_refused() {
        let mut welcome = normal();
        let kind = welcome.world.structure_kinds[0];
        welcome.world.structure_kinds = vec![kind; MAX_STRUCTURE_KINDS];
        assert_eq!(check_welcome(&welcome), Ok(()), "the limit itself is fine");

        welcome.world.structure_kinds = vec![kind; MAX_STRUCTURE_KINDS + 1];
        assert!(refused(&welcome).contains("tower kinds"));
    }

    #[test]
    fn a_tower_kind_with_zero_max_hp_is_refused() {
        let mut welcome = normal();
        welcome.world.structure_kinds[0] = StructureKindInfo {
            max_hp: 0,
            ..welcome.world.structure_kinds[0]
        };
        assert!(refused(&welcome).contains("tower kind has a max HP"));
    }

    #[test]
    fn a_tower_range_that_is_not_a_positive_number_within_the_map_is_refused() {
        for bad in [
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            MAX_TOWER_RANGE_TILES + 1.0,
        ] {
            let mut welcome = normal();
            welcome.world.structure_kinds[1] = StructureKindInfo {
                range: bad,
                ..welcome.world.structure_kinds[1]
            };
            assert!(refused(&welcome).contains("tower range"), "range {bad}");
        }
        let mut welcome = normal();
        welcome.world.structure_kinds[1].range = MAX_TOWER_RANGE_TILES;
        assert_eq!(check_welcome(&welcome), Ok(()), "the limit itself is fine");
    }

    #[test]
    fn a_body_radius_that_is_not_a_small_positive_number_is_refused() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, MAX_RADIUS_TILES + 1.0] {
            let mut welcome = normal();
            welcome.world.player_radius = bad;
            assert!(refused(&welcome).contains("radius"), "player radius {bad}");

            let mut welcome = normal();
            welcome.world.monster_kinds[0] = MonsterKindInfo {
                radius: bad,
                ..welcome.world.monster_kinds[0]
            };
            assert!(refused(&welcome).contains("radius"), "monster radius {bad}");
        }
    }
}
