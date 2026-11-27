//! 마지막 밤의 보스: 인원에 따른 HP, 대상을 고르는 규칙, 예고된 내려찍기를 걸어서 피하는 것,
//! 포탑의 피해와 효과를 받는 방식, 맞는 몸의 크기, 일반 몬스터가 달라지지 않는 것.

use super::support::{Scenario, close, nights, outcome, phase, watchtower_shot};
use crate::boss::{Act, Aim};
use crate::build;
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::kind::{MonsterKind, StructureKind, WeaponKind};
use crate::math::Vec2;
use crate::monster::Order;
use crate::player::Presence;
use crate::structures::{Tower, TowerState};
use crate::telegraph::{self, Zone};
use crate::tuning::{BossTuning, Debuff, Finale, StrikeTuning, Tuning, WaveScaling};
use crate::world::{Hit, Source, Target, Tile};

/// 기본 수치에서 인원이 넷일 때 보스의 최대 HP입니다.
fn full_hp() -> u32 {
    Tuning::default().monsters.boss.max_hp
}

/// 보스의 수치를 켭니다. 원정을 시험하는 시나리오는 보스를 꺼 두고 시작하므로, 보스가 필요한 테스트가 부릅니다.
fn with_boss(t: &mut Tuning) {
    t.nights.finale = Tuning::default().nights.finale;
}

/// 시나리오의 수치에서 보스의 행동을 바꿉니다. 보스가 켜져 있어야 합니다.
fn boss_tuning(t: &mut Tuning) -> &mut BossTuning {
    match &mut t.nights.finale {
        Finale::Boss(cfg) => cfg,
        Finale::LastWave => panic!("the scenario has a boss"),
    }
}

/// 보스와 보통 몬스터가 놓인 자리에 서 있기만 하는 20 x 20 빈 들판입니다. 보스의 행동만 시험할 때 씁니다.
fn still_field() -> Scenario {
    Scenario::with_tuning(|t| {
        with_boss(t);
        t.monsters.boss.speed = 0.0;
        t.monsters.grunt.speed = 0.0;
    })
}

/// 보스의 몸을 `(x, y)`에 놓습니다. 몸은 서 있기만 하므로 몸이 맞고 효과를 받는 규칙만 시험할 수 있습니다.
fn put_boss_body(s: &mut Scenario, x: f32, y: f32) -> MonsterId {
    let t = s.sim.tuning().clone();
    s.sim
        .world_mut()
        .spawn_monster(MonsterKind::Boss, Order::Stand, Vec2::new(x, y), &t)
}

/// 보스를 `(x, y)`에 놓고 보스의 단계로 바꿉니다. 방에 `party`명이 있는 밤입니다.
fn put_boss(s: &mut Scenario, x: f32, y: f32, party: u32) -> MonsterId {
    let t = s.sim.tuning().clone();
    let Finale::Boss(cfg) = &t.nights.finale else {
        panic!("the scenario has a boss");
    };
    s.sim.set_phase(Phase::Night {
        day: 1,
        party,
        step: NightStep::Boss,
    });
    s.sim
        .world_mut()
        .spawn_boss(Vec2::new(x, y), party, cfg, &t)
}

fn move_boss(s: &mut Scenario, x: f32, y: f32) {
    let id = s.sim.boss().expect("a boss").id;
    s.sim
        .world_mut()
        .monsters
        .iter_mut()
        .find(|monster| monster.id == id)
        .expect("the body")
        .pos = Vec2::new(x, y);
}

fn hp_of(s: &Scenario, id: MonsterId) -> u32 {
    s.monster(id).expect("the monster is alive").hp
}

fn boss_hp(s: &Scenario) -> u32 {
    hp_of(s, s.sim.boss().expect("a boss").id)
}

fn aim(s: &Scenario) -> Aim {
    s.sim.boss().expect("a boss").aim
}

fn act(s: &Scenario) -> Act {
    s.sim.boss().expect("a boss").act
}

/// 플레이어 `by`가 보스에게 `amount`의 피해를 준 것으로 만듭니다. 무기를 거치지 않고 대상을 고르는 규칙만 시험할 때 씁니다.
fn hit_boss(s: &mut Scenario, by: PlayerId, amount: u32) {
    let t = s.sim.tuning().clone();
    let id = s.sim.boss().expect("a boss").id;
    s.sim.world_mut().apply_hits(
        &[Hit {
            target: Target::Monster(id),
            amount,
            by: Source::Player(by),
            debuff: None,
        }],
        &t,
    );
}

/// 걷는 시간이 남지 않았다고 정해서 다음 Tick에 행동을 시작하게 합니다.
fn skip_the_rest(s: &mut Scenario) {
    let boss = s.sim.world_mut().boss.as_mut().expect("a boss");
    boss.act = Act::Walking { ticks_left: 1 };
}

