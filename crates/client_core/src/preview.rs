//! 건설 키를 누르기 전에 포탑이 놓일 타일을 알려 주는 계산입니다.
//!
//! 타일을 정하는 함수는 서버가 건설을 시작할 때 쓰는 `caravan_rules::front_tile` 하나이고, 여기서도 같은 함수를 부릅니다.
//! 놓을 수 있는지는 판정하지 않습니다. 판정은 서버만 하고, 거절되면 이유와 타일이 스냅샷으로 옵니다.
//! 입력 메시지에는 타일의 좌표가 없으므로, 이 값은 화면에 그리는 데만 씁니다.
//!
//! 화면의 캐릭터는 서버보다 조금 늦은 위치(보간 지연)에 그려지고, 미리 보기도 그 위치에서 계산합니다.
//! 그래서 걸으면서 건설 키를 누르면 서버가 정한 타일이 미리 보기보다 한 칸 앞일 수 있습니다. 멈춰 서면 둘이 같습니다.

use caravan_protocol::{WireInteraction, WireLife, WireStructureKind, WireTower};
use caravan_rules::{Tile, front_tile};

use crate::interp::PlayerView;

/// `me`가 지금 건설 키를 누르면 포탑이 놓일 타일입니다. 맵의 크기는 `map_size`(타일 수)입니다.
/// 이미 무언가를 하고 있거나 거절 이유가 화면에 있는 플레이어와 쓰러진 플레이어에게는 보여 주지 않습니다.
pub fn build_preview(me: &PlayerView, map_size: (u16, u16)) -> Option<Tile> {
    match me.life {
        WireLife::Alive {
            interaction: None,
            build_refusal: None,
            ..
        } => front_tile(me.pos, me.facing, map_size),
        _ => None,
    }
}

/// 지을 종류를 바꾼 뒤 미리 보기의 범위를 보여 주는 시간(초)입니다.
pub const PREVIEW_RANGE_SECS: f32 = 2.0;

