//! 소총을 확인합니다: 누른 Tick에 조준선 위의 처음 닿는 몬스터 한 마리를 맞히는 것, 막힌 타일 뒤와 사거리 밖의 몬스터를 맞히지 않는 것,
//! 사격 선이 끝난 자리가 마지막 공격에 상태로 남는 것.
//!
//! 사수는 (3, 10)에 서서 동쪽을 쏩니다. 소총의 사거리는 12타일이고 선의 굵기는 0이라서, 몬스터(반지름 0.4)의 중심이 선에서 0.4타일 안에 있으면 맞습니다.

use super::support::{Scenario, close};
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::math::{Rect, Vec2};
use crate::tuning::{Delivery, HitscanTuning, Tuning, WeaponTuning};

fn quiet_field(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        adjust(t);
    })
}

fn fire(x: f32, y: f32) -> PlayerInput {
    PlayerInput {
        aim: AimDir::new(x, y),
        attack: true,
        ..PlayerInput::default()
    }
}

/// 소총을 든 사수가 (3, 10)에 서서 쉬지 않고 동쪽을 쏩니다.
fn rifleman(s: &mut Scenario) -> PlayerId {
    let a = s.player_at(3.0, 10.0);
    s.give_weapon(a, WeaponKind::Rifle);
    s.hold(a, fire(1.0, 0.0));
    a
}

fn hp_of(s: &Scenario, id: MonsterId) -> u32 {
    s.monster(id).expect("the monster is alive").hp
}

/// 플레이어가 마지막으로 쏜 선이 끝난 자리입니다.
fn trace_end_of(s: &Scenario, id: PlayerId) -> Option<Vec2> {
    s.player(id).last_attack.and_then(|attack| attack.trace_end)
}

#[test]
fn the_rifle_hits_the_first_monster_on_the_line_in_the_tick_of_the_click_and_no_other() {
    let mut s = quiet_field(|_| {});
    let a = rifleman(&mut s);
    let near = s.monster_at(7.0, 10.0);
    let behind = s.monster_at(9.0, 10.0);
    let damage = s.sim.tuning().weapons.rifle.damage;

    s.run_ticks(1);

    assert_eq!(hp_of(&s, near), 1000 - damage);
    assert_eq!(hp_of(&s, behind), 1000);
    assert_eq!(s.sim.projectiles().count(), 0, "nothing flies");
    let end = trace_end_of(&s, a).expect("the shot left a line");
    assert!(
        close(end.x, 7.0 - 0.4) && close(end.y, 10.0),
        "the line ends at the near edge of the monster: {end:?}"
    );
}

#[test]
fn a_wall_between_stops_the_shot_and_the_line_ends_at_the_wall() {
    let mut s = quiet_field(|t| t.map.obstacles = vec![Rect::new(6.0, 9.0, 7.0, 11.0)]);
    let a = rifleman(&mut s);
    let behind = s.monster_at(9.0, 10.0);

    s.run_ticks(1);

    assert_eq!(hp_of(&s, behind), 1000);
    let end = trace_end_of(&s, a).expect("the shot left a line");
    assert!(close(end.x, 6.0), "{end:?}");
}

#[test]
fn a_monster_the_line_touches_at_the_same_distance_as_a_wall_is_hit() {
    // 선이 벽에 닿는 거리와 몬스터(반지름 0.25)에 닿는 거리가 같도록, 몬스터를 벽의 안쪽 4분의 1타일에 놓습니다.
    let mut s = quiet_field(|t| {
        t.map.obstacles = vec![Rect::new(6.0, 9.0, 7.0, 11.0)];
        t.monsters.grunt.radius = 0.25;
    });
    let a = rifleman(&mut s);
    let at_the_wall = s.monster_at(6.25, 10.0);
    let damage = s.sim.tuning().weapons.rifle.damage;

    s.run_ticks(1);

    assert_eq!(hp_of(&s, at_the_wall), 1000 - damage);
    let end = trace_end_of(&s, a).expect("the shot left a line");
    assert!(close(end.x, 6.0), "{end:?}");
}

#[test]
fn a_monster_beyond_the_range_is_not_hit_and_the_line_ends_at_the_range() {
    let mut s = quiet_field(|_| {});
    let a = rifleman(&mut s);
    let far = s.monster_at(16.0, 10.0);

    s.run_ticks(1);

    assert_eq!(hp_of(&s, far), 1000);
    let end = trace_end_of(&s, a).expect("the shot left a line");
    assert!(close(end.x, 3.0 + 12.0) && close(end.y, 10.0), "{end:?}");
}