fn run_until_slamming(s: &mut Scenario) {
    skip_the_rest(s);
    s.run_until(5, |sim| {
        matches!(sim.boss().map(|boss| boss.act), Some(Act::Slamming(_)))
    });
}

fn slam_center(s: &Scenario) -> Vec2 {
    match act(s) {
        Act::Slamming(telegraph) => match telegraph.zone {
            Zone::Circle { center, .. } => center,
        },
        other => panic!("the boss is not slamming: {other:?}"),
    }
}

fn hp_of_player(s: &Scenario, id: PlayerId) -> u32 {
    s.player(id).hp()
}

// ---------------------------------------------------------------------------------------------
// 등장과 성공과 실패

/// 마지막 밤의 마지막 무리가 모두 나타나서 싸우는 단계까지 진행합니다. 몬스터가 모두 처치된 것으로 하려면 이어서 `clear_monsters`를 부릅니다.
fn run_until_the_last_wave_is_out(s: &mut Scenario) {
    s.run_until(2000, |sim| {
        matches!(
            phase(sim),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });
}

#[test]
fn clearing_the_last_wave_of_the_last_night_brings_five_seconds_of_warning_and_then_the_boss() {
    let mut s = Scenario::with_cycle(|t| {
        with_boss(t);
        t.cycle.day_secs = t.dt();
        t.nights.plans = nights(&[&[2]]);
    });
    run_until_the_last_wave_is_out(&mut s);
    s.clear_monsters();

    s.run_ticks(1);
    let warning = s.ticks(5.0);
    assert_eq!(warning, 100);
    assert!(matches!(
        phase(&s.sim),
        Phase::Night {
            step: NightStep::Warning { ticks_left },
            ..
        } if ticks_left == warning
    ));
    assert_eq!(outcome(&s.sim), None, "the expedition goes on");
    let waited = s.run_until(500, |sim| sim.boss().is_some());

    assert_eq!(waited, warning, "five seconds of warning");
    assert!(matches!(
        phase(&s.sim),
        Phase::Night {
            step: NightStep::Boss,
            ..
        }
    ));
    assert_eq!(s.sim.monsters().count(), 1, "the boss alone");
}

#[test]
fn the_boss_arrives_where_the_night_waves_arrive() {
    let mut s = Scenario::default_game(|t| {
        with_boss(t);
        t.cycle.day_secs = t.dt();
        t.nights.plans = nights(&[&[1]]);
    });
    run_until_the_last_wave_is_out(&mut s);
    let wave_spot = s.sim.monsters().last().expect("the wave's monster").pos;
    s.clear_monsters();
    s.run_until(2000, |sim| sim.boss().is_some());

    let id = s.sim.boss().expect("a boss").id;
    let body = s.monster(id).expect("the body");
    let wagon = s.sim.wagon().rect;
    let gap = wagon.distance_to(body.pos);
    assert!(
        (gap - s.sim.tuning().nights.spawn_distance).abs() < 1.0,
        "{gap} tiles from the wagon, like the waves"
    );
    assert!(
        body.pos.distance(wave_spot) > 5.0,
        "it comes from the direction that follows the last wave's, not from the same side"
    );
    assert_eq!(body.kind, MonsterKind::Boss);
}

#[test]
fn a_night_that_is_not_the_last_has_no_boss() {
    let mut s = Scenario::with_cycle(|t| {
        with_boss(t);
        t.cycle.day_secs = t.dt();
        t.nights.plans = nights(&[&[1], &[1]]);
    });
    run_until_the_last_wave_is_out(&mut s);
    s.clear_monsters();

    s.run_ticks(1);

    assert!(matches!(phase(&s.sim), Phase::Dawn { .. }));
    s.run_ticks(2 * s.ticks(5.0));
    assert!(s.sim.boss().is_none());
}

#[test]
fn the_expedition_does_not_succeed_while_the_boss_lives_and_succeeds_the_moment_it_falls() {
    let mut s = still_field();
    let boss = put_boss(&mut s, 5.0, 5.0, 4);

    s.run_ticks(200);
    assert_eq!(outcome(&s.sim), None, "the boss still stands");

    let slayer = s.player_at(5.0, 7.0);
    s.sim
        .world_mut()
        .monsters
        .iter_mut()
        .find(|monster| monster.id == boss)
        .expect("the body")
        .hp = 1;
    s.hold(
        slayer,
        PlayerInput {
            attack: true,
            aim: AimDir::new(0.0, -1.0),
            ..PlayerInput::default()
        },
    );
    s.run_ticks(1);

    assert_eq!(outcome(&s.sim), Some(Outcome::Victory));
}

#[test]
fn a_wagon_that_falls_or_a_party_that_goes_down_ends_the_boss_fight_in_defeat() {
    let mut s = still_field();
    put_boss(&mut s, 5.0, 5.0, 4);
    s.player_at(5.0, 15.0);
    s.set_wagon_hp(0);
    s.run_ticks(1);
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));

    let mut s = still_field();
    put_boss(&mut s, 5.0, 5.0, 4);
    let only = s.player_at(5.0, 15.0);
    s.knock_down(only);
    s.run_ticks(1);
    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));
}

