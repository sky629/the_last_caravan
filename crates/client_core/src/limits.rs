//! 서버가 입장할 때 보낸 `Welcome`을 믿고 쓰기 전에 확인하는 상한입니다.
//!
//! 이 값들은 터무니없는 값을 보내는 서버(고장 났거나 악의가 있는 서버)가 클라이언트의 메모리와 시간을 끝없이 쓰지 못하게 막는 점검용 상한이고,
//! 게임 수치가 아닙니다. 실제 정원, Tick 속도, 맵의 크기는 언제나 서버의 `Tuning`이 정하며 클라이언트에는 어떤 값도 고정되어 있지 않습니다.
//! 상한은 지금의 값(정원 4, 20Hz, 24 x 24 맵)보다 훨씬 크게 잡았으므로, 정상적인 서버가 수치를 바꿔도 걸리지 않습니다.
//!
//! `Welcome`은 `Session`이 입장을 처리하는 곳에서 한 번 확인합니다. 통과한 값은 HUD와 화면 코드가 다시 확인하지 않고 믿습니다.

use caravan_protocol::Welcome;
use caravan_rules::Rect;

use crate::iso;

/// 한 방의 정원입니다. HUD가 동료 막대를 정원만큼 만듭니다.
pub const MAX_CAPACITY: u32 = 16;
/// 서버의 초당 Tick 수입니다. 재생 시각과 입력 간격이 이 값으로 계산됩니다.
pub const MAX_TICK_HZ: u32 = 240;
/// 맵 한 변의 타일 수입니다. 지면 타일을 하나씩 만드므로 한 변의 제곱만큼의 개체가 생깁니다.
pub const MAX_MAP_SIDE_TILES: f32 = 128.0;
pub const MAX_OBSTACLES: usize = 256;
pub const MAX_TOWERS: usize = 32;
pub const MAX_MONSTER_KINDS: usize = 16;
/// 장애물, 마차, 감시탑이 차지하는 타일의 합입니다. 상자를 타일마다 하나씩 만듭니다.
pub const MAX_SOLID_TILES: usize = 4096;
/// 캐릭터를 그리는 원의 반지름(타일)입니다.
pub const MAX_RADIUS_TILES: f32 = 8.0;

/// 상한을 넘거나 말이 되지 않는 값이 있으면 그 이유를 영어 문장으로 돌려줍니다. 화면에 그대로 표시됩니다.
pub fn check_welcome(welcome: &Welcome) -> Result<(), String> {
    let world = &welcome.world;

    check_range("capacity", welcome.capacity, 1, MAX_CAPACITY)?;
    check_range("tick rate", welcome.tick_hz, 1, MAX_TICK_HZ)?;
    check_count("obstacles", world.obstacles.len(), MAX_OBSTACLES)?;
    check_count("towers", world.towers.len(), MAX_TOWERS)?;
    check_count(
        "monster kinds",
        world.monster_kinds.len(),
        MAX_MONSTER_KINDS,
    )?;

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
    let solids = world
        .obstacles
        .iter()
        .copied()
        .chain([world.wagon])
        .chain(world.towers.iter().map(|site| site.rect));
    let mut solid_tiles = 0;
    for solid in solids {
        let rect = solid.to_rect();
        if !is_inside(rect, map) {
            return Err("an obstacle, the wagon, or a tower lies outside the map".to_string());
        }
        solid_tiles += iso::tile_count(rect);
    }
    check_count("solid tiles", solid_tiles, MAX_SOLID_TILES)
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
    use caravan_protocol::{MonsterKindInfo, TowerSite, WirePos, WireRect};
    use caravan_rules::{Sim, Tuning};

    /// 기본 수치에서 서버가 보내는 입장 안내입니다.
    fn normal() -> Welcome {
        let mut sim = Sim::new(Tuning::default());
        let first = sim.add_player().unwrap();
        Welcome::from_sim(&sim, sim.player(first).unwrap())
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
    fn too_many_obstacles_towers_or_monster_kinds_are_refused() {
        let mut welcome = normal();
        welcome.world.obstacles = vec![rect(1, 1, tiles(2), tiles(2)); MAX_OBSTACLES + 1];
        assert!(refused(&welcome).contains("obstacles"));

        let mut welcome = normal();
        let site = TowerSite {
            id: 1,
            rect: rect(tiles(15), tiles(11), tiles(16), tiles(12)),
        };
        welcome.world.towers = vec![site; MAX_TOWERS + 1];
        assert!(refused(&welcome).contains("towers"));

        let mut welcome = normal();
        let kind = welcome.world.monster_kinds[0];
        welcome.world.monster_kinds = vec![kind; MAX_MONSTER_KINDS + 1];
        assert!(refused(&welcome).contains("monster kinds"));
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
        welcome.world.obstacles = vec![rect(tiles(20), tiles(20), tiles(40), tiles(21))];
        assert!(refused(&welcome).contains("outside the map"), "outside");

        let mut welcome = normal();
        welcome.world.wagon = rect(tiles(14), tiles(13), tiles(10), tiles(11));
        assert!(refused(&welcome).contains("outside the map"), "backwards");
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
