//! 돌진에 맞은 플레이어가 밀려나는 것: 돌진의 방향으로 3타일, 0.3초 동안 조작이 듣지 않는 것, 하던 일이 취소되는 것,
//! 지형과 포탑과 마차와 맵의 끝 앞에서 멈추는 것, 쓰러진 플레이어와 자리를 비운 플레이어와 몬스터가 밀리지 않는 것.

use super::boss::{boss_tuning, hit_boss, hp_of_player, put_boss, skip_the_rest};
use super::charge::{charge_field, run_until_walking, winding_toward};
use super::support::{Scenario, close};
use crate::ids::PlayerId;
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::math::{Rect, Vec2};
use crate::player::{Interaction, Presence};

fn pos_of(s: &Scenario, id: PlayerId) -> Vec2 {
    s.player(id).pos
}

fn is_knocked(s: &Scenario, id: PlayerId) -> bool {
    matches!(
        s.player(id).interaction(),
        Some(Interaction::Knocked { .. })
    )
}

/// 플레이어가 돌진에 맞아서 밀려나기 시작한 Tick까지 진행합니다.
fn run_until_knocked(s: &mut Scenario, id: PlayerId) {
    s.run_until(400, |sim| {
        sim.player(id)
            .is_some_and(|player| matches!(player.interaction(), Some(Interaction::Knocked { .. })))
    });
}

#[test]
fn a_player_in_the_lane_is_hit_once_and_pushed_three_tiles_along_the_charge() {
    let mut s = charge_field(|_| {});
    let player = winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));
    run_until_knocked(&mut s, player);
    let hit_at = pos_of(&s, player);

    s.run_ticks(s.ticks(0.3));

    let at = pos_of(&s, player);
    assert!(
        close(at.x - hit_at.x, 3.0) && close(at.y, hit_at.y),
        "3 tiles along the charge: from {hit_at:?} to {at:?}"
    );
    run_until_walking(&mut s);
    assert_eq!(hp_of_player(&s, player), 100 - 40, "hit once");
}

#[test]
fn a_player_is_pushed_along_the_direction_of_the_lane_whatever_it_is() {
    let mut s = charge_field(|_| {});
    let player = winding_toward(&mut s, (3.0, 3.0), (8.0, 8.0));
    run_until_knocked(&mut s, player);
    let hit_at = pos_of(&s, player);

    s.run_ticks(s.ticks(0.3));

    let moved = pos_of(&s, player) - hit_at;
    let along = Vec2::new(1.0, 1.0).normalize_or_zero();
    assert!(close(moved.length(), 3.0), "3 tiles: {:?}", moved.length());
    assert!(
        close(moved.dot(along), 3.0),
        "along the charge, not across it: {moved:?}"
    );
}

#[test]
fn a_pushed_player_ignores_every_input_for_three_tenths_of_a_second_and_then_obeys_again() {
    let mut s = charge_field(|_| {});
    let player = winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));
    s.give_weapon(player, WeaponKind::Bow);
    run_until_knocked(&mut s, player);
    let kind = s.player(player).build_kind;
    let before = pos_of(&s, player);
    let held = PlayerInput {
        attack: true,
        move_dir: MoveDir::new(0.0, 1.0),
        aim: AimDir::new(1.0, 0.0),
        ..PlayerInput::default()
    };
    s.hold(player, held);
    let shots = s.sim.projectiles().count();

    for tick in 1..=s.ticks(0.3) {
        if tick == 3 {
            s.tap_tower_key(player);
        } else {
            s.run_ticks(1);
        }
        assert!(is_knocked(&s, player), "still pushed on tick {tick}");
        assert_eq!(
            pos_of(&s, player).y,
            before.y,
            "the held move does nothing on tick {tick}"
        );
        assert_eq!(s.sim.projectiles().count(), shots, "no shot on tick {tick}");
    }
    assert_eq!(
        s.player(player).build_kind,
        kind,
        "the key that changes the tower kind does nothing while pushed"
    );

    s.hold(player, held);
    s.run_ticks(1);
    assert!(!is_knocked(&s, player), "free again after 0.3 seconds");
    assert!(pos_of(&s, player).y > before.y, "the held move works now");
    assert!(s.sim.projectiles().count() > shots, "and so does the bow");
}

#[test]
fn a_push_cancels_a_build_in_progress_and_returns_the_cost() {
    let mut s = charge_field(|t| t.structures.watchtower.build_secs = 10.0);
    let builder = s.player_at(14.5, 14.5);
    let stash = s.sim.stash();
    s.hold_build(builder, 1.0, 0.0);
    s.run_ticks(3);
    assert_eq!(s.sim.towers().count(), 1, "a site is going up");
    assert!(s.sim.stash() < stash, "and it was paid for");

    put_boss(&mut s, 7.0, 14.5, 4);
    hit_boss(&mut s, builder, 10);
    s.run_ticks(1);
    skip_the_rest(&mut s);
    run_until_knocked(&mut s, builder);
    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 0, "the site is cancelled");
    assert_eq!(s.sim.stash(), stash, "the cost is back");
    assert_eq!(
        s.player(builder).interaction().map(|_| ()),
        Some(()),
        "the player is pushed, not building"
    );
}