#[test]
fn the_boss_that_comes_has_the_hp_of_the_party_that_began_the_night() {
    for party in [1_u32, 2, 4] {
        let mut s = Scenario::with_cycle(|t| {
            with_boss(t);
            t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
            t.cycle.day_secs = t.dt();
            t.nights.plans = nights(&[&[1]]);
            t.guards.per_node = 0;
        });
        for _ in 0..party {
            s.player_at(10.0, 10.0);
        }
        run_until_the_last_wave_is_out(&mut s);
        s.clear_monsters();

        s.run_until(500, |sim| sim.boss().is_some());

        let hp = (full_hp() * party).div_ceil(4);
        assert_eq!(boss_hp(&s), hp, "{party} players");
        assert_eq!(s.sim.boss().expect("a boss").max_hp, hp);
    }
}

#[test]
fn the_camp_never_has_a_boss_and_a_tuning_without_a_boss_ends_the_last_wave_in_victory() {
    let mut s = Scenario::camp(|_| {});
    s.run_ticks(200);
    assert!(s.sim.boss().is_none());
    assert!(s.sim.monsters().all(|m| m.kind == MonsterKind::Scarecrow));

    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = t.dt();
        t.nights.plans = nights(&[&[1]]);
    });
    assert_eq!(s.sim.tuning().nights.finale, Finale::LastWave);
    run_until_the_last_wave_is_out(&mut s);
    s.clear_monsters();
    s.run_ticks(1);
    assert_eq!(outcome(&s.sim), Some(Outcome::Victory));
}

// ---------------------------------------------------------------------------------------------
// 인원에 따른 HP

#[test]
fn the_boss_max_hp_is_proportional_to_the_party_of_that_night() {
    for party in 1..=4 {
        let hp = (full_hp() * party).div_ceil(4);
        let mut s = Scenario::with_tuning(|t| {
            with_boss(t);
            t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        });
        put_boss(&mut s, 5.0, 5.0, party);

        assert_eq!(boss_hp(&s), hp, "{party} players");
        assert_eq!(s.sim.boss().expect("a boss").max_hp, hp);
    }
}

#[test]
fn a_boss_of_a_fixed_size_night_has_the_full_hp_whatever_the_party() {
    let mut s = Scenario::with_tuning(with_boss);
    put_boss(&mut s, 5.0, 5.0, 1);

    assert_eq!(boss_hp(&s), full_hp());
}

// ---------------------------------------------------------------------------------------------
// 대상

#[test]
fn the_boss_aims_at_the_player_who_hit_it_hardest_and_only_a_harder_hitter_takes_the_aim_over() {
    let mut s = still_field();
    let (a, b) = (s.player_at(5.0, 8.0), s.player_at(8.0, 5.0));
    put_boss(&mut s, 5.0, 5.0, 4);

    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Wagon, "nobody has hit it yet");

    hit_boss(&mut s, a, 50);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Player(a));

    hit_boss(&mut s, b, 30);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Player(a), "50 is more than 30");

    hit_boss(&mut s, b, 30);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Player(b), "60 is more than 50");

    hit_boss(&mut s, a, 10);
    s.run_ticks(1);
    assert_eq!(
        aim(&s),
        Aim::Player(b),
        "60 and 60 are a tie, and the holder keeps it"
    );

    hit_boss(&mut s, a, 1);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Player(a), "61 is more than 60");
}

#[test]
fn a_tie_between_two_players_who_do_not_hold_the_aim_goes_to_the_lower_number() {
    let mut s = still_field();
    let (a, b) = (s.player_at(5.0, 8.0), s.player_at(8.0, 5.0));
    put_boss(&mut s, 5.0, 5.0, 4);

    hit_boss(&mut s, b, 40);
    hit_boss(&mut s, a, 40);
    s.run_ticks(1);

    assert_eq!(aim(&s), Aim::Player(a.min(b)));
}

#[test]
fn a_player_who_goes_down_or_leaves_the_seat_is_skipped_and_with_nobody_left_the_boss_aims_at_the_wagon()
 {
    let mut s = still_field();
    let (a, b) = (s.player_at(5.0, 8.0), s.player_at(8.0, 5.0));
    // 때리지 않은 플레이어 하나가 멀리 서 있어야 앞의 둘이 모두 쓰러지거나 자리를 비워도 원정이 실패로 끝나지 않습니다.
    s.player_at(15.0, 2.0);
    put_boss(&mut s, 5.0, 5.0, 4);
    hit_boss(&mut s, a, 50);
    hit_boss(&mut s, b, 20);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Player(a));

    s.knock_down(a);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Player(b), "the next one");

    s.sim.set_presence(b, Presence::Away);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Wagon, "an absent player is not aimed at");

    s.sim.set_presence(b, Presence::Here);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Player(b));
    s.kill(b);
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Wagon);
}

