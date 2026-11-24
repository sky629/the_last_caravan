//! 활의 탄환을 확인합니다: 쏘는 방향과 속도, 처음 닿는 몬스터 한 마리, 막힌 타일과 사거리에서 사라지는 것, 플레이어를 지나가는 것,
//! 한 Tick에 얇은 몬스터를 건너뛰지 않는 명중 판정.
//!
//! 시나리오는 20 x 20 맵이고 마차는 (17, 17)에서 (19, 19)까지입니다. 활은 Tick마다 0.7타일을 날아가고(초당 14타일, 20 Hz),
//! 사거리는 9타일이고, 탄환의 반지름은 0.15타일입니다. 그래서 몬스터(반지름 0.4)의 중심이 탄환이 지나가는 선에서 0.55타일 안에 있으면 맞습니다.
//! 사수는 (3, 10)에 서서 동쪽을 조준하고, 몬스터는 한 번에 죽지 않게 HP를 크게 하고 제자리에 서 있게 합니다.
//! 기대하는 피해량과 속도는 수치표의 활 행에서 구합니다.

use super::support::{Scenario, close};
use crate::expedition::{Outcome, Phase};
use crate::ids::{MonsterId, PlayerId, ProjectileId};
use crate::input::{AimDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::math::{Rect, Vec2};
use crate::projectile::{Flight, Projectile};
use crate::tuning::{Delivery, ProjectileTuning, Tuning};
use crate::world::Tile;

fn bow_spec(s: &Scenario) -> ProjectileTuning {
    let Delivery::Projectile(spec) = s.sim.tuning().weapons.bow.delivery else {
        panic!("the bow shoots projectiles");
    };
    spec
}

fn bow_damage(s: &Scenario) -> u32 {
    s.sim.tuning().weapons.bow.damage
}

/// 한 Tick에 탄환이 날아가는 거리입니다.
fn step_of(s: &Scenario) -> f32 {
    bow_spec(s).speed * s.sim.tuning().dt()
}

/// 몬스터가 한 번에 죽지 않고 제자리에 서 있는 시나리오입니다. `adjust`로 수치를 더 바꿉니다.
fn quiet_field(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    Scenario::with_tuning(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.speed = 0.0;
        t.monsters.grunt.separation_speed = 0.0;
        adjust(t);
    })
}

fn shoot(x: f32, y: f32) -> PlayerInput {
    PlayerInput {
        aim: AimDir::new(x, y),
        attack: true,
        ..PlayerInput::default()
    }
}

/// (3, 10)에 선 사수가 동쪽을 향해 쉬지 않고 쏩니다.
fn archer_in(mut s: Scenario) -> (Scenario, PlayerId) {
    let a = s.player_at(3.0, 10.0);
    s.give_weapon(a, WeaponKind::Bow);
    s.hold(a, shoot(1.0, 0.0));
    (s, a)
}

fn archer() -> (Scenario, PlayerId) {
    archer_in(quiet_field(|_| {}))
}

