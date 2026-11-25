//! 짧게 보이는 효과입니다. 플레이어의 부채꼴 공격, 소총의 사격 선, 활과 석궁의 탄환, 감시탑의 사격, 감시탑이 놓일 타일과 건설이 거절된 타일을 선으로 그립니다.
//!
//! 효과는 이벤트가 아니라 상태에서 그립니다. 스냅샷에 마지막 공격 Tick(소총이면 사격 선이 끝난 자리도)과 마지막 사격이 들어 있으므로,
//! 화면에 그리는 Tick이 그 직후이면 효과를 그립니다. 탄환은 스냅샷의 탄환 목록에서 그립니다.
//! 그래서 모든 클라이언트가 같은 효과를 같은 때에 봅니다.

use bevy::prelude::*;
use caravan_client_core::{Frame, PlayerView, ProjectileView, build_preview, effect_progress};
use caravan_protocol::{WireDelivery, WireLife, WireTowerState, WireWeaponKind};
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

/// 공격과 사격 선과 탄환과 사격 효과, 감시탑이 놓일 타일, 내 건설이 거절된 타일을 그립니다.
pub fn draw_effects(frame: Res<CurrentFrame>, scene: Res<Scene>, mut gizmos: Gizmos) {
    let Some(frame) = &frame.0 else {
        return;
    };
    for player in &frame.players {
        let Some(attack) = player.last_attack else {
            continue;
        };
        let progress = effect_progress(frame.tick, attack.tick, config::SWING_TICKS);
        let arc = swing_arc(&scene, attack.weapon);
        if let (Some(progress), Some((radius, half_angle))) = (progress, arc) {
            draw_swing(&mut gizmos, player, progress, radius, half_angle);
        }
        let trace_progress = effect_progress(frame.tick, attack.tick, config::TRACE_TICKS);
        if let (Some(progress), Some(end)) = (trace_progress, attack.trace_end) {
            draw_trace(&mut gizmos, player, end.to_tiles(), progress);
        }
    }
    for projectile in &frame.projectiles {
        draw_projectile(&mut gizmos, projectile);
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

/// 공격한 무기가 부채꼴로 공격하면 부채꼴의 반지름(타일)과 절반 각도(라디안)입니다. 입장 안내의 무기 표에서 구하므로 서버의 수치와 같습니다.
/// 탄환으로 쏘는 무기에는 부채꼴이 없습니다. 판정은 서버가 하고, 이 부채꼴은 그림일 뿐입니다.
fn swing_arc(scene: &Scene, weapon: WireWeaponKind) -> Option<(f32, f32)> {
    match scene.delivery(weapon)? {
        WireDelivery::Arc {
            reach,
            half_angle_deg,
        } => Some((scene.player.radius + reach, half_angle_deg.to_radians())),
        WireDelivery::Projectile | WireDelivery::Hitscan => None,
    }
}

/// 플레이어가 바라보는 방향의 부채꼴을 가슴 높이에 그립니다. 시간이 갈수록 옅어집니다.
fn draw_swing(
    gizmos: &mut Gizmos,
    player: &PlayerView,
    progress: f32,
    radius: f32,
    half_angle: f32,
) {
    let lift = Vec2::new(0.0, config::PLAYER_BODY.y * 0.4);
    let color = config::SWING.with_alpha(1.0 - progress);
    let facing = player.facing.y.atan2(player.facing.x);
    let arc: Vec<Vec2> = (0..=config::SWING_ARC_STEPS)
        .map(|step| {
            let fraction = step as f32 / config::SWING_ARC_STEPS as f32;
            let angle = facing + half_angle * (2.0 * fraction - 1.0);
            let on_ground = Ground::new(
                player.pos.x + radius * angle.cos(),
                player.pos.y + radius * angle.sin(),
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

/// 소총의 사격 선입니다. 쏜 플레이어의 지금 위치에서 서버가 정한 끝까지 가슴 높이에 긋고, 시간이 갈수록 옅어집니다.
fn draw_trace(gizmos: &mut Gizmos, player: &PlayerView, end: Ground, progress: f32) {
    let (from, to) = trace_line(player, end);
    gizmos.line_2d(from, to, config::TRACE.with_alpha(1.0 - progress));
}

/// 사격 선의 화면 좌표(시작, 끝)입니다. 둘 다 가슴 높이입니다. 선의 시작은 서버가 보내지 않고 쏜 플레이어의 위치에서 긋습니다.
fn trace_line(player: &PlayerView, end: Ground) -> (Vec2, Vec2) {
    let lift = Vec2::new(0.0, config::PLAYER_BODY.y * 0.4);
    (
        convert::screen_pos(player.pos) + lift,
        convert::screen_pos(end) + lift,
    )
}

/// 탄환의 선입니다. 머리는 탄환의 위치이고 꼬리는 날아온 쪽으로 `BULLET_TAIL_TILES`만큼 늘어납니다. 둘 다 가슴 높이입니다.
/// 반환하는 값은 화면 좌표의 (꼬리, 머리)입니다.
fn tracer(projectile: &ProjectileView) -> (Vec2, Vec2) {
    let lift = Vec2::new(0.0, config::PLAYER_BODY.y * 0.4);
    let tail_on_ground = Ground::new(
        projectile.pos.x - projectile.facing.x * config::BULLET_TAIL_TILES,
        projectile.pos.y - projectile.facing.y * config::BULLET_TAIL_TILES,
    );
    (
        convert::screen_pos(tail_on_ground) + lift,
        convert::screen_pos(projectile.pos) + lift,
    )
}

fn draw_projectile(gizmos: &mut Gizmos, projectile: &ProjectileView) {
    let (tail, head) = tracer(projectile);
    gizmos.line_2d(tail, head, config::BULLET);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn arrow(x: f32, y: f32, facing_x: f32, facing_y: f32) -> ProjectileView {
        ProjectileView {
            id: 1,
            pos: Ground::new(x, y),
            facing: Ground::new(facing_x, facing_y),
        }
    }

    fn rifleman_at(x: f32, y: f32) -> PlayerView {
        PlayerView {
            id: 1,
            team: caravan_rules::Team::CARAVAN,
            pos: Ground::new(x, y),
            facing: Ground::new(1.0, 0.0),
            life: WireLife::Dead,
            last_attack: None,
            loadout: caravan_protocol::WireLoadout {
                held: WireWeaponKind::Rifle,
                spare: None,
            },
            prompt: None,
        }
    }

    /// 화면 좌표에서 가슴 높이를 빼고 지면 위치로 되돌립니다.
    fn on_ground(screen: Vec2) -> Ground {
        convert::ground_pos(screen - Vec2::new(0.0, config::PLAYER_BODY.y * 0.4))
    }

    #[test]
    fn the_head_of_the_line_is_at_the_projectile_and_the_tail_is_behind_it_along_its_direction() {
        let (tail, head) = tracer(&arrow(10.0, 8.0, 0.0, 1.0));

        let head_on_ground = on_ground(head);
        let tail_on_ground = on_ground(tail);
        assert!((head_on_ground.x - 10.0).abs() < 1e-3 && (head_on_ground.y - 8.0).abs() < 1e-3);
        assert!((tail_on_ground.x - 10.0).abs() < 1e-3, "{tail_on_ground:?}");
        assert!(
            (tail_on_ground.y - (8.0 - config::BULLET_TAIL_TILES)).abs() < 1e-3,
            "{tail_on_ground:?}"
        );
    }

    #[test]
    fn a_diagonal_projectile_has_a_tail_of_the_same_length_on_the_ground() {
        let diagonal = std::f32::consts::FRAC_1_SQRT_2;
        let (tail, head) = tracer(&arrow(10.0, 8.0, diagonal, diagonal));

        let length = (on_ground(head) - on_ground(tail)).length();

        assert!(
            (length - config::BULLET_TAIL_TILES).abs() < 1e-3,
            "{length}"
        );
    }

    #[test]
    fn the_trace_starts_at_the_player_where_he_stands_now_and_ends_where_the_server_says() {
        let player = rifleman_at(10.0, 8.0);

        let (from, to) = trace_line(&player, Ground::new(16.5, 8.0));

        let from_on_ground = on_ground(from);
        let to_on_ground = on_ground(to);
        assert!(
            (from_on_ground.x - 10.0).abs() < 1e-3 && (from_on_ground.y - 8.0).abs() < 1e-3,
            "{from_on_ground:?}"
        );
        assert!(
            (to_on_ground.x - 16.5).abs() < 1e-3 && (to_on_ground.y - 8.0).abs() < 1e-3,
            "{to_on_ground:?}"
        );

        let moved = rifleman_at(10.0, 9.0);
        let (from_after_moving, _) = trace_line(&moved, Ground::new(16.5, 8.0));
        let from_after_moving = on_ground(from_after_moving);
        assert!(
            (from_after_moving.y - 9.0).abs() < 1e-3,
            "the line follows the player: {from_after_moving:?}"
        );
    }
}