#[test]
fn a_player_farther_than_eight_tiles_is_not_aimed_at_however_hard_the_player_hits() {
    let mut s = still_field();
    let far = s.player_at(5.0, 13.0);
    let near = s.player_at(5.0, 8.0);
    put_boss(&mut s, 5.0, 5.0, 4);
    hit_boss(&mut s, far, 500);
    hit_boss(&mut s, near, 1);

    s.run_ticks(1);
    assert_eq!(
        aim(&s),
        Aim::Player(far),
        "exactly 8 tiles away is in range"
    );

    s.move_player(far, Vec2::new(5.0, 13.01));
    s.run_ticks(1);
    assert_eq!(
        aim(&s),
        Aim::Player(near),
        "beyond 8 tiles, the far one is out"
    );

    s.move_player(near, Vec2::new(5.0, 14.0));
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Wagon, "both are out of range");
}

#[test]
fn damage_older_than_five_seconds_no_longer_counts_and_tower_damage_never_does() {
    let mut s = still_field();
    let a = s.player_at(5.0, 8.0);
    put_boss(&mut s, 5.0, 5.0, 4);
    hit_boss(&mut s, a, 50);
    let window = s.ticks(5.0);

    s.run_ticks(window);
    assert_eq!(
        aim(&s),
        Aim::Player(a),
        "exactly five seconds later it still counts"
    );
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Wagon, "one tick later it does not");

    // 포탑이 준 피해는 아무리 많아도 대상을 정하지 않습니다.
    let t = s.sim.tuning().clone();
    let id = s.sim.boss().expect("a boss").id;
    s.sim.world_mut().apply_hits(
        &[Hit {
            target: Target::Monster(id),
            amount: 100,
            by: Source::Tower,
            debuff: None,
        }],
        &t,
    );
    s.run_ticks(1);
    assert_eq!(aim(&s), Aim::Wagon);
}

#[test]
fn a_players_bleed_counts_for_the_aim() {
    let mut s = still_field();
    let owner = s.player_at(5.0, 8.0);
    let boss = put_boss(&mut s, 5.0, 5.0, 4);
    s.afflict(
        boss,
        Debuff::Bleed {
            damage: 5,
            secs: 3.0,
        },
        owner,
    );

    s.run_ticks(s.ticks(1.0));
    assert_eq!(hp_of(&s, boss), full_hp() - 5);
    // 대상은 피해를 적용하기 전에 정하므로 다음 Tick에 출혈의 피해가 반영됩니다.
    s.run_ticks(1);

    assert_eq!(
        aim(&s),
        Aim::Player(owner),
        "the bleed is the player's damage"
    );
}

#[test]
fn the_boss_starts_a_move_only_when_its_aim_is_within_eight_tiles() {
    let mut s = still_field();
    put_boss(&mut s, 3.0, 3.0, 4);
    skip_the_rest(&mut s);

    s.run_ticks(60);
    assert!(
        matches!(act(&s), Act::Walking { ticks_left: 0 }),
        "the wagon is too far, so the boss waits: {:?}",
        act(&s)
    );

    // 마차의 사각형은 (17, 17)부터입니다. (10, 10)에서 가장자리까지 9.9타일, (12, 12)에서 7.07타일입니다.
    move_boss(&mut s, 10.0, 10.0);
    s.run_ticks(3);
    assert!(matches!(act(&s), Act::Walking { .. }));
    move_boss(&mut s, 12.0, 12.0);
    s.run_ticks(1);
    assert!(matches!(act(&s), Act::Slamming(_)), "{:?}", act(&s));
}

// ---------------------------------------------------------------------------------------------
// 내려찍기

/// 보스(6, 10)가 3타일 떨어진 플레이어를 노리고 막 내려찍기를 예고한 상황입니다.
fn slam_at_a_player() -> (Scenario, PlayerId) {
    let mut s = still_field();
    let player = s.player_at(6.0, 13.0);
    put_boss(&mut s, 6.0, 10.0, 4);
    hit_boss(&mut s, player, 10);
    s.run_ticks(1);
    run_until_slamming(&mut s);
    (s, player)
}

#[test]
fn the_slam_is_telegraphed_for_one_point_five_seconds_where_the_aim_stood_and_does_not_follow() {
    let (mut s, player) = slam_at_a_player();
    let center = slam_center(&s);
    assert_eq!(center, Vec2::new(6.0, 13.0), "where the aim stood");
    let Act::Slamming(telegraph) = act(&s) else {
        panic!("slamming");
    };
    assert_eq!(telegraph.ticks_left, 30, "one and a half seconds");

    s.hold_move(player, 1.0, 0.0);
    for _ in 0..telegraph.ticks_left - 1 {
        s.run_ticks(1);
        assert_eq!(slam_center(&s), center, "the circle stays");
    }
}

