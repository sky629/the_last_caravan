//! 건설 키를 누르기 전에 감시탑이 놓일 타일을 알려 주는 계산입니다.
//!
//! 타일을 정하는 함수는 서버가 건설을 시작할 때 쓰는 `caravan_rules::front_tile` 하나이고, 여기서도 같은 함수를 부릅니다.
//! 놓을 수 있는지는 판정하지 않습니다. 판정은 서버만 하고, 거절되면 이유와 타일이 스냅샷으로 옵니다.
//! 입력 메시지에는 타일의 좌표가 없으므로, 이 값은 화면에 그리는 데만 씁니다.

use caravan_protocol::WireLife;
use caravan_rules::{Tile, front_tile};

use crate::interp::PlayerView;

/// `me`가 지금 건설 키를 누르면 감시탑이 놓일 타일입니다. 맵의 크기는 `map_size`(타일 수)입니다.
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

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::{WireBuildRefusal, WireCannotBuild, WireInteraction, WireTile};
    use caravan_rules::{AimDir, PlayerInput, PlayerInputs, Sim, Team, Tuning, Vec2};

    const MAP: (u16, u16) = (64, 64);

    fn view(life: WireLife) -> PlayerView {
        PlayerView {
            id: 1,
            team: Team::CARAVAN,
            pos: Vec2::new(10.5, 10.5),
            facing: Vec2::new(1.0, 0.0),
            life,
            last_attack_tick: None,
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
        }
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
            let mut sim = Sim::new(Tuning::default());
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
