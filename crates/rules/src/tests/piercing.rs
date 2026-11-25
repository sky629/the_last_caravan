//! 여러 마리를 맞힐 수 있는 탄환을 확인합니다: 닿는 순서대로 맞히고 정해진 수만큼 맞히면 사라지는 것, 같은 몬스터를 두 번 맞히지 않는 것,
//! 막힌 타일과 사거리의 끝에서 사라지는 것. 한 마리만 맞히는 탄환(활)은 `projectiles.rs`가 확인합니다.
//!
//! 시나리오는 `projectiles.rs`와 같고, 사수는 (3, 10)에 서서 동쪽을 쏩니다. 활의 행에서 맞힐 수 있는 수(`pierce`)만 바꿔서 쓰므로
//! 이 파일은 무기의 수치가 아니라 탄환의 규칙을 확인합니다. 활의 탄환은 Tick마다 0.7타일을 날아가고 사거리는 9타일이며,
//! 몬스터(반지름 0.4)와 탄환(반지름 0.15)은 중심이 0.55타일 안으로 지나갈 때 닿습니다.

use super::support::{Scenario, close};
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::math::Rect;
use crate::projectile::{Flight, Projectile};
use crate::tuning::{Delivery, ProjectileTuning, Tuning};

fn bow_damage(s: &Scenario) -> u32 {
    s.sim.tuning().weapons.bow.damage
}

/// 활의 탄환이 맞힐 수 있는 수를 `pierce`로 바꾸는 수치입니다.
fn bow_pierces(t: &mut Tuning, pierce: u32) {
    let Delivery::Projectile(spec) = &mut t.weapons.bow.delivery else {
        panic!("the bow shoots projectiles");
    };
    spec.pierce = pierce;
}

/// 몬스터가 한 번에 죽지 않고 제자리에 서 있게 합니다.
fn still_monsters(t: &mut Tuning) {
    t.monsters.grunt.max_hp = 1000;
    t.monsters.grunt.speed = 0.0;
    t.monsters.grunt.separation_speed = 0.0;
}

/// 몬스터가 제자리에 서 있고, 활의 탄환이 `pierce`마리를 맞히는 시나리오입니다. `adjust`로 수치를 더 바꿉니다.
fn quiet_field(pierce: u32, adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    Scenario::with_tuning(|t| {
        still_monsters(t);
        bow_pierces(t, pierce);
        adjust(t);
    })
}