#[test]
fn a_player_who_stays_in_the_circle_takes_the_damage_and_one_who_leaves_takes_none() {
    let (mut stay, stayer) = slam_at_a_player();
    stay.run_ticks(30);
    assert_eq!(hp_of_player(&stay, stayer), 100 - 50);
    assert!(
        matches!(act(&stay), Act::Walking { .. }),
        "the slam is over"
    );

    let (mut leave, leaver) = slam_at_a_player();
    leave.hold_move(leaver, 1.0, 0.0);
    leave.run_ticks(30);
    assert_eq!(hp_of_player(&leave, leaver), 100);
}

/// 원의 중심에 서 있는 플레이어가 걸어서 원 밖으로 나가는 데 필요한 Tick입니다. 반지름보다 멀어져야 원 밖입니다.
fn ticks_to_walk_out(s: &Scenario) -> u32 {
    let t = s.sim.tuning();
    let Finale::Boss(cfg) = &t.nights.finale else {
        panic!("a boss");
    };
    let step = t.player.speed * t.dt();
    (cfg.slam.radius / step).floor() as u32 + 1
}

#[test]
fn a_player_in_the_middle_who_starts_walking_four_tenths_of_a_second_late_gets_out() {
    let (mut s, player) = slam_at_a_player();
    let late = s.ticks(0.4);
    assert_eq!(late, 8);

    s.run_ticks(late);
    s.hold_move(player, 1.0, 0.0);
    s.run_ticks(30 - late);

    assert_eq!(hp_of_player(&s, player), 100, "out of the circle in time");
}

#[test]
fn the_latest_start_that_still_gets_out_by_walking_is_most_of_a_second() {
    let (probe, _) = slam_at_a_player();
    let windup = 30;
    let needed = ticks_to_walk_out(&probe);
    let latest = windup - needed;
    println!(
        "slam: {windup} ticks of warning, {needed} ticks to walk out, so the latest start is {latest} ticks late"
    );
    assert!(
        latest >= probe.ticks(0.8),
        "0.4 seconds late is well inside, and so is 0.8"
    );

    let (mut s, player) = slam_at_a_player();
    s.run_ticks(latest);
    s.hold_move(player, 1.0, 0.0);
    s.run_ticks(windup - latest);
    assert_eq!(
        hp_of_player(&s, player),
        100,
        "{latest} ticks late is still enough"
    );

    let (mut s, player) = slam_at_a_player();
    s.run_ticks(latest + 1);
    s.hold_move(player, 1.0, 0.0);
    s.run_ticks(windup - latest - 1);
    assert_eq!(hp_of_player(&s, player), 50, "one tick later is not");
}

#[test]
fn a_slam_hurts_the_towers_and_the_wagon_it_covers_and_spares_the_rest() {
    let mut s = still_field();
    let player = s.player_at(15.0, 15.0);
    put_boss(&mut s, 12.0, 15.0, 4);
    hit_boss(&mut s, player, 10);
    let inside = s.tower_at(14, 15);
    let outside = s.tower_at(2, 2);
    s.run_ticks(1);
    run_until_slamming(&mut s);
    // 플레이어는 원의 중심에 선 채로 맞고, 마차(17, 17)는 중심에서 2.83타일이라 원 밖입니다.
    s.run_ticks(30);

    let full = s.sim.tuning().structures.watchtower.max_hp;
    let hp = |id| s.sim.towers().find(|tower| tower.id == id).map(Tower::hp);
    assert_eq!(hp(inside), Some(full - 100));
    assert_eq!(hp(outside), Some(full));
    assert_eq!(s.sim.wagon().hp, s.sim.tuning().wagon.max_hp);
}

#[test]
fn a_slam_on_the_wagon_lands_on_the_middle_of_the_wagon() {
    let mut s = still_field();
    put_boss(&mut s, 12.0, 12.0, 4);
    run_until_slamming(&mut s);

    assert_eq!(slam_center(&s), s.sim.wagon().rect.center());
    s.run_ticks(30);
    assert_eq!(s.sim.wagon().hp, s.sim.tuning().wagon.max_hp - 60);
}

#[test]
fn a_slam_does_not_hurt_the_boss_or_the_monsters_in_it() {
    let mut s = still_field();
    let boss = put_boss(&mut s, 12.0, 12.0, 4);
    let grunt = s.monster_at(13.0, 12.0);
    run_until_slamming(&mut s);

    s.run_ticks(30);

    assert_eq!(hp_of(&s, boss), full_hp());
    assert_eq!(hp_of(&s, grunt), 60);
}