/// 지금 범위를 미리 보여 줄 자리와 종류입니다. 범위의 원은 크고 캐릭터를 따라다니므로 필요할 때만 보여 줍니다.
/// - 내가 포탑을 짓는 동안에는 그 포탑의 자리와 종류입니다.
/// - 지을 종류를 바꾼 뒤 `PREVIEW_RANGE_SECS` 동안에는 건설 키를 누르면 포탑이 놓일 타일과 고른 종류입니다.
///   `secs_since_kind_changed`는 내 화면에서 고른 종류가 바뀐 뒤 지난 시간이고, 바뀐 적이 없으면 `None`입니다.
/// - 그 밖에는 보여 주지 않습니다.
pub fn range_preview(
    me: &PlayerView,
    towers: &[WireTower],
    map_size: (u16, u16),
    secs_since_kind_changed: Option<f32>,
) -> Option<(Tile, WireStructureKind)> {
    if let WireLife::Alive {
        interaction: Some(WireInteraction::Building { tower }),
        ..
    } = me.life
    {
        let site = towers.iter().find(|candidate| candidate.id == tower)?;
        return Some((site.tile.to_tile(), site.kind));
    }
    let just_chosen = secs_since_kind_changed.is_some_and(|secs| secs < PREVIEW_RANGE_SECS);
    just_chosen
        .then(|| build_preview(me, map_size))
        .flatten()
        .map(|tile| (tile, me.build_kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::{
        WireBuffs, WireBuildRefusal, WireCannotBuild, WireLoadout, WireTile, WireTowerState,
        WireWeaponKind,
    };
    use caravan_rules::{AimDir, PlayerInput, PlayerInputs, Seed, Sim, Start, Team, Tuning, Vec2};

    const MAP: (u16, u16) = (64, 64);

    fn view(life: WireLife) -> PlayerView {
        PlayerView {
            id: 1,
            team: Team::CARAVAN,
            pos: Vec2::new(10.5, 10.5),
            facing: Vec2::new(1.0, 0.0),
            life,
            last_attack: None,
            loadout: WireLoadout {
                held: WireWeaponKind::Sword,
                spare: None,
            },
            prompt: None,
            build_kind: WireStructureKind::Watchtower,
        }
    }

    fn alive(interaction: Option<WireInteraction>, refusal: Option<WireBuildRefusal>) -> WireLife {
        WireLife::Alive {
            hp: 100,
            max_hp: 100,
            carried: 0,
            stamina: 100,
            interaction,
            build_refusal: refusal,
            buffs: WireBuffs::NONE,
        }
    }

    fn choosing(kind: WireStructureKind) -> PlayerView {
        PlayerView {
            build_kind: kind,
            ..view(alive(None, None))
        }
    }

    #[test]
    fn the_range_is_previewed_for_two_seconds_after_the_kind_changes_and_not_before_or_after() {
        let me = choosing(WireStructureKind::StunTower);
        let shown = Some((Tile::new(11, 10), WireStructureKind::StunTower));

        assert_eq!(range_preview(&me, &[], MAP, None), None);
        assert_eq!(range_preview(&me, &[], MAP, Some(0.0)), shown);
        assert_eq!(range_preview(&me, &[], MAP, Some(1.9)), shown);
        assert_eq!(range_preview(&me, &[], MAP, Some(2.0)), None);
        assert_eq!(range_preview(&me, &[], MAP, Some(60.0)), None);
    }

    #[test]
    fn the_range_of_the_tower_i_am_building_is_previewed_at_its_tile_for_as_long_as_i_build() {
        let site = WireTower {
            id: 7,
            tile: WireTile { x: 20, y: 21 },
            kind: WireStructureKind::SlowTower,
            state: WireTowerState::Building { progress: 0.5 },
        };
        let other = WireTower {
            id: 8,
            tile: WireTile { x: 30, y: 30 },
            kind: WireStructureKind::HasteTower,
            state: WireTowerState::Building { progress: 0.5 },
        };
        let me = PlayerView {
            // 짓는 동안 E를 눌러도 짓고 있는 포탑의 종류는 바뀌지 않으므로, 고른 종류가 아니라 그 포탑의 종류를 보여 줍니다.
            build_kind: WireStructureKind::StunTower,
            ..view(alive(Some(WireInteraction::Building { tower: 7 }), None))
        };

        for since in [None, Some(0.5), Some(60.0)] {
            assert_eq!(
                range_preview(&me, &[other, site], MAP, since),
                Some((Tile::new(20, 21), WireStructureKind::SlowTower)),
                "{since:?}"
            );
        }
    }

    #[test]
    fn nothing_is_previewed_for_a_player_who_cannot_build_now() {
        let reviving = view(alive(
            Some(WireInteraction::Reviving {
                target: 2,
                progress: 0.5,
            }),
            None,
        ));
        assert_eq!(range_preview(&reviving, &[], MAP, Some(0.5)), None);
        assert_eq!(
            range_preview(&view(WireLife::Dead), &[], MAP, Some(0.5)),
            None
        );
    }

    #[test]
    fn an_idle_player_is_shown_the_tile_in_front() {
        assert_eq!(
            build_preview(&view(alive(None, None)), MAP),
            Some(Tile::new(11, 10))
        );
    }

    #[test]
    fn nothing_is_shown_past_the_edge_of_the_map() {
        let mut at_the_edge = view(alive(None, None));
        at_the_edge.pos = Vec2::new(63.5, 10.5);

        assert_eq!(build_preview(&at_the_edge, MAP), None);
    }

    #[test]
    fn nothing_is_shown_while_the_player_is_busy_refused_downed_or_dead() {
        let refusal = WireBuildRefusal {
            tile: WireTile { x: 11, y: 10 },
            why: WireCannotBuild::TileTaken,
        };
        let downed = WireLife::Downed {
            bleed_secs_left: 20.0,
            revive_progress: None,
        };

        for life in [
            alive(Some(WireInteraction::Building { tower: 1 }), None),
            alive(None, Some(refusal)),
            downed,
            WireLife::Dead,
        ] {
            assert_eq!(build_preview(&view(life), MAP), None, "{life:?}");
        }
    }

    #[test]
    fn the_preview_is_the_tile_the_server_then_builds_on() {
        let aims = [
            (1.0, 0.0),
            (1.0, 1.0),
            (0.0, 1.0),
            (-1.0, 1.0),
            (-1.0, 0.0),
            (-1.0, -1.0),
            (0.0, -1.0),
            (1.0, -1.0),
        ];
        for (x, y) in aims {
            // 건설은 야영지에서 거절되므로, 원정이 시작된 방에서 시험합니다.
            let mut tuning = Tuning::default();
            tuning.camp.start = Start::AtOnce;
            let mut sim = Sim::new(tuning, Seed(1));
            let id = sim.add_player().expect("the room has space");
            let aim = AimDir::new(x, y);
            // 조준을 한 번 보내서 캐릭터가 그 방향을 바라보게 한 뒤, 건설 키를 누르기 전의 모습으로 미리 보기를 구합니다.
            let looking = PlayerInput {
                aim,
                ..PlayerInput::default()
            };
            sim.step(&PlayerInputs::from([(id, looking)]));
            let player = sim.player(id).expect("the player exists");
            let me = PlayerView {
                pos: player.pos,
                facing: player.facing,
                ..view(alive(None, None))
            };
            let shown = build_preview(&me, MAP).expect("the tile is on the map");

            let building = PlayerInput {
                build: true,
                ..looking
            };
            sim.step(&PlayerInputs::from([(id, building)]));

            let site = sim.towers().next().expect("the server started a site");
            assert_eq!(site.tile, shown, "aiming ({x}, {y})");
        }
    }
}