/// (3, 10)에 선 사수가 활을 한 번 쏘고 `ticks` Tick을 더 진행합니다. 처음 Tick에 탄환이 나가고, 쿨다운 때문에 둘째 탄환은 나가지 않습니다.
fn shoot_once(s: &mut Scenario, ticks: u32) -> PlayerId {
    let a = s.player_at(3.0, 10.0);
    s.give_weapon(a, WeaponKind::Bow);
    let aim = AimDir::new(1.0, 0.0);
    s.hold(
        a,
        PlayerInput {
            aim,
            attack: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(1);
    s.hold(
        a,
        PlayerInput {
            aim,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(ticks);
    a
}

/// 탄환이 끝나는 Tick까지 진행합니다. 끝난 탄환은 한 Tick만 남습니다.
fn run_until_landed(s: &mut Scenario) {
    s.run_until(60, |sim| sim.projectiles().any(|shot| !shot.is_flying()));
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
fn a_projectile_hits_each_monster_it_passes_once_and_ends_after_the_last_one_it_can_hit() {
    let mut s = quiet_field(3, |_| {});
    let line: Vec<MonsterId> = (0..5).map(|i| s.monster_at(6.0 + i as f32, 10.0)).collect();
    let damage = bow_damage(&s);

    shoot_once(&mut s, 0);
    run_until_landed(&mut s);

    for id in &line[..3] {
        assert_eq!(hp_of(&s, *id), 1000 - damage, "hit exactly once");
    }
    for id in &line[3..] {
        assert_eq!(hp_of(&s, *id), 1000, "beyond the count");
    }
    let shot = only_projectile(&s);
    assert_eq!(shot.flight, Flight::Landed);
    assert!(close(shot.pos.x, 8.0 - 0.55), "{:?}", shot.pos);
}

#[test]
fn a_projectile_that_has_passed_a_monster_does_not_hit_it_again_while_still_overlapping() {
    let mut s = quiet_field(4, |_| {});
    let only = s.monster_at(6.0, 10.0);
    let damage = bow_damage(&s);

    shoot_once(&mut s, 4);

    let shot = only_projectile(&s);
    assert!(
        shot.is_flying(),
        "the projectile flew on through the monster"
    );
    assert!(
        shot.pos.x > 6.0 - 0.55 && shot.pos.x < 6.0 + 0.55,
        "still inside the monster's width, at {:?}",
        shot.pos
    );
    s.run_ticks(20);
    assert_eq!(
        hp_of(&s, only),
        1000 - damage,
        "one hit, however long it overlapped"
    );
}

#[test]
fn monsters_inside_one_tick_of_flight_are_hit_nearest_first_and_the_farthest_is_spared() {
    // 한 Tick에 5타일을 날아가는 탄환이 한 번에 세 마리를 지나갑니다. 번호가 작은 몬스터가 가장 멀리 있습니다.
    let mut s = quiet_field(2, |t| {
        if let Delivery::Projectile(spec) = &mut t.weapons.bow.delivery {
            spec.speed = 100.0;
        }
    });
    let far = s.monster_at(7.0, 10.0);
    let near = s.monster_at(5.0, 10.0);
    let middle = s.monster_at(6.0, 10.0);
    let damage = bow_damage(&s);

    shoot_once(&mut s, 0);

    assert_eq!(hp_of(&s, near), 1000 - damage);
    assert_eq!(hp_of(&s, middle), 1000 - damage);
    assert_eq!(
        hp_of(&s, far),
        1000,
        "the count ran out before the farthest"
    );
    assert_eq!(only_projectile(&s).flight, Flight::Landed);
}

#[test]
fn a_projectile_that_kills_what_it_hits_flies_on_to_the_next_monster() {
    let damage = Scenario::open_field().sim.tuning().weapons.bow.damage;
    let mut s = quiet_field(3, |t| t.monsters.grunt.max_hp = damage);
    for i in 0..3 {
        s.monster_at(6.0 + i as f32, 10.0);
    }

    shoot_once(&mut s, 0);
    run_until_landed(&mut s);

    assert_eq!(s.sim.monsters().count(), 0, "all three died to one shot");
}

#[test]
fn a_wall_ends_the_projectile_and_protects_the_monsters_behind_it() {
    let mut s = quiet_field(4, |t| {
        t.map.obstacles = vec![Rect::new(8.0, 9.0, 9.0, 11.0)]
    });
    let before = s.monster_at(6.0, 10.0);
    let behind = s.monster_at(10.0, 10.0);
    let damage = bow_damage(&s);

    shoot_once(&mut s, 0);
    run_until_landed(&mut s);

    assert_eq!(hp_of(&s, before), 1000 - damage);
    assert_eq!(hp_of(&s, behind), 1000);
    let shot = only_projectile(&s);
    assert_eq!(shot.flight, Flight::Landed);
    assert!(close(shot.pos.x, 8.0 - 0.15), "{:?}", shot.pos);
}

#[test]
fn a_wall_in_the_same_tick_protects_the_monsters_behind_it_too() {
    // 한 Tick에 5타일을 날아가는 탄환의 선분 안에 몬스터, 벽, 벽 뒤의 몬스터가 모두 있습니다.
    let mut s = quiet_field(4, |t| {
        t.map.obstacles = vec![Rect::new(5.0, 9.0, 6.0, 11.0)];
        if let Delivery::Projectile(spec) = &mut t.weapons.bow.delivery {
            spec.speed = 100.0;
        }
    });
    let before = s.monster_at(4.2, 10.0);
    let behind = s.monster_at(7.5, 10.0);
    let damage = bow_damage(&s);

    shoot_once(&mut s, 0);

    assert_eq!(hp_of(&s, before), 1000 - damage);
    assert_eq!(hp_of(&s, behind), 1000, "the wall is between");
    let shot = only_projectile(&s);
    assert_eq!(shot.flight, Flight::Landed);
    assert!(close(shot.pos.x, 5.0 - 0.15), "{:?}", shot.pos);
}

#[test]
fn a_projectile_with_hits_left_ends_at_the_end_of_its_range() {
    let mut s = quiet_field(4, |_| {});
    let in_reach = s.monster_at(6.0, 10.0);
    let out_of_reach = s.monster_at(14.0, 10.0);
    let damage = bow_damage(&s);
    let range = match s.sim.tuning().weapons.bow.delivery {
        Delivery::Projectile(spec) => spec.range,
        Delivery::Arc(_) => panic!("the bow shoots projectiles"),
    };

    shoot_once(&mut s, 0);
    run_until_landed(&mut s);

    assert_eq!(hp_of(&s, in_reach), 1000 - damage);
    assert_eq!(hp_of(&s, out_of_reach), 1000);
    let shot = only_projectile(&s);
    assert_eq!(shot.flight, Flight::Landed);
    assert!(close(shot.pos.x, 3.0 + range), "{:?}", shot.pos);
}

#[test]
fn a_projectile_that_can_hit_one_monster_still_ends_at_the_first_it_touches() {
    let mut s = quiet_field(1, |_| {});
    let near = s.monster_at(6.0, 10.0);
    let behind = s.monster_at(7.0, 10.0);
    let damage = bow_damage(&s);

    shoot_once(&mut s, 3);

    assert_eq!(hp_of(&s, near), 1000 - damage);
    assert_eq!(hp_of(&s, behind), 1000);
    assert_eq!(only_projectile(&s).flight, Flight::Landed);
}

/// 수치표의 석궁 행입니다.
fn crossbow_spec(s: &Scenario) -> ProjectileTuning {
    let Delivery::Projectile(spec) = s.sim.tuning().weapons.crossbow.delivery else {
        panic!("the crossbow shoots projectiles");
    };
    spec
}

/// (3, 10)에 선 사수가 석궁을 한 번 쏘고 `ticks` Tick을 더 진행합니다.
fn shoot_crossbow_once(s: &mut Scenario, ticks: u32) -> PlayerId {
    let a = s.player_at(3.0, 10.0);
    s.give_weapon(a, WeaponKind::Crossbow);
    let aim = AimDir::new(1.0, 0.0);
    s.hold(
        a,
        PlayerInput {
            aim,
            attack: true,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(1);
    s.hold(
        a,
        PlayerInput {
            aim,
            ..PlayerInput::default()
        },
    );
    s.run_ticks(ticks);
    a
}

#[test]
fn a_crossbow_bolt_hits_the_fourth_monster_as_hard_as_the_first_and_spares_the_fifth() {
    let mut s = Scenario::with_tuning(still_monsters);
    let line: Vec<MonsterId> = (0..5).map(|i| s.monster_at(6.0 + i as f32, 10.0)).collect();
    let damage = s.sim.tuning().weapons.crossbow.damage;

    shoot_crossbow_once(&mut s, 0);
    run_until_landed(&mut s);

    for id in &line[..4] {
        assert_eq!(
            hp_of(&s, *id),
            1000 - damage,
            "every monster the bolt passes takes the same damage"
        );
    }
    assert_eq!(hp_of(&s, line[4]), 1000, "the fifth is beyond the count");
    let bolt = only_projectile(&s);
    assert_eq!(bolt.flight, Flight::Landed);
    assert!(close(bolt.pos.x, 9.0 - 0.55), "{:?}", bolt.pos);
}

#[test]
fn a_crossbow_bolt_ends_at_a_wall_and_passes_through_players() {
    let mut s = Scenario::with_tuning(|t| {
        still_monsters(t);
        t.map.obstacles = vec![Rect::new(8.0, 9.0, 9.0, 11.0)];
    });
    let ally = s.player_at(5.0, 10.0);
    let behind_the_wall = s.monster_at(10.0, 10.0);

    shoot_crossbow_once(&mut s, 0);
    run_until_landed(&mut s);

    assert_eq!(s.player(ally).hp(), 100, "the ally in the way is unhurt");
    assert_eq!(hp_of(&s, behind_the_wall), 1000);
    let bolt = only_projectile(&s);
    assert!(
        close(bolt.pos.x, 8.0 - 0.15),
        "stopped at the wall: {:?}",
        bolt.pos
    );
}

#[test]
fn a_crossbow_bolt_ends_at_the_end_of_its_range() {
    let mut s = Scenario::with_tuning(still_monsters);
    let range = crossbow_spec(&s).range;

    shoot_crossbow_once(&mut s, 0);
    run_until_landed(&mut s);

    let bolt = only_projectile(&s);
    assert_eq!(bolt.flight, Flight::Landed);
    assert!(
        close(bolt.pos.x, 3.0 + range),
        "ended at its range: {:?}",
        bolt.pos
    );
}