#[test]
fn the_boss_walks_a_rest_between_slams_and_slams_again() {
    let mut s = still_field();
    put_boss(&mut s, 12.0, 12.0, 4);
    let rest = s.ticks(3.0);
    let mut starts = Vec::new();
    let mut was_slamming = false;
    for _ in 0..300 {
        s.run_ticks(1);
        let slamming = matches!(act(&s), Act::Slamming(_));
        if slamming && !was_slamming {
            starts.push(s.sim.tick());
        }
        was_slamming = slamming;
    }

    assert!(starts.len() >= 3, "{starts:?}");
    assert_eq!(starts[0], u64::from(rest), "the first slam after the rest");
    for pair in starts.windows(2) {
        assert_eq!(pair[1] - pair[0], u64::from(30 + rest), "slam then rest");
    }
}

// ---------------------------------------------------------------------------------------------
// 걷기

/// 보스가 걷기만 하는 20 x 20 빈 들판입니다. 걷는 시간이 길어서 행동을 시작하지 않습니다.
fn walking_field() -> Scenario {
    Scenario::with_tuning(|t| {
        with_boss(t);
        boss_tuning(t).rest_secs = 1000.0;
    })
}

fn body_of_boss(s: &Scenario) -> Vec2 {
    s.monster(s.sim.boss().expect("a boss").id)
        .expect("the body")
        .pos
}

#[test]
fn the_boss_walks_to_the_wagon_and_stops_at_its_edge_without_striking_it() {
    let mut s = walking_field();
    put_boss(&mut s, 5.0, 5.0, 4);
    let wagon = s.sim.wagon().rect;
    let reach = s.sim.tuning().monsters.longest_engage_distance();

    s.run_ticks(600);

    let gap = wagon.distance_to(body_of_boss(&s));
    assert!(gap <= reach + 0.05, "the boss is at the wagon: {gap}");
    assert!(gap > reach - 0.3, "and no closer than its reach: {gap}");
    assert_eq!(s.sim.wagon().hp, s.sim.tuning().wagon.max_hp, "no melee");
}

#[test]
fn the_boss_walks_to_the_player_it_aims_at_and_stops_beside_the_player_without_striking() {
    let mut s = walking_field();
    let player = s.player_at(5.0, 9.0);
    put_boss(&mut s, 5.0, 5.0, 4);
    hit_boss(&mut s, player, 10);

    // 피해를 준 지 5초가 지나면 대상이 마차로 돌아가므로, 그 안에 닿을 거리에 세웁니다.
    s.run_ticks(80);

    let gap = body_of_boss(&s).distance(s.player(player).pos);
    assert!(gap < 1.5, "the boss came to the player: {gap}");
    assert_eq!(hp_of_player(&s, player), 100, "no melee");
}

