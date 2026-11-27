//! 마지막 밤의 보스: 포탑의 피해와 효과를 받는 방식, 맞는 몸의 크기, 일반 몬스터가 달라지지 않는 것.

use super::support::{Scenario, close, nights, watchtower_shot};
use crate::ids::MonsterId;
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::kind::{MonsterKind, StructureKind, WeaponKind};
use crate::math::Vec2;
use crate::monster::Order;
use crate::structures::TowerState;
use crate::tuning::{Debuff, Tuning};
use crate::world::{Hit, Source, Target};

/// 기본 수치에서 인원이 넷일 때 보스의 최대 HP입니다.
fn full_hp() -> u32 {
    Tuning::default().monsters.boss.max_hp
}

/// 보통 몬스터도 놓인 자리에 서 있기만 하는 20 x 20 빈 들판입니다.
fn still_field() -> Scenario {
    Scenario::with_tuning(|t| t.monsters.grunt.speed = 0.0)
}

/// 보스의 몸을 `(x, y)`에 놓습니다. 몸은 서 있기만 하므로 몸이 맞고 효과를 받는 규칙만 시험할 수 있습니다.
fn put_boss_body(s: &mut Scenario, x: f32, y: f32) -> MonsterId {
    let t = s.sim.tuning().clone();
    s.sim
        .world_mut()
        .spawn_monster(MonsterKind::Boss, Order::Stand, Vec2::new(x, y), &t)
}

fn hp_of(s: &Scenario, id: MonsterId) -> u32 {
    s.monster(id).expect("the monster is alive").hp
}

// ---------------------------------------------------------------------------------------------
// 포탑의 피해와 효과

#[test]
fn a_tower_hit_takes_a_tenth_of_the_damage_off_the_boss_and_the_same_hit_takes_all_off_a_grunt() {
    let mut s = still_field();
    let boss = put_boss_body(&mut s, 5.0, 5.0);
    let grunt = s.monster_at(10.0, 10.0);
    let t = s.sim.tuning().clone();
    let by_tower = |target, amount| Hit {
        target: Target::Monster(target),
        amount,
        by: Source::Tower,
        debuff: None,
    };

    s.sim.world_mut().apply_hits(
        &[by_tower(boss, 100), by_tower(grunt, 15), by_tower(boss, 15)],
        &t,
    );

    assert_eq!(
        hp_of(&s, boss),
        full_hp() - 10 - 2,
        "100 gives 10 and 15 gives 2"
    );
    assert_eq!(hp_of(&s, grunt), 60 - 15);
}

#[test]
fn a_tower_hit_of_one_still_takes_one_hp_off_the_boss() {
    let mut s = still_field();
    let boss = put_boss_body(&mut s, 5.0, 5.0);
    let t = s.sim.tuning().clone();

    s.sim.world_mut().apply_hits(
        &[Hit {
            target: Target::Monster(boss),
            amount: 1,
            by: Source::Tower,
            debuff: None,
        }],
        &t,
    );

    assert_eq!(hp_of(&s, boss), full_hp() - 1);
}

#[test]
fn a_player_hit_takes_the_full_damage_off_the_boss() {
    let mut s = still_field();
    let player = s.player_at(5.0, 8.0);
    let boss = put_boss_body(&mut s, 5.0, 5.0);
    let t = s.sim.tuning().clone();

    s.sim.world_mut().apply_hits(
        &[Hit {
            target: Target::Monster(boss),
            amount: 25,
            by: Source::Player(player),
            debuff: None,
        }],
        &t,
    );

    assert_eq!(hp_of(&s, boss), full_hp() - 25);
}

#[test]
fn stun_never_takes_hold_of_the_boss_and_slow_takes_half_as_much_and_bleed_takes_as_much_as_ever() {
    let mut s = still_field();
    let owner = s.player_at(5.0, 8.0);
    let boss = put_boss_body(&mut s, 5.0, 5.0);
    let grunt = s.monster_at(10.0, 10.0);

    for target in [boss, grunt] {
        s.afflict(target, Debuff::Stun { secs: 0.8 }, owner);
        s.afflict(
            target,
            Debuff::Slow {
                factor: 0.7,
                secs: 2.0,
            },
            owner,
        );
        s.afflict(
            target,
            Debuff::Bleed {
                damage: 5,
                secs: 3.0,
            },
            owner,
        );
    }

    let effects = |id| s.monster(id).expect("alive").effects.clone();
    assert!(!effects(boss).is_stunned(), "the boss is not stunned");
    assert!(effects(grunt).is_stunned(), "a grunt is");
    assert!(close(effects(boss).walk_factor(), 0.85), "half as slow");
    assert!(close(effects(grunt).walk_factor(), 0.7));
    assert!(effects(boss).is_bleeding() && effects(grunt).is_bleeding());
}