/// 조준은 그대로 두고 공격 버튼만 뗍니다.
fn stop_shooting(s: &mut Scenario, id: PlayerId) {
    s.hold(
        id,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
}

/// 한 번만 쏘고 `ticks` Tick을 더 진행합니다. 처음 Tick에 탄환이 나가고, 쿨다운 때문에 둘째 탄환은 나가지 않습니다.
fn shoot_once_and_run(s: &mut Scenario, id: PlayerId, ticks: u32) {
    s.run_ticks(1);
    stop_shooting(s, id);
    s.run_ticks(ticks);
}

fn hp_of(s: &Scenario, id: MonsterId) -> u32 {
    s.monster(id).expect("the monster is alive").hp
}

fn only_projectile(s: &Scenario) -> &Projectile {
    let mut all = s.sim.projectiles();
    let first = all.next().expect("one projectile");
    assert!(all.next().is_none(), "exactly one projectile");
    first
}

#[test]
fn a_shot_makes_a_projectile_that_flies_in_the_aimed_direction_at_the_table_speed() {
    let (mut s, a) = archer();
    let step = step_of(&s);

    s.run_ticks(1);
    stop_shooting(&mut s, a);
    let shot = only_projectile(&s);
    assert_eq!(shot.id, ProjectileId(1));
    assert!(shot.is_flying());
    assert!(close(shot.pos.x, 3.0 + step) && close(shot.pos.y, 10.0));
    assert_eq!(shot.dir, Vec2::new(1.0, 0.0));
    assert_eq!(s.player(a).last_attack_tick, Some(1));

    s.run_ticks(4);
    let shot = only_projectile(&s);
    assert_eq!(shot.id, ProjectileId(1), "the same projectile keeps flying");
    assert!(close(shot.pos.x, 3.0 + 5.0 * step), "{:?}", shot.pos);
}

#[test]
fn a_projectile_flies_where_the_player_aims_not_only_east() {
    let (mut s, a) = archer();
    let step = step_of(&s);
    s.hold(a, shoot(0.0, -1.0));

    s.run_ticks(1);
    stop_shooting(&mut s, a);
    s.run_ticks(1);

    let shot = only_projectile(&s);
    assert!(close(shot.pos.x, 3.0), "{:?}", shot.pos);
    assert!(close(shot.pos.y, 10.0 - 2.0 * step), "{:?}", shot.pos);

    let (mut s, a) = archer();
    s.hold(a, shoot(1.0, 1.0));
    s.run_ticks(1);
    let diagonal = std::f32::consts::FRAC_1_SQRT_2 * step;
    let shot = only_projectile(&s);
    assert!(close(shot.pos.x, 3.0 + diagonal), "{:?}", shot.pos);
    assert!(close(shot.pos.y, 10.0 + diagonal), "{:?}", shot.pos);
}

#[test]
fn the_cooldown_spaces_the_shots() {
    let (mut s, _) = archer();

    s.run_ticks(s.ticks(1.0));

    // 쿨다운 0.4초는 8 Tick이므로, 1초 동안 첫 Tick과 9번째와 17번째 Tick에 쏩니다.
    let newest = s.sim.projectiles().map(|shot| shot.id).max();
    assert_eq!(newest, Some(ProjectileId(3)));
}

#[test]
fn a_projectile_damages_the_first_monster_it_touches_and_then_ends() {
    let (mut s, a) = archer();
    let near = s.monster_at(6.0, 10.0);
    let behind = s.monster_at(7.0, 10.0);
    let damage = bow_damage(&s);

    shoot_once_and_run(&mut s, a, 3);

    assert_eq!(hp_of(&s, near), 1000 - damage);
    assert_eq!(hp_of(&s, behind), 1000, "only the first one is hit");
    let shot = only_projectile(&s);
    assert_eq!(shot.flight, Flight::Landed);
    assert!(close(shot.pos.x, 6.0 - 0.55), "{:?}", shot.pos);

    s.run_ticks(1);
    assert_eq!(s.sim.projectiles().count(), 0, "gone a tick later");
    assert_eq!(hp_of(&s, near), 1000 - damage, "it did not hit twice");
}

#[test]
fn a_monster_beside_the_line_is_not_hit_and_one_inside_the_width_is() {
    let (mut s, a) = archer();
    let too_far_aside = s.monster_at(5.0, 10.7);
    let just_inside = s.monster_at(7.0, 10.5);
    let damage = bow_damage(&s);

    shoot_once_and_run(&mut s, a, 6);

    assert_eq!(
        hp_of(&s, too_far_aside),
        1000,
        "0.7 aside is wider than 0.4 + 0.15"
    );
    assert_eq!(hp_of(&s, just_inside), 1000 - damage, "0.5 aside is inside");
}

#[test]
fn a_projectile_cannot_jump_over_a_thin_monster_within_one_tick() {
    // 한 Tick에 9타일을 날아가는 탄환과 지름이 0.1인 몬스터입니다. 이동한 뒤의 자리만 검사하면 지나칩니다.
    let (mut s, a) = archer_in(quiet_field(|t| {
        t.monsters.grunt.radius = 0.05;
        if let Delivery::Projectile(spec) = &mut t.weapons.bow.delivery {
            spec.speed = 200.0;
            spec.radius = 0.05;
        }
    }));
    let thin = s.monster_at(7.0, 10.0);
    let damage = bow_damage(&s);
    assert!(
        step_of(&s) >= bow_spec(&s).range,
        "one tick flies the whole range"
    );

    shoot_once_and_run(&mut s, a, 0);

    assert_eq!(hp_of(&s, thin), 1000 - damage);
    assert!(close(only_projectile(&s).pos.x, 7.0 - 0.1));
}

#[test]
fn a_projectile_hits_a_monster_that_overlaps_the_player_in_the_tick_it_is_shot() {
    let (mut s, a) = archer();
    let close_by = s.monster_at(3.2, 10.0);
    let damage = bow_damage(&s);

    shoot_once_and_run(&mut s, a, 0);

    assert_eq!(hp_of(&s, close_by), 1000 - damage);
    assert_eq!(only_projectile(&s).flight, Flight::Landed);
}

#[test]
fn a_monster_behind_the_player_is_not_hit() {
    let (mut s, a) = archer();
    let behind = s.monster_at(1.5, 10.0);

    shoot_once_and_run(&mut s, a, 3);

    assert_eq!(hp_of(&s, behind), 1000);
    assert!(only_projectile(&s).is_flying());
}

/// 사수와 막는 것 뒤의 몬스터 하나를 놓고 한 번 쏜 뒤, 탄환이 끝난 자리의 x를 돌려줍니다. 몬스터는 다치지 않아야 합니다.
fn shoot_at_a_blocker(mut s: Scenario, archer_at: Vec2, monster_at: Vec2) -> f32 {
    let a = s.player_at(archer_at.x, archer_at.y);
    s.give_weapon(a, WeaponKind::Bow);
    s.hold(a, shoot(1.0, 0.0));
    let behind = s.monster_at(monster_at.x, monster_at.y);

    shoot_once_and_run(&mut s, a, 0);
    s.run_until(30, |sim| sim.projectiles().any(|shot| !shot.is_flying()));

    assert_eq!(hp_of(&s, behind), 1000, "the blocker protects the monster");
    only_projectile(&s).pos.x
}

#[test]
fn an_obstacle_stops_the_projectile() {
    let s = quiet_field(|t| t.map.obstacles = vec![Rect::new(8.0, 9.0, 9.0, 11.0)]);

    let x = shoot_at_a_blocker(s, Vec2::new(3.0, 10.0), Vec2::new(10.0, 10.0));

    assert!(close(x, 8.0 - 0.15), "stopped at the wall, at {x}");
}

#[test]
fn a_tower_stops_the_projectile() {
    // 감시탑의 피해량을 0으로 해서 감시탑의 사격이 결과에 섞이지 않게 합니다.
    let mut s = quiet_field(|t| t.structures.watchtower.damage = 0);
    s.tower_at(8, 10);

    let x = shoot_at_a_blocker(s, Vec2::new(3.0, 10.5), Vec2::new(10.0, 10.5));

    assert!(close(x, 8.0 - 0.15), "stopped at the tower, at {x}");
}

#[test]
fn the_wagon_stops_the_projectile() {
    let s = quiet_field(|_| {});

    let x = shoot_at_a_blocker(s, Vec2::new(12.0, 18.0), Vec2::new(19.6, 18.0));

    assert!(close(x, 17.0 - 0.15), "stopped at the wagon, at {x}");
}

#[test]
fn a_gather_node_stops_the_projectile() {
    let s = quiet_field(|t| t.map.nodes = vec![Tile::new(8, 10)]);

    let x = shoot_at_a_blocker(s, Vec2::new(3.0, 10.5), Vec2::new(10.0, 10.5));

    assert!(close(x, 8.0 - 0.15), "stopped at the node, at {x}");
}

#[test]
fn a_tower_that_is_still_being_built_stops_the_projectile() {
    let mut s = quiet_field(|_| {});
    let builder = s.player_at(13.5, 15.5);
    s.hold_build(builder, 0.0, -1.0);
    s.run_until(5, |sim| sim.towers().count() == 1);
    assert!(
        s.sim.towers().all(|tower| !tower.is_standing()),
        "the tower is not finished"
    );
    let site = s.tower().tile;
    let y = f32::from(site.y) + 0.5;

    let x = shoot_at_a_blocker(s, Vec2::new(6.0, y), Vec2::new(15.0, y));

    assert!(
        close(x, f32::from(site.x) - 0.15),
        "stopped at the tower under construction, at {x}"
    );
}

#[test]
fn a_projectile_that_flies_past_its_range_ends_without_hurting_anyone() {
    let (mut s, a) = archer();
    let range = bow_spec(&s).range;
    let out_of_reach = s.monster_at(3.0 + range + 1.0, 10.0);

    shoot_once_and_run(&mut s, a, 20);

    assert_eq!(hp_of(&s, out_of_reach), 1000);
    assert_eq!(s.sim.projectiles().count(), 0, "it ended and was removed");
}

#[test]
fn a_projectile_ends_exactly_at_its_range() {
    let (mut s, a) = archer();
    let range = bow_spec(&s).range;
    s.run_ticks(1);
    stop_shooting(&mut s, a);

    s.run_until(30, |sim| sim.projectiles().any(|shot| !shot.is_flying()));

    let shot = only_projectile(&s);
    assert!(close(shot.pos.x, 3.0 + range), "ended at {:?}", shot.pos);
    assert!(close(shot.pos.y, 10.0));
}

#[test]
fn a_projectile_that_leaves_the_map_ends_at_its_edge() {
    let (mut s, a) = archer();
    s.sim.world_mut().players.get_mut(&a).unwrap().pos = Vec2::new(12.0, 10.0);
    let radius = bow_spec(&s).radius;
    s.run_ticks(1);
    stop_shooting(&mut s, a);

    s.run_until(30, |sim| sim.projectiles().any(|shot| !shot.is_flying()));

    let shot = only_projectile(&s);
    assert!(
        close(shot.pos.x, 20.0 - radius),
        "ended at the map edge, at {:?}",
        shot.pos
    );
}

#[test]
fn a_player_pressed_against_a_wall_can_shoot_along_it() {
    let (mut s, a) = archer_in(quiet_field(|t| {
        t.map.obstacles = vec![Rect::new(2.0, 5.0, 14.0, 6.0)];
    }));
    // 플레이어의 가장자리가 벽에 닿는 자리입니다. 벽의 위쪽 변은 y = 6입니다.
    let radius = s.sim.tuning().player.radius;
    s.sim.world_mut().players.get_mut(&a).unwrap().pos = Vec2::new(3.0, 6.0 + radius);
    let range = bow_spec(&s).range;
    s.run_ticks(1);
    stop_shooting(&mut s, a);

    s.run_until(30, |sim| sim.projectiles().any(|shot| !shot.is_flying()));

    let shot = only_projectile(&s);
    assert!(
        close(shot.pos.x, 3.0 + range),
        "flew the whole range along the wall, ended at {:?}",
        shot.pos
    );
}

#[test]
fn a_projectile_passes_through_players_without_hurting_them() {
    let (mut s, a) = archer();
    let ally = s.player_at(5.0, 10.0);
    let target = s.monster_at(7.0, 10.0);
    let damage = bow_damage(&s);

    shoot_once_and_run(&mut s, a, 5);

    assert_eq!(s.player(ally).hp(), 100, "the ally in the way is unhurt");
    assert_eq!(s.player(a).hp(), 100, "the archer is unhurt");
    assert_eq!(
        hp_of(&s, target),
        1000 - damage,
        "the monster behind is hit"
    );
}

#[test]
fn two_projectiles_that_reach_one_monster_in_the_same_tick_both_count() {
    let (mut s, a) = archer();
    let b = s.player_at(3.0, 10.2);
    s.give_weapon(b, WeaponKind::Bow);
    s.hold(b, shoot(1.0, 0.0));
    let target = s.monster_at(6.0, 10.1);
    let damage = bow_damage(&s);

    s.run_ticks(1);
    stop_shooting(&mut s, a);
    stop_shooting(&mut s, b);
    s.run_ticks(3);

    assert_eq!(hp_of(&s, target), 1000 - 2 * damage);
}

#[test]
fn a_projectile_flies_on_after_the_player_who_shot_it_is_gone() {
    let (mut s, a) = archer();
    let target = s.monster_at(6.0, 10.0);
    let damage = bow_damage(&s);
    s.run_ticks(1);
    stop_shooting(&mut s, a);

    s.sim.remove_player(a);
    s.run_ticks(4);

    assert_eq!(hp_of(&s, target), 1000 - damage);
}

#[test]
fn a_monster_killed_by_the_projectile_disappears() {
    let damage = Scenario::open_field().sim.tuning().weapons.bow.damage;
    let (mut s, a) = archer_in(quiet_field(|t| t.monsters.grunt.max_hp = damage));
    let target = s.monster_at(6.0, 10.0);

    shoot_once_and_run(&mut s, a, 3);

    assert!(s.monster(target).is_none());
    assert_eq!(s.sim.monsters().count(), 0);
}

#[test]
fn a_downed_player_does_not_shoot() {
    let (mut s, a) = archer();
    s.knock_down(a);

    s.run_ticks(20);

    assert_eq!(s.sim.projectiles().count(), 0);
    assert_eq!(s.player(a).last_attack_tick, None);
}

#[test]
fn a_new_expedition_clears_the_projectiles() {
    let (mut s, a) = archer();
    s.run_ticks(1);
    stop_shooting(&mut s, a);
    assert_eq!(s.flying(), 1);
    s.sim.set_phase(Phase::Over {
        outcome: Outcome::Defeat,
        ticks_left: 1,
    });

    s.run_until(5, |sim| sim.expedition_number() == 2);

    assert_eq!(s.sim.projectiles().count(), 0);
}

#[test]
fn the_projectile_keeps_the_damage_it_was_shot_with() {
    let (mut s, a) = archer();
    let target = s.monster_at(6.0, 10.0);
    let bow_damage = bow_damage(&s);
    s.run_ticks(1);
    stop_shooting(&mut s, a);

    s.give_weapon(a, WeaponKind::Sword);
    s.run_ticks(3);

    assert_eq!(
        hp_of(&s, target),
        1000 - bow_damage,
        "the arrow is not changed by the weapon in hand now"
    );
}