#[test]
fn a_player_pushed_toward_a_wall_a_tower_the_wagon_or_the_edge_stops_in_front_of_it() {
    struct Case {
        name: &'static str,
        boss: (f32, f32),
        player: (f32, f32),
        /// 막힌 곳 앞에서 플레이어의 중심이 멈추는 x입니다. 막힌 곳의 가장자리에서 플레이어의 반지름만큼 앞입니다.
        blocked_at: f32,
    }
    let cases = [
        Case {
            name: "wall",
            boss: (3.0, 10.0),
            player: (8.5, 10.0),
            blocked_at: 10.0,
        },
        Case {
            name: "tower",
            boss: (3.0, 10.5),
            player: (8.5, 10.5),
            blocked_at: 10.0,
        },
        Case {
            name: "wagon",
            boss: (9.0, 18.0),
            player: (15.0, 18.0),
            blocked_at: 17.0,
        },
        Case {
            name: "edge",
            boss: (11.0, 10.0),
            player: (18.0, 10.0),
            blocked_at: 20.0,
        },
    ];
    for case in cases {
        let mut s = charge_field(|t| {
            if case.name == "wall" {
                t.map.obstacles.push(Rect::new(10.0, 9.0, 11.0, 11.0));
                t.map.player_spawns = vec![Vec2::new(5.0, 5.0)];
            }
        });
        if case.name == "tower" {
            s.tower_at(10, 10);
        }
        let player = winding_toward(&mut s, case.boss, case.player);
        s.sim.world_mut().wagon.hp = u32::MAX;
        let radius = s.sim.tuning().player.radius;

        run_until_walking(&mut s);
        s.run_ticks(s.ticks(0.3));

        let at = pos_of(&s, player);
        let stops_at = case.blocked_at - radius;
        assert!(
            at.x <= stops_at + 1e-3 && at.x > stops_at - 0.3,
            "{}: stopped at {}, the blocked side is at {}",
            case.name,
            at.x,
            case.blocked_at
        );
        let mut near = Vec::new();
        s.sim
            .world_mut()
            .tiles
            .blocked_rects_near(at, radius, &mut near);
        assert!(
            near.iter()
                .all(|solid| solid.distance_to(at) >= radius - 1e-3),
            "{}: the player is not inside anything at {at:?}",
            case.name
        );
        assert!(
            at.x >= 0.0 && at.x <= 20.0,
            "{}: inside the map at {at:?}",
            case.name
        );
    }
}

#[test]
fn a_player_who_goes_down_from_the_charge_and_one_who_is_away_or_downed_are_not_pushed() {
    let mut s = charge_field(|t| {
        // 플레이어가 한 번에 쓰러지는 피해입니다.
        boss_tuning(t).charge.strike.player = 200;
    });
    // 노려지는 플레이어는 띠의 끝 쪽에 서서 보스가 8타일 안에서 돌진을 시작하게 합니다.
    let aimed = winding_toward(&mut s, (3.0, 10.0), (10.0, 10.0));
    // 자리에 있는 플레이어가 모두 쓰러지면 원정이 실패로 끝나므로, 띠 밖에서 지켜보는 플레이어가 있습니다.
    s.player_at(1.0, 1.0);
    let downed = s.player_at(7.0, 10.0);
    let away = s.player_at(8.0, 10.2);
    let grunt = s.monster_at(9.0, 9.8);
    s.knock_down(downed);
    s.sim.set_presence(away, Presence::Away);

    run_until_walking(&mut s);
    s.run_ticks(s.ticks(0.3) + 1);

    assert!(
        s.player(aimed).is_downed(),
        "the charge took the aimed player down"
    );
    assert_eq!(pos_of(&s, aimed), Vec2::new(10.0, 10.0), "and did not push");
    assert_eq!(pos_of(&s, downed), Vec2::new(7.0, 10.0));
    assert_eq!(pos_of(&s, away), Vec2::new(8.0, 10.2));
    assert_eq!(hp_of_player(&s, away), 100);
    // 몬스터는 돌진에 맞지 않고 밀려나지도 않습니다. 몸이 겹쳐서 서로 비켜나는 거리만 움직입니다.
    let grunt = s.monster(grunt).expect("alive");
    assert_eq!(grunt.hp, s.sim.tuning().monsters.grunt.max_hp);
    assert!(
        grunt.pos.distance(Vec2::new(9.0, 9.8)) < 0.5,
        "{:?}",
        grunt.pos
    );
}

#[test]
fn a_push_cancels_a_revive_in_progress_and_the_revive_starts_over_after_it() {
    // 벽(타일 (8, 9), (8, 10))이 밀려나는 거리를 짧게 막으므로, 밀려난 뒤에도 쓰러진 동료가 부활시키는 거리 안에 있습니다.
    let mut s = charge_field(|t| {
        t.revive.revive_secs = 10.0;
        t.map.obstacles.push(Rect::new(8.0, 9.0, 9.0, 11.0));
        t.map.player_spawns = vec![Vec2::new(5.0, 5.0)];
    });
    let reviver = winding_toward(&mut s, (3.0, 10.0), (7.0, 10.0));
    let ally = s.player_at(6.5, 10.4);
    s.knock_down(ally);
    s.hold_interact(reviver);
    s.run_ticks(20);
    let before = s
        .sim
        .revive_progress(ally)
        .expect("the revive is under way");

    run_until_knocked(&mut s, reviver);
    s.run_ticks(1);

    assert_eq!(
        s.sim.revive_progress(ally),
        None,
        "no revive while the reviver is pushed"
    );
    s.run_ticks(s.ticks(0.3));
    let after = s
        .sim
        .revive_progress(ally)
        .expect("the revive starts again once the push is over");
    assert!(
        after < before,
        "from the beginning: {after} against {before} before the push"
    );
}
