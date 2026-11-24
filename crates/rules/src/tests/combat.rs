//! 플레이어의 근접 공격을 확인합니다.

use super::support::Scenario;
use crate::ids::MonsterId;
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::math::Vec2;

/// 몬스터가 한 번에 죽지 않도록 HP를 크게 한 시나리오입니다.
fn with_tough_monsters() -> Scenario {
    Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 1000)
}

fn attack_east() -> PlayerInput {
    PlayerInput {
        aim: AimDir::new(1.0, 0.0),
        attack: true,
        ..PlayerInput::default()
    }
}

fn hp_of(s: &Scenario, id: MonsterId) -> u32 {
    s.monster(id).expect("the monster is alive").hp
}

#[test]
fn a_swing_damages_a_monster_in_front_of_the_player() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let monster = s.monster_at(10.9, 10.0);
    s.hold(a, attack_east());

    s.run_ticks(1);

    let damage = s.sim.tuning().player.attack_damage;
    assert_eq!(hp_of(&s, monster), 1000 - damage);
    assert_eq!(s.player(a).last_attack_tick, Some(1));
}

#[test]
fn a_swing_hits_every_hostile_monster_in_the_arc() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let front = s.monster_at(10.9, 10.0);
    let front_left = s.monster_at(10.9, 10.4);
    let behind = s.monster_at(9.1, 10.0);
    s.hold(a, attack_east());

    s.run_ticks(1);

    assert_eq!(hp_of(&s, front), 975);
    assert_eq!(hp_of(&s, front_left), 975);
    assert_eq!(
        hp_of(&s, behind),
        1000,
        "behind the player is outside the arc"
    );
}

#[test]
fn a_monster_beyond_the_reach_is_not_hit() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let near = s.monster_at(11.7, 10.0);
    let far = s.monster_at(12.5, 10.0);
    s.hold(a, attack_east());

    s.run_ticks(1);

    // 반지름 0.35와 0.4를 빼면 근접 공격 거리 1.0 안에 들어오는 거리는 1.75까지입니다.
    assert_eq!(hp_of(&s, near), 975);
    assert_eq!(hp_of(&s, far), 1000);
}

#[test]
fn a_monster_outside_the_arc_is_not_hit() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let inside = s.monster_at(10.9, 10.4);
    let outside = s.monster_at(10.2, 10.9);
    s.hold(a, attack_east());

    s.run_ticks(1);

    assert_eq!(hp_of(&s, inside), 975);
    assert_eq!(
        hp_of(&s, outside),
        1000,
        "almost straight north is outside +-60 degrees"
    );
}

#[test]
fn the_cooldown_spaces_the_swings() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let monster = s.monster_at(10.9, 10.0);
    s.hold(a, attack_east());

    s.run_ticks(s.ticks(1.0));

    // 쿨다운 0.5초는 10 Tick이므로, 1초 동안 첫 Tick과 11번째 Tick에 휘두릅니다.
    assert_eq!(hp_of(&s, monster), 1000 - 2 * 25);
}

#[test]
fn a_player_who_is_not_holding_attack_does_not_swing() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let monster = s.monster_at(10.9, 10.0);
    s.hold(
        a,
        PlayerInput {
            attack: false,
            ..attack_east()
        },
    );

    s.run_ticks(20);

    assert_eq!(hp_of(&s, monster), 1000);
    assert_eq!(s.player(a).last_attack_tick, None);
}

#[test]
fn two_players_hitting_the_same_monster_both_count() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let b = s.player_at(10.0, 11.0);
    let monster = s.monster_at(10.7, 10.5);
    s.hold(a, attack_east());
    s.hold(b, attack_east());

    s.run_ticks(1);

    assert_eq!(hp_of(&s, monster), 1000 - 2 * 25, "C5");
}

#[test]
fn a_monster_at_zero_hp_disappears() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);
    let monster = s.monster_at(10.9, 10.0);
    s.hold(a, attack_east());

    // 몬스터 HP 60은 피해 25를 세 번 맞아야 0이 됩니다. 세 번째 공격은 21번째 Tick입니다.
    s.run_ticks(20);
    assert_eq!(hp_of(&s, monster), 10);
    s.run_ticks(1);
    assert!(s.monster(monster).is_none());
    assert_eq!(s.sim.monsters().count(), 0);
}

#[test]
fn a_monster_killed_this_tick_still_lands_its_own_hit() {
    let mut s = Scenario::with_tuning(|t| t.monsters.grunt.max_hp = 25);
    let a = s.player_at(10.0, 10.0);
    s.monster_at(10.9, 10.0);
    s.hold(a, attack_east());

    s.run_ticks(1);

    assert_eq!(s.sim.monsters().count(), 0);
    assert_eq!(s.player(a).hp(), 100 - 10);
}

#[test]
fn the_swing_finds_exactly_the_monsters_in_reach_across_many_grid_cells() {
    // 몬스터 64마리를 여러 격자 칸에 걸쳐 늘어놓고, 한 번 휘둘러서 맞은 몬스터가
    // 거리와 각도를 하나씩 직접 계산한 결과와 같은지 확인합니다.
    let mut s = with_tough_monsters();
    let a = s.player_at(9.4, 9.1);
    let facing = AimDir::new(1.0, 0.4);
    s.hold(
        a,
        PlayerInput {
            aim: facing,
            attack: true,
            ..PlayerInput::default()
        },
    );
    let mut placed = Vec::new();
    for column in 0..8 {
        for row in 0..8 {
            let at = Vec2::new(8.0 + 0.55 * column as f32, 8.0 + 0.55 * row as f32);
            placed.push((s.monster_at(at.x, at.y), at));
        }
    }

    s.run_ticks(1);

    let reach = s.sim.tuning().player.attack_reach;
    let radii = s.sim.tuning().player.radius + s.sim.tuning().monsters.grunt.radius;
    let arc_cos = s.sim.tuning().player.attack_arc_cos();
    let origin = Vec2::new(9.4, 9.1);
    let facing = facing.get().unwrap();
    let (mut hit_count, mut miss_count) = (0, 0);
    for (id, at) in placed {
        let offset = at - origin;
        let gap = offset.length() - radii;
        let cos = facing.dot(offset) / offset.length();
        let margin = 0.02;
        let clearly_in = gap < reach - margin && cos > arc_cos + margin;
        let clearly_out = gap > reach + margin || cos < arc_cos - margin;
        if clearly_in {
            assert_eq!(hp_of(&s, id), 975, "{at:?} should be hit");
            hit_count += 1;
        } else if clearly_out {
            assert_eq!(hp_of(&s, id), 1000, "{at:?} should not be hit");
            miss_count += 1;
        }
    }
    assert!(
        hit_count >= 5 && miss_count >= 30,
        "{hit_count} hit, {miss_count} missed"
    );
}

#[test]
fn a_move_made_during_the_swing_does_not_change_the_swing_direction() {
    let mut s = with_tough_monsters();
    let a = s.player_at(10.0, 10.0);
    let behind = s.monster_at(9.1, 10.0);
    s.hold(
        a,
        PlayerInput {
            move_dir: MoveDir::new(1.0, 0.0),
            aim: AimDir::new(-1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    );

    s.run_ticks(1);

    assert_eq!(
        hp_of(&s, behind),
        975,
        "the player aims west while walking east"
    );
}
