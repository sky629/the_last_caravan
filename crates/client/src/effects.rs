//! 짧게 보이는 효과입니다. 플레이어의 근접 공격, 감시탑의 사격, 감시탑이 놓일 타일과 건설이 거절된 타일을 선으로 그립니다.
//!
//! 효과는 이벤트가 아니라 상태에서 그립니다. 스냅샷에 마지막 공격 Tick과 마지막 사격이 들어 있으므로,
//! 화면에 그리는 Tick이 그 직후이면 효과를 그립니다. 그래서 모든 클라이언트가 같은 효과를 같은 때에 봅니다.

use bevy::prelude::*;
use caravan_client_core::{Frame, PlayerView, build_preview, effect_progress};
use caravan_protocol::{WireLife, WireTowerState};
use caravan_rules::{Tile, Vec2 as Ground};

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::view::Scene;

/// 시작할 때 한 번 실행해서 효과 선을 굵게 합니다.
pub fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.line.width = config::EFFECT_LINE_WIDTH;
}

/// 공격과 사격 효과, 감시탑이 놓일 타일, 내 건설이 거절된 타일을 그립니다.
pub fn draw_effects(frame: Res<CurrentFrame>, scene: Res<Scene>, mut gizmos: Gizmos) {
    let Some(frame) = &frame.0 else {
        return;
    };
    for player in &frame.players {
        let progress = player
            .last_attack_tick
            .and_then(|tick| effect_progress(frame.tick, tick, config::SWING_TICKS));
        if let Some(progress) = progress {
            draw_swing(&mut gizmos, player, progress);
        }
    }
    draw_tower_shots(&mut gizmos, frame);
    let Some(me) = frame.player(scene.you) else {
        return;
    };
    // 거절된 타일의 테두리는 이유를 알리는 HUD 문구와 함께 보입니다. 미리 보기는 거절 이유가 없을 때만 나옵니다.
    if let WireLife::Alive {
        build_refusal: Some(refusal),
        ..
    } = me.life
    {
        draw_tile_outline(&mut gizmos, refusal.tile.to_tile(), config::REFUSAL);
    }
    if let Some(tile) = build_preview(me, scene.map_size) {
        draw_tile_outline(&mut gizmos, tile, config::PREVIEW);
    }
}

/// 타일의 테두리를 지면에 그립니다.
fn draw_tile_outline(gizmos: &mut Gizmos, tile: Tile, color: Color) {
    let rect = tile.rect();
    let corners = [
        Ground::new(rect.min.x, rect.min.y),
        Ground::new(rect.max.x, rect.min.y),
        Ground::new(rect.max.x, rect.max.y),
        Ground::new(rect.min.x, rect.max.y),
        Ground::new(rect.min.x, rect.min.y),
    ];
    gizmos.linestrip_2d(corners.map(convert::screen_pos), color);
}

/// 플레이어가 바라보는 방향의 부채꼴을 가슴 높이에 그립니다. 시간이 갈수록 옅어집니다.
fn draw_swing(gizmos: &mut Gizmos, player: &PlayerView, progress: f32) {
    let lift = Vec2::new(0.0, config::PLAYER_BODY.y * 0.4);
    let color = config::SWING.with_alpha(1.0 - progress);
    let facing = player.facing.y.atan2(player.facing.x);
    let arc: Vec<Vec2> = (0..=config::SWING_ARC_STEPS)
        .map(|step| {
            let fraction = step as f32 / config::SWING_ARC_STEPS as f32;
            let angle = facing + config::SWING_HALF_ANGLE * (2.0 * fraction - 1.0);
            let on_ground = Ground::new(
                player.pos.x + config::SWING_REACH_TILES * angle.cos(),
                player.pos.y + config::SWING_REACH_TILES * angle.sin(),
            );
            convert::screen_pos(on_ground) + lift
        })
        .collect();
    let center = convert::screen_pos(player.pos) + lift;
    if let (Some(first), Some(last)) = (arc.first().copied(), arc.last().copied()) {
        gizmos.line_2d(center, first, color);
        gizmos.line_2d(center, last, color);
    }
    gizmos.linestrip_2d(arc, color);
}

/// 서 있는 감시탑이 마지막으로 쏜 몬스터까지 선을 긋습니다. 그 몬스터가 이미 사라졌으면 그리지 않습니다.
fn draw_tower_shots(gizmos: &mut Gizmos, frame: &Frame) {
    for tower in &frame.towers {
        let WireTowerState::Standing {
            last_shot: Some(shot),
            ..
        } = tower.state
        else {
            continue;
        };
        let Some(progress) = effect_progress(frame.tick, shot.tick, config::SHOT_TICKS) else {
            continue;
        };
        let target = frame
            .monsters
            .iter()
            .find(|monster| monster.id == shot.target);
        let Some(target) = target else {
            continue;
        };
        let center = tower.tile.to_tile().center();
        let from = convert::screen_pos(center) + Vec2::new(0.0, config::TOWER_HEIGHT);
        let to = convert::screen_pos(target.pos) + Vec2::new(0.0, config::MONSTER_BODY.y * 0.5);
        gizmos.line_2d(from, to, config::SHOT.with_alpha(1.0 - progress));
    }
}