#[test]
fn a_player_bleed_hurts_the_boss_once_a_second_and_a_tower_bleed_is_cut_like_other_tower_damage() {
    let mut s = still_field();
    let owner = s.player_at(5.0, 8.0);
    let by_player = put_boss_body(&mut s, 5.0, 5.0);
    let by_tower = put_boss_body(&mut s, 12.0, 5.0);
    let bleed = Debuff::Bleed {
        damage: 50,
        secs: 3.0,
    };
    s.afflict(by_player, bleed, owner);
    let t = s.sim.tuning().clone();
    {
        let monster = s
            .sim
            .world_mut()
            .monsters
            .iter_mut()
            .find(|monster| monster.id == by_tower)
            .expect("the body");
        let resist = t.monsters.get(monster.kind).resist;
        monster.effects.apply(bleed, Source::Tower, &resist, &t);
    }

    s.run_ticks(s.ticks(1.0));

    assert_eq!(hp_of(&s, by_player), full_hp() - 50);
    assert_eq!(hp_of(&s, by_tower), full_hp() - 5);
}

#[test]
fn a_stun_tower_does_not_pick_the_boss_and_does_not_spend_its_cooldown_on_it() {
    let mut s = still_field();
    let boss = put_boss_body(&mut s, 6.0, 6.0);
    s.tower_of(StructureKind::StunTower, 8, 6);

    s.run_ticks(100);

    match &s.tower().state {
        TowerState::Standing {
            cooldown_ticks,
            last_shot,
            ..
        } => {
            assert_eq!(*cooldown_ticks, 0, "the cooldown was never spent");
            assert_eq!(*last_shot, None, "it never fired at the boss");
        }
        other => panic!("standing: {other:?}"),
    }
    assert!(!s.monster(boss).expect("boss").effects.is_stunned());
}

#[test]
fn a_stun_tower_with_the_boss_nearer_than_a_grunt_stuns_the_grunt() {
    let mut s = still_field();
    put_boss_body(&mut s, 6.0, 6.0);
    let grunt = s.monster_at(11.0, 6.0);
    s.tower_of(StructureKind::StunTower, 8, 6);

    s.run_ticks(2);

    assert!(s.monster(grunt).expect("grunt").effects.is_stunned());
}

#[test]
fn a_slow_tower_slows_the_boss_to_eighty_five_hundredths_and_a_grunt_to_seventy() {
    let mut s = still_field();
    let boss = put_boss_body(&mut s, 6.0, 6.0);
    let grunt = s.monster_at(6.0, 8.0);
    s.tower_of(StructureKind::SlowTower, 8, 6);

    s.run_ticks(2);

    let walk = |id| s.monster(id).expect("alive").effects.walk_factor();
    assert!(close(walk(boss), 0.85), "{}", walk(boss));
    assert!(close(walk(grunt), 0.7));
}

// ---------------------------------------------------------------------------------------------
// 맞는 몸

/// 플레이어가 `player_at`에서 `aim_at` 방향으로 40 Tick 동안 공격한 뒤의 보스 HP입니다. 보스는 `(15, 10)`에 서 있습니다.
fn boss_hp_after_attacking(weapon: WeaponKind, player_at: Vec2, aim_at: (f32, f32)) -> u32 {
    let mut s = Scenario::with_tuning(|t| t.monsters.boss.max_hp = 100_000);
    let id = s.player_at(player_at.x, player_at.y);
    s.give_weapon(id, weapon);
    let boss = put_boss_body(&mut s, 15.0, 10.0);
    s.hold(
        id,
        PlayerInput {
            attack: true,
            aim: AimDir::new(aim_at.0, aim_at.1),
            move_dir: MoveDir::new(0.0, 0.0),
            ..PlayerInput::default()
        },
    );
    s.run_ticks(40);
    hp_of(&s, boss)
}

#[test]
fn a_sword_reaches_the_edge_of_the_boss_body_a_tile_from_its_middle_and_not_beyond() {
    let full = 100_000;
    // 중심 사이 거리 = 플레이어 반지름 0.35 + 검의 닿는 거리 1.0 + 보스의 맞는 몸 1.0 = 2.35
    assert!(boss_hp_after_attacking(WeaponKind::Sword, Vec2::new(12.7, 10.0), (1.0, 0.0)) < full);
    assert_eq!(
        boss_hp_after_attacking(WeaponKind::Sword, Vec2::new(12.6, 10.0), (1.0, 0.0)),
        full
    );
}

#[test]
fn an_arrow_that_passes_a_tile_from_the_middle_of_the_boss_hits_it_and_one_that_passes_wider_does_not()
 {
    let full = 100_000;
    // 화살은 위쪽 한 칸 옆(y = 11.0)을 지나갑니다. 보스의 맞는 몸 1.0에 화살의 반지름 0.15를 더한 거리 안입니다.
    let near = boss_hp_after_attacking(WeaponKind::Bow, Vec2::new(9.0, 11.0), (1.0, 0.0));
    let wide = boss_hp_after_attacking(WeaponKind::Bow, Vec2::new(9.0, 11.3), (1.0, 0.0));

    assert!(near < full, "{near}");
    assert_eq!(wide, full);
}