#[test]
fn the_line_follows_the_aim_in_any_direction_and_has_the_length_of_the_range() {
    let mut s = quiet_field(|_| {});
    let a = s.player_at(5.0, 5.0);
    s.give_weapon(a, WeaponKind::Rifle);
    s.hold(a, fire(1.0, 1.0));

    s.run_ticks(1);

    let end = trace_end_of(&s, a).expect("the shot left a line");
    let along = 12.0 * std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        close(end.x, 5.0 + along) && close(end.y, 5.0 + along),
        "{end:?}"
    );
}

#[test]
fn a_line_that_leaves_the_map_ends_at_the_edge() {
    let mut s = quiet_field(|_| {});
    let a = s.player_at(10.0, 10.0);
    s.give_weapon(a, WeaponKind::Rifle);
    s.hold(a, fire(0.0, -1.0));

    s.run_ticks(1);

    let end = trace_end_of(&s, a).expect("the shot left a line");
    assert!(close(end.x, 10.0) && close(end.y, 0.0), "{end:?}");
}

#[test]
fn the_line_has_no_width_so_a_monster_is_hit_only_when_the_line_passes_through_its_body() {
    let mut s = quiet_field(|_| {});
    rifleman(&mut s);
    let inside = s.monster_at(5.0, 10.39);
    let outside = s.monster_at(8.0, 10.41);
    let damage = s.sim.tuning().weapons.rifle.damage;

    s.run_ticks(1);

    assert_eq!(hp_of(&s, inside), 1000 - damage, "0.39 aside is inside 0.4");
    assert_eq!(hp_of(&s, outside), 1000, "0.41 aside is outside 0.4");
}

#[test]
fn the_line_stays_in_the_last_attack_after_the_rifleman_moves_and_a_later_swing_has_none() {
    let mut s = quiet_field(|_| {});
    let a = rifleman(&mut s);
    s.arm(a, WeaponKind::Rifle, Some(WeaponKind::Sword));
    s.monster_at(7.0, 10.0);
    s.run_ticks(1);
    let end = trace_end_of(&s, a).expect("the shot left a line");

    s.hold_move(a, 0.0, 1.0);
    s.run_ticks(5);
    assert_eq!(
        trace_end_of(&s, a),
        Some(end),
        "the line end stays where the shot ended"
    );

    s.hold(a, PlayerInput::default());
    s.tap_switch_key(a);
    s.hold(a, fire(1.0, 0.0));
    s.run_ticks(40);
    assert_eq!(
        s.player(a).last_attack.map(|attack| attack.weapon),
        Some(WeaponKind::Sword)
    );
    assert_eq!(trace_end_of(&s, a), None, "a swing draws no line");
}

#[test]
fn the_rifle_waits_for_its_cooldown_between_shots() {
    let mut s = quiet_field(|_| {});
    rifleman(&mut s);
    let target = s.monster_at(7.0, 10.0);
    let damage = s.sim.tuning().weapons.rifle.damage;
    let cooldown = s.ticks(s.sim.tuning().weapons.rifle.cooldown_secs);

    s.run_ticks(cooldown);
    assert_eq!(hp_of(&s, target), 1000 - damage, "one shot so far");

    s.run_ticks(1);
    assert_eq!(hp_of(&s, target), 1000 - 2 * damage, "the second shot");
}

#[test]
fn the_rifle_shoots_the_damage_and_range_of_its_own_row() {
    let mut s = quiet_field(|t| {
        t.weapons.rifle = WeaponTuning {
            damage: 7,
            cooldown_secs: 0.25,
            delivery: Delivery::Hitscan(HitscanTuning { range: 5.0 }),
            on_hit: None,
            chest_weight: 1,
        };
    });
    let near_rifleman = rifleman(&mut s);
    let in_reach = s.monster_at(7.9, 10.0);
    let far_rifleman = s.player_at(3.0, 14.0);
    s.give_weapon(far_rifleman, WeaponKind::Rifle);
    s.hold(far_rifleman, fire(1.0, 0.0));
    let out_of_reach = s.monster_at(9.0, 14.0);

    s.run_ticks(1);

    assert_eq!(hp_of(&s, in_reach), 1000 - 7);
    assert!(close(trace_end_of(&s, near_rifleman).unwrap().x, 7.9 - 0.4));
    assert_eq!(hp_of(&s, out_of_reach), 1000, "beyond 5 tiles");
    assert!(
        close(trace_end_of(&s, far_rifleman).unwrap().x, 3.0 + 5.0),
        "the line ends at the range of the row"
    );
}

#[test]
fn two_rifles_that_fire_in_the_same_tick_both_hit_the_same_monster() {
    let mut s = quiet_field(|_| {});
    rifleman(&mut s);
    let b = s.player_at(3.0, 10.2);
    s.give_weapon(b, WeaponKind::Rifle);
    s.hold(b, fire(1.0, 0.0));
    let target = s.monster_at(7.0, 10.1);
    let damage = s.sim.tuning().weapons.rifle.damage;

    s.run_ticks(1);

    assert_eq!(hp_of(&s, target), 1000 - 2 * damage);
}