#[test]
fn the_boss_stands_still_while_it_telegraphs_a_slam() {
    let mut s = Scenario::with_tuning(|t| {
        with_boss(t);
        t.monsters.grunt.speed = 0.0;
    });
    let player = s.player_at(10.0, 13.0);
    put_boss(&mut s, 10.0, 9.0, 4);
    hit_boss(&mut s, player, 10);
    s.run_ticks(1);
    run_until_slamming(&mut s);
    let at = body_of_boss(&s);

    s.run_ticks(29);

    assert!(matches!(act(&s), Act::Slamming(_)), "still telegraphing");
    assert_eq!(body_of_boss(&s), at);
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

// ---------------------------------------------------------------------------------------------
// 예고된 공격이 맞히는 것

#[test]
fn a_strike_hurts_the_players_towers_and_wagon_the_circle_covers_and_spares_the_rest() {
    let mut s = still_field();
    let inside = s.player_at(5.0, 5.0);
    let outside = s.player_at(8.1, 5.0);
    let downed = s.player_at(5.5, 5.0);
    s.knock_down(downed);
    let near_tower = s.tower_at(6, 5);
    let far_tower = s.tower_at(9, 9);
    // 마차는 (17, 17)에서 (19, 19)이므로 원의 중심을 마차의 모서리 곁으로 옮긴 두 번째 영역으로 시험합니다.
    let circle = |x: f32, y: f32| Zone::Circle {
        center: Vec2::new(x, y),
        radius: 2.5,
    };
    let damage = StrikeTuning {
        player: 50,
        tower: 100,
        wagon: 60,
    };

    let mut hits = Vec::new();
    telegraph::strike(s.sim.world_mut(), &circle(5.0, 5.0), &damage, &mut hits);

    let hit_of = |target: Target| hits.iter().find(|hit| hit.target == target);
    assert_eq!(
        hit_of(Target::Player(inside)).map(|hit| hit.amount),
        Some(50)
    );
    assert_eq!(hit_of(Target::Player(outside)).map(|hit| hit.amount), None);
    assert_eq!(hit_of(Target::Player(downed)).map(|hit| hit.amount), None);
    assert_eq!(
        hit_of(Target::Tower(near_tower)).map(|hit| hit.amount),
        Some(100)
    );
    assert_eq!(hit_of(Target::Tower(far_tower)).map(|hit| hit.amount), None);
    assert_eq!(hit_of(Target::Wagon).map(|hit| hit.amount), None);
    assert!(hits.iter().all(|hit| hit.by == Source::Monster));

    let mut hits = Vec::new();
    telegraph::strike(s.sim.world_mut(), &circle(15.5, 17.0), &damage, &mut hits);
    assert_eq!(hits.len(), 1, "only the wagon: {hits:?}");
    assert_eq!(hits[0].target, Target::Wagon);
    assert_eq!(hits[0].amount, 60);
}

// ---------------------------------------------------------------------------------------------
// 한 칸 폭의 길

/// 맵의 모든 타일입니다.
fn all_tiles(t: &Tuning) -> Vec<Tile> {
    let mut tiles = Vec::new();
    for x in 0..t.map.width as u16 {
        for y in 0..t.map.height as u16 {
            tiles.push(Tile::new(x, y));
        }
    }
    tiles
}

/// `tiles`의 차례대로 타일을 하나씩 훑으면서, 건설의 규칙이 허락하는 타일마다 감시탑을 세웁니다. `count`개를 세우면 멈춥니다.
/// 규칙이 허락하지 않는 타일(길을 끊거나 마을 밖인 타일)은 건너뜁니다. 세운 포탑의 수를 돌려줍니다.
fn place_watchtowers_in_order(s: &mut Scenario, tiles: Vec<Tile>, count: usize) -> usize {
    let t = s.sim.tuning().clone();
    let mut placed = 0;
    for tile in tiles {
        if placed >= count {
            break;
        }
        let allowed = build::check_site(s.sim.world_mut(), &t, PlayerId(0), tile, None).is_ok();
        if allowed {
            let state = Tower::standing_state(StructureKind::Watchtower, &t);
            s.sim
                .world_mut()
                .add_tower(StructureKind::Watchtower, tile, state);
            placed += 1;
        }
    }
    placed
}

/// 마차에서 가까운 순서(`inner_first`)나 먼 순서로 건설의 규칙이 허락하는 만큼 빽빽하게 감시탑을 세웁니다.
fn fill_the_village_as_densely_as_the_build_rule_allows(
    s: &mut Scenario,
    inner_first: bool,
) -> usize {
    let t = s.sim.tuning().clone();
    let wagon = t.map.wagon;
    let mut tiles = all_tiles(&t);
    tiles.sort_by(|a, b| {
        let da = wagon.distance_to(a.center());
        let db = wagon.distance_to(b.center());
        let by_distance = if inner_first {
            da.total_cmp(&db)
        } else {
            db.total_cmp(&da)
        };
        by_distance.then(a.cmp(b))
    });
    place_watchtowers_in_order(s, tiles, t.village.tower_limit as usize)
}

/// 보스가 마차의 가장자리에서 걷는 몸으로 닿을 거리 안에 들어오는 Tick까지 진행하고 걸린 Tick을 돌려줍니다.
fn walk_to_the_wagon(s: &mut Scenario) -> u32 {
    let wagon = s.sim.wagon().rect;
    let reach = s.sim.tuning().monsters.longest_engage_distance();
    s.run_until(3000, |sim| {
        let id = sim.boss().expect("a boss").id;
        let body = sim.monsters().find(|m| m.id == id).expect("the body");
        wagon.distance_to(body.pos) <= reach + 0.3
    })
}

#[test]
fn the_boss_reaches_the_wagon_through_the_densest_tower_layouts_the_build_rule_allows() {
    for inner_first in [false, true] {
        let mut s = Scenario::default_game(|t| {
            with_boss(t);
            // 포탑이 사격하지 않고 보스가 행동하지 않게 해서 길만 시험합니다. 보스가 포탑을 부수고 지나가지 않았다는 것은 아래에서 확인합니다.
            watchtower_shot(t).damage = 0;
            boss_tuning(t).rest_secs = 1000.0;
            t.guards.per_node = 0;
            t.guards.per_chest = 0;
        });
        let placed = fill_the_village_as_densely_as_the_build_rule_allows(&mut s, inner_first);
        assert_eq!(placed, 40, "the build rule lets the cap be reached");
        let spawn = s.sim.tuning().map.player_spawns[0];
        s.player_at(spawn.x, spawn.y);
        s.sim.set_phase(Phase::Night {
            day: 3,
            party: 1,
            step: NightStep::Warning { ticks_left: 1 },
        });
        s.run_ticks(1);

        let ticks = walk_to_the_wagon(&mut s);

        let order = if inner_first {
            "inner first"
        } else {
            "outer first"
        };
        println!("the boss reached the wagon through 40 towers placed {order} in {ticks} ticks");
        assert_eq!(
            s.sim.towers().filter(|tower| tower.is_standing()).count(),
            40,
            "{order}: no tower was knocked down to make way"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 포탑만의 전투

/// 플레이어 하나가 `waits_at`에 서 있기만 하는 기본 맵입니다. 마지막 밤은 아직 시작하지 않았습니다. 수치는 `adjust`로 더 바꿉니다.
fn lone_player_before_the_last_night(waits_at: Vec2, adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    let mut s = Scenario::default_game(|t| {
        with_boss(t);
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
        adjust(t);
    });
    s.player_at(waits_at.x, waits_at.y);
    s
}

/// 기본 맵에서 플레이어가 등장하는 자리입니다. 마차에서 가깝습니다.
fn spawn_spot() -> Vec2 {
    Tuning::default().map.player_spawns[0]
}

/// 1명 방의 마지막 밤을 경고부터 시작하고, 보스가 나타난 Tick까지 진행합니다.
fn start_the_last_night(s: &mut Scenario) {
    s.sim.set_phase(Phase::Night {
        day: 3,
        party: 1,
        step: NightStep::Warning { ticks_left: 1 },
    });
    s.run_ticks(1);
}

/// 보스가 나타나는 자리와 마차의 중심 사이를 이은 선 위에서 마차 쪽으로 `distance`타일 온 점입니다. 감시탑이 보스의 길 위에 모이는 자리를 잡는 데 씁니다.
fn on_the_approach(distance: f32) -> Vec2 {
    let mut s = lone_player_before_the_last_night(spawn_spot(), |_| {});
    start_the_last_night(&mut s);
    let id = s.sim.boss().expect("a boss").id;
    let arrival = s.monster(id).expect("the body").pos;
    let center = s.sim.wagon().rect.center();
    center + (arrival - center).normalize_or_zero() * distance
}

/// 감시탑을 보스의 길 위에 상한까지 세우고, 플레이어가 `waits_at`에서 가만히 서 있는 1명 방의 마지막 밤을 원정이 끝나거나 보스가 쓰러질 때까지 진행합니다.
/// 감시탑의 사격에 보스가 줄어든 정도와 끝난 모습을 한 줄로 출력하고 끝난 시나리오를 돌려줍니다.
fn towers_alone(waits_at: Vec2, towers: usize, adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    let anchor = on_the_approach(7.0);
    let mut s = lone_player_before_the_last_night(waits_at, adjust);
    let mut tiles = all_tiles(s.sim.tuning());
    tiles.sort_by(|a, b| {
        let da = a.center().distance(anchor);
        let db = b.center().distance(anchor);
        da.total_cmp(&db).then(a.cmp(b))
    });
    let placed = place_watchtowers_in_order(&mut s, tiles, towers);
    assert_eq!(placed, towers, "the build rule lets the towers be placed");
    start_the_last_night(&mut s);
    let start_hp = boss_hp(&s);
    s.run_until(s.ticks(300.0), |sim| {
        sim.debrief().is_some() || sim.boss().is_none()
    });
    let hp_left = s
        .sim
        .boss()
        .and_then(|boss| s.monster(boss.id))
        .map_or(0, |body| body.hp);
    println!(
        "{towers} towers alone, the player waits at ({}, {}): {:?} at {:.0}s, boss HP {hp_left} of {start_hp}, wagon {}, towers standing {}, players alive {}",
        waits_at.x,
        waits_at.y,
        outcome(&s.sim),
        s.sim.tick() as f32 * s.sim.tuning().dt(),
        s.sim.wagon().hp,
        s.sim.towers().filter(|tower| tower.is_standing()).count(),
        s.sim.players().filter(|player| player.is_alive()).count(),
    );
    s
}

#[test]
fn forty_watchtowers_on_the_boss_path_cannot_win_the_expedition_for_a_room_that_does_nothing() {
    // 포탑의 수가 상한(40개)이고 보스의 길 위에 빽빽이 모여 있어도, 아무도 싸우지 않으면 보스가 포탑에 죽지 않고 원정이 패배로 끝나야 합니다.
    let s = towers_alone(spawn_spot(), 40, |_| {});

    assert_eq!(outcome(&s.sim), Some(Outcome::Defeat));
    assert!(
        s.sim.boss().is_some(),
        "the boss is alive when the expedition is lost"
    );
}

#[test]
fn measure_the_towers_alone_against_a_player_who_waits_far_from_the_wagon() {
    // 측정만 합니다. 플레이어가 마차의 내려찍기에 쓰러지지 않는 먼 곳에서 가만히 있으면 포탑만으로 이기는지, 포탑의 피해 비율마다 출력합니다.
    let far = Vec2::new(2.0, 2.0);
    for percent in [10, 7, 5] {
        for towers in [15, 40] {
            towers_alone(far, towers, |t| {
                t.monsters.boss.resist.tower_damage_percent = percent;
            });
        }
    }
}