#[test]
fn a_rifle_shot_that_passes_a_tile_from_the_middle_of_the_boss_hits_it() {
    let full = 100_000;
    assert!(boss_hp_after_attacking(WeaponKind::Rifle, Vec2::new(8.0, 10.9), (1.0, 0.0)) < full);
    assert_eq!(
        boss_hp_after_attacking(WeaponKind::Rifle, Vec2::new(8.0, 11.1), (1.0, 0.0)),
        full
    );
}

#[test]
fn a_rifle_line_that_ends_inside_the_boss_body_still_hits_it() {
    let full = 100_000;
    // 소총의 선은 플레이어의 중심에서 12타일 나아가고 보스의 중심에서 0.5타일 앞(맞는 몸 안)에서 끝납니다.
    // 걷는 몸(0.4타일)에는 닿지 않는 거리입니다.
    assert!(boss_hp_after_attacking(WeaponKind::Rifle, Vec2::new(2.5, 10.0), (1.0, 0.0)) < full);
    assert_eq!(
        boss_hp_after_attacking(WeaponKind::Rifle, Vec2::new(1.9, 10.0), (1.0, 0.0)),
        full,
        "a line that ends 1.1 tiles before the middle stays outside the body"
    );
}

#[test]
fn a_watchtower_shoots_a_boss_whose_edge_is_in_range_and_not_a_grunt_at_the_same_distance() {
    // 감시탑의 사거리는 6타일이고 보스의 맞는 몸이 걷는 몸 밖으로 0.6타일 나옵니다. 탑의 중심 (10.5, 3.5)에서 보스의 중심까지
    // 6.6타일 안이면 겨눕니다. 보통 몬스터는 6타일 안이어야 합니다.
    let lost_hp = |x: f32, boss: bool| {
        let mut s = still_field();
        let id = if boss {
            put_boss_body(&mut s, x, 3.5)
        } else {
            s.monster_at(x, 3.5)
        };
        s.tower_at(10, 3);
        s.run_ticks(5);
        let full = if boss { full_hp() } else { 60 };
        full - hp_of(&s, id)
    };

    assert_eq!(lost_hp(3.5, true), 0, "7.0 tiles is out of reach");
    assert!(
        lost_hp(4.0, true) > 0,
        "6.5 tiles is in reach for the edge of the body"
    );
    assert_eq!(lost_hp(4.4, false), 0, "a grunt 6.1 tiles away is out");
    assert!(lost_hp(4.6, false) > 0, "a grunt 5.9 tiles away is in");
}

// ---------------------------------------------------------------------------------------------
// 일반 몬스터가 달라지지 않는 것

/// 몬스터의 위치와 HP를 비트 단위로 적습니다.
fn signature(s: &Scenario) -> Vec<(u32, u32, u32, u32)> {
    s.sim
        .monsters()
        .map(|m| (m.id.0, m.pos.x.to_bits(), m.pos.y.to_bits(), m.hp))
        .collect()
}

#[test]
fn the_boss_row_changes_nothing_for_the_grunts() {
    // 마을이 포탑으로 가득하고 몬스터 120마리가 몰려오는 기본 맵입니다. 보스의 줄을 몬스터의 줄과 같게 되돌려도 몬스터의 위치가 비트까지 같아야 합니다.
    let run = |boss_row_is_a_grunt: bool| {
        let mut s = Scenario::default_game(|t| {
            if boss_row_is_a_grunt {
                t.monsters.boss = t.monsters.grunt.clone();
            }
            t.cycle.day_secs = t.dt();
            t.nights.plans = nights(&[&[120]]);
            t.nights.first_wave_delay_secs = t.dt();
            t.nights.spawn_interval_secs = t.dt();
            t.wagon.max_hp = u32::MAX;
            t.guards.per_node = 0;
            t.guards.per_chest = 0;
            t.village.start_towers = t.spread_tower_sites(t.village.tower_limit);
            watchtower_shot(t).damage = 0;
        });
        s.run_ticks(900);
        signature(&s)
    };

    let with_the_boss_row = run(false);
    let without = run(true);

    assert!(with_the_boss_row.len() > 50, "a crowd");
    assert_eq!(with_the_boss_row, without, "same positions bit for bit");
}

#[test]
fn the_boss_row_leaves_the_walking_bounds_of_the_grunts_as_they_were() {
    let t = Tuning::default();

    assert_eq!(t.monsters.largest_radius(), t.monsters.grunt.radius);
    assert_eq!(
        t.monsters.longest_engage_distance(),
        t.monsters.grunt.radius + t.monsters.grunt.attack_reach
    );
    assert!(
        t.monsters.boss.hit_radius() > 0.5,
        "the hit body is big enough that a single tile cannot hold it"
    );
    assert!(t.monsters.boss.radius < 0.5, "the walking body is small");
}
