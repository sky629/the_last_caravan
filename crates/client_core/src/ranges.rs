//! 화면에 그릴 포탑의 범위입니다. 서 있는 범위형 포탑(범위 안의 모두에게 작용하는 포탑)마다 원 하나를 그리고,
//! 건설 미리 보기가 있으면 그 타일에 고른 종류의 범위를 그립니다. 범위와 범위형인지는 `Welcome`의 종류표에서 읽습니다.

use caravan_protocol::{WireStructureKind, WireTowerState, WorldInfo};
use caravan_rules::{Tile, Vec2};

use crate::interp::Frame;

/// 지면 위의 원 하나입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeRing {
    pub kind: WireStructureKind,
    /// 지면 좌표(타일)입니다.
    pub center: Vec2,
    /// 타일 수입니다.
    pub radius: f32,
    /// 건설 미리 보기의 원이면 참입니다. 화면은 서 있는 포탑의 원보다 진하게 그립니다.
    pub preview: bool,
}

/// 이번 프레임에 그릴 범위입니다. `preview`는 건설 키를 누르면 포탑이 놓일 타일과 지을 종류입니다.
/// 미리 보기는 범위 안의 하나를 겨누는 종류에도 범위를 그려서, 어디까지 닿는 포탑을 고르는지 보여 줍니다.
pub fn range_rings(
    frame: &Frame,
    world: &WorldInfo,
    preview: Option<(Tile, WireStructureKind)>,
) -> Vec<RangeRing> {
    let standing = frame.towers.iter().filter_map(|tower| {
        let info = world.structure(tower.kind)?;
        let stands = matches!(tower.state, WireTowerState::Standing { .. });
        (stands && info.covers_area).then_some(RangeRing {
            kind: tower.kind,
            center: tower.tile.to_tile().center(),
            radius: info.range,
            preview: false,
        })
    });
    let previewed = preview.and_then(|(tile, kind)| {
        let info = world.structure(kind)?;
        Some(RangeRing {
            kind,
            center: tile.center(),
            radius: info.range,
            preview: true,
        })
    });
    standing.chain(previewed).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::{
        RoomCode, SeatToken, Welcome, WirePhase, WireShot, WireTile, WireTower, WireWagon,
    };
    use caravan_rules::{Seed, Sim, Tuning};

    fn world() -> WorldInfo {
        let mut sim = Sim::new(Tuning::default(), Seed(1));
        let id = sim.add_player().unwrap();
        let room = RoomCode::parse("test").unwrap();
        Welcome::from_sim(&sim, sim.player(id).unwrap(), &room, SeatToken([7; 16])).world
    }

    fn tower(id: u32, kind: WireStructureKind, state: WireTowerState) -> WireTower {
        WireTower {
            id,
            tile: WireTile {
                x: id as u8 * 2,
                y: 4,
            },
            kind,
            state,
        }
    }

    fn standing() -> WireTowerState {
        WireTowerState::Standing {
            hp: 300,
            last_shot: None::<WireShot>,
        }
    }

    fn frame(towers: Vec<WireTower>) -> Frame {
        Frame {
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
            towers,
            nodes: Vec::new(),
            piles: Vec::new(),
            chests: Vec::new(),
            ground_weapons: Vec::new(),
            players: Vec::new(),
            monsters: Vec::new(),
            projectiles: Vec::new(),
        }
    }

    #[test]
    fn only_standing_towers_that_cover_an_area_show_their_range() {
        let towers = vec![
            tower(1, WireStructureKind::Watchtower, standing()),
            tower(2, WireStructureKind::SlowTower, standing()),
            tower(3, WireStructureKind::StunTower, standing()),
            tower(4, WireStructureKind::HasteTower, standing()),
            tower(5, WireStructureKind::SwiftTower, standing()),
            tower(
                6,
                WireStructureKind::SlowTower,
                WireTowerState::Building { progress: 0.5 },
            ),
            tower(
                7,
                WireStructureKind::HasteTower,
                WireTowerState::Building { progress: 0.5 },
            ),
        ];

        let rings = range_rings(&frame(towers), &world(), None);

        let shown: Vec<(WireStructureKind, f32)> =
            rings.iter().map(|ring| (ring.kind, ring.radius)).collect();
        assert_eq!(
            shown,
            [
                (WireStructureKind::SlowTower, 4.0),
                (WireStructureKind::HasteTower, 3.0),
                (WireStructureKind::SwiftTower, 3.0),
            ]
        );
        assert!(rings.iter().all(|ring| !ring.preview));
    }

    #[test]
    fn the_preview_shows_the_range_of_the_chosen_kind_even_for_a_tower_that_aims_at_one_monster() {
        let rings = range_rings(
            &frame(Vec::new()),
            &world(),
            Some((Tile::new(9, 9), WireStructureKind::StunTower)),
        );

        assert_eq!(
            rings,
            [RangeRing {
                kind: WireStructureKind::StunTower,
                center: Tile::new(9, 9).center(),
                radius: 5.0,
                preview: true,
            }]
        );
    }

    #[test]
    fn a_kind_the_welcome_does_not_list_has_no_range_to_draw() {
        let mut world = world();
        world
            .structure_kinds
            .retain(|info| info.kind != WireStructureKind::SlowTower);

        let rings = range_rings(
            &frame(vec![tower(1, WireStructureKind::SlowTower, standing())]),
            &world,
            Some((Tile::new(9, 9), WireStructureKind::SlowTower)),
        );

        assert!(rings.is_empty());
    }
}
