//! 몬스터 300마리가 있고 감시탑이 상한만큼 서 있을 때 Tick 하나를 계산하는 데 걸리는 시간을 잽니다.
//!
//! 시간의 한계는 릴리스 빌드에서만 확인합니다. 최적화하지 않은 빌드의 시간은 한계와 관계가 없기 때문입니다.
//! 일반 테스트에서는 측정 상황이 말한 대로 만들어졌는지(몬스터 300마리, 감시탑 상한, 건설과 파괴가 실제로 일어남, 탄환이 실제로 날아다님, 석궁의 탄환이 몬스터를 꿰뚫음, 소총이 몬스터를 맞힘)만 확인합니다.
//! 시간을 보려면 다음 명령을 실행합니다.
//!
//! ```text
//! cargo test --release -p caravan_rules perf -- --nocapture
//! ```
//!
//! 재는 것은 `Sim::step` 하나가 걸린 시간이고 네트워크와 스냅샷 부호화는 들어 있지 않습니다. 그 둘을 합친 시간은 `caravan_server`의 `load` 테스트가 잽니다.

use std::time::{Duration, Instant};

use super::support::{Scenario, nights};
use crate::ids::TowerId;
use crate::input::{AimDir, PlayerInput};
use crate::kind::WeaponKind;
use crate::math::Vec2;
use crate::structures::{Tower, TowerState};
use crate::tuning::Tuning;
use crate::world::Tile;

/// 몬스터 300마리가 1 Tick마다 한 마리씩 나타나서 마차로 몰려드는 기본 맵입니다. 마차와 플레이어는 죽지 않습니다.
/// 감시탑은 `towers`에 적힌 자리에 서 있고 HP가 `tower_hp`입니다. 감시탑은 사격하지만 피해를 주지 않아서 몬스터가 처치되지 않고,
/// 그래서 300마리가 계속 남습니다. 사격할 대상을 고르는 계산은 피해를 줄 때와 같고, 달라지는 것은 피해를 적용하는 한 줄뿐입니다.
fn village_under_siege(towers: Vec<Tile>, tower_hp: u32) -> Scenario {
    village_under_siege_with(towers, tower_hp, |_| {})
}

/// `village_under_siege`와 같지만 수치를 `adjust`로 더 바꿉니다.
fn village_under_siege_with(
    towers: Vec<Tile>,
    tower_hp: u32,
    adjust: impl FnOnce(&mut Tuning),
) -> Scenario {
    Scenario::default_game(|t| {
        t.cycle.day_secs = t.dt();
        t.nights.plans = nights(&[&[300]]);
        t.nights.first_wave_delay_secs = t.dt();
        t.nights.spawn_interval_secs = t.dt();
        t.wagon.max_hp = u32::MAX;
        t.player.max_hp = u32::MAX;
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
        t.village.start_towers = towers;
        t.structures.watchtower.max_hp = tower_hp;
        t.structures.watchtower.damage = 0;
        adjust(t);
    })
}

/// 몬스터 300마리가 모두 나타날 때까지 진행합니다.
fn until_the_crowd_is_there(s: &mut Scenario) {
    s.run_until(600, |sim| sim.monsters().count() == 300);
}

fn standing_towers(s: &Scenario) -> usize {
    s.sim.towers().filter(|tower| tower.is_standing()).count()
}

/// 한 Tick을 진행하고 걸린 시간을 돌려줍니다.
fn timed_tick(s: &mut Scenario) -> Duration {
    let started = Instant::now();
    s.run_ticks(1);
    started.elapsed()
}

/// Tick 간격에 대한 비율로 정한 시간 한계입니다. 릴리스 빌드에서만 확인합니다.
fn check_limit(what: &str, took: Duration, limit: Duration) {
    println!("{what}: {took:?} (limit {limit:?})");
    if !cfg!(debug_assertions) {
        assert!(
            took <= limit,
            "{what} took {took:?}, over the limit {limit:?}"
        );
    }
}

/// 몬스터 무리가 나타나는 동쪽에서 가장 가까운 감시탑의 HP를 1로 줄입니다. 무리가 곧 부숩니다.
fn weaken_the_eastmost_tower(s: &mut Scenario) {
    let eastmost = s
        .sim
        .towers()
        .max_by_key(|tower| tower.tile.x)
        .map(|tower| tower.id)
        .expect("a tower stands");
    if let Some(Tower {
        state: TowerState::Standing { hp, .. },
        ..
    }) = s.sim.world_mut().towers.get_mut(&eastmost)
    {
        *hp = 1;
    }
}

#[test]
fn perf_the_average_tick_with_the_tower_cap_and_300_monsters_is_a_tenth_of_the_interval_at_most() {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let towers = defaults.spread_tower_sites(defaults.village.tower_limit);
    assert_eq!(towers.len(), cap, "the village has room for the cap");
    let mut s = village_under_siege(towers, u32::MAX);
    until_the_crowd_is_there(&mut s);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");

    let mut total = Duration::ZERO;
    let mut slowest = Duration::ZERO;
    for _ in 0..1000 {
        let took = timed_tick(&mut s);
        total += took;
        slowest = slowest.max(took);
    }
    assert_eq!(
        s.sim.monsters().count(),
        300,
        "300 monsters in every sampled tick"
    );
    assert_eq!(standing_towers(&s), cap, "the cap in every sampled tick");

    let interval = Duration::from_secs_f32(defaults.dt());
    println!("slowest of 1000 ticks: {slowest:?}");
    check_limit("average of 1000 ticks", total / 1000, interval / 10);
}

/// 한 Tick에 감시탑 모음에서 일어난 변화입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    BuildStarted,
    BuildFinished,
    Destroyed,
}

/// 감시탑의 번호와 서 있는지를 번호 순서로 적은 목록입니다. 두 시점의 목록을 비교해서 변화를 찾습니다.
fn towers_now(s: &Scenario) -> Vec<(TowerId, bool)> {
    s.sim
        .towers()
        .map(|tower| (tower.id, tower.is_standing()))
        .collect()
}

fn changes_between(before: &[(TowerId, bool)], after: &[(TowerId, bool)]) -> Vec<Change> {
    let mut found = Vec::new();
    for (id, standing) in after {
        match before.iter().find(|(earlier, _)| earlier == id) {
            None => found.push(Change::BuildStarted),
            Some((_, false)) if *standing => found.push(Change::BuildFinished),
            Some(_) => {}
        }
    }
    for (id, _) in before {
        if !after.iter().any(|(later, _)| later == id) {
            found.push(Change::Destroyed);
        }
    }
    found
}

#[test]
fn perf_a_tick_that_builds_or_destroys_a_tower_fits_inside_one_interval() {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let all = defaults.spread_tower_sites(defaults.village.tower_limit);
    // 지을 자리는 몬스터 무리가 몰리는 동쪽의 반대편인 서쪽에서 고르고, 나머지 감시탑은 처음부터 서 있습니다.
    let site = all
        .iter()
        .rev()
        .copied()
        .find(|tile| tile.x <= 26)
        .expect("the village has a site on the west side");
    let start: Vec<Tile> = all.iter().copied().filter(|tile| *tile != site).collect();
    let mut s = village_under_siege(start, u32::MAX);
    until_the_crowd_is_there(&mut s);
    assert_eq!(standing_towers(&s), cap - 1, "one place is left to the cap");
    let builder = s.player_at(f32::from(site.x) - 0.5, f32::from(site.y) + 0.5);
    s.hold_build(builder, 1.0, 0.0);

    // Tick마다 걸린 시간과 그 Tick에 일어난 변화를 적습니다. 건설이 끝난 뒤(감시탑이 상한만큼 선 뒤)에 가장 동쪽에 선 감시탑의 HP를 1로 줄이면
    // 무리가 그 감시탑을 부숩니다.
    let weakened_at = 100;
    let mut ticks: Vec<(Duration, Vec<Change>)> = Vec::new();
    for tick in 0..600 {
        if tick == weakened_at {
            assert_eq!(
                standing_towers(&s),
                cap,
                "the cap is reached before one is destroyed"
            );
            weaken_the_eastmost_tower(&mut s);
        }
        let before = towers_now(&s);
        let took = timed_tick(&mut s);
        ticks.push((took, changes_between(&before, &towers_now(&s))));
    }
    assert_eq!(s.sim.monsters().count(), 300, "300 monsters the whole time");

    // 몬스터가 감시탑을 부순 Tick의 이동 경로 계산은 다음 Tick의 처음에 일어나므로, 변화가 일어난 Tick과 그 다음 Tick 가운데 느린 쪽을 그 변화의 시간으로 칩니다.
    let around = |change: Change| -> Vec<Duration> {
        (0..ticks.len())
            .filter(|&i| ticks[i].1.contains(&change))
            .map(|i| {
                ticks[i]
                    .0
                    .max(ticks.get(i + 1).map_or(Duration::ZERO, |next| next.0))
            })
            .collect()
    };
    let started = around(Change::BuildStarted);
    let finished = around(Change::BuildFinished);
    let destroyed = around(Change::Destroyed);
    assert_eq!(started.len(), 1, "the tower was started");
    assert_eq!(finished.len(), 1, "the tower was finished");
    assert_eq!(destroyed.len(), 1, "the weakened tower was destroyed");

    let interval = Duration::from_secs_f32(defaults.dt());
    let slowest_of_all = ticks
        .iter()
        .map(|(took, _)| *took)
        .max()
        .unwrap_or_default();
    println!(
        "{cap} towers and 300 monsters; one build and one destroyed tower in 600 ticks; slowest of all 600 ticks: {slowest_of_all:?}"
    );
    check_limit("tick pair around the build start", started[0], interval);
    check_limit("tick pair around the build finish", finished[0], interval);
    check_limit(
        "tick pair around the destroyed tower",
        destroyed[0],
        interval,
    );
}

/// `weapon`을 든 플레이어를 `pos`에 세우고 `aim` 방향으로 쉬지 않고 쏘게 합니다.
fn add_shooter(s: &mut Scenario, weapon: WeaponKind, pos: Vec2, aim: (f32, f32)) {
    let id = s.player_at(pos.x, pos.y);
    s.give_weapon(id, weapon);
    s.hold(
        id,
        PlayerInput {
            aim: AimDir::new(aim.0, aim.1),
            attack: true,
            ..PlayerInput::default()
        },
    );
}

/// `measure_with_arrows`가 돌려주는 값입니다.
struct Measured {
    /// Tick 하나의 평균 시간입니다.
    average: Duration,
    /// 한 Tick에 날아다닌 탄환의 최대 수입니다. 탄환이 실제로 날아다니는지 알려면 숫자를 보기 전에 확인해야 합니다.
    most_flying: usize,
    /// 지켜본 Tick 동안 쏜 탄환의 가장 큰 번호입니다. 탄환의 번호는 쏜 순서로 1부터 매기므로 쏜 수를 셀 때 씁니다.
    last_bolt: u32,
}

/// 1000 Tick을 진행하고 Tick 하나의 평균 시간과 가장 느린 시간, 날아다니는 탄환의 평균과 최대 수를 출력합니다.
/// `last_bolt`는 이 측정이 시작될 때까지 쏜 탄환의 가장 큰 번호이고, `Measured::last_bolt`에서 이 값을 뺀 것이 측정하는 동안 쏜 수입니다.
fn measure_with_arrows(s: &mut Scenario, last_bolt: u32) -> Measured {
    let mut total = Duration::ZERO;
    let mut slowest = Duration::ZERO;
    let mut flying_sum = 0;
    let mut most_flying = 0;
    let mut last_bolt = last_bolt;
    for _ in 0..1000 {
        let took = timed_tick(s);
        total += took;
        slowest = slowest.max(took);
        flying_sum += s.flying();
        most_flying = most_flying.max(s.flying());
        last_bolt = last_bolt.max(bolts_launched(s));
    }
    println!(
        "slowest of 1000 ticks: {slowest:?}; arrows in the air: average {:.1}, most {most_flying}",
        flying_sum as f32 / 1000.0
    );
    Measured {
        average: total / 1000,
        most_flying,
        last_bolt,
    }
}

/// 지금 세계에 있는 탄환 가운데 가장 큰 번호입니다. 탄환이 없으면 0입니다.
fn bolts_launched(s: &Scenario) -> u32 {
    s.sim.projectiles().map(|bolt| bolt.id.0).max().unwrap_or(0)
}

/// `ticks` Tick을 진행하면서 쏜 탄환의 가장 큰 번호를 돌려줍니다. 탄환은 쏜 Tick에 세계에 있으므로 한 Tick씩 진행하면 놓치지 않습니다.
fn run_and_count_bolts(s: &mut Scenario, ticks: u32) -> u32 {
    let mut last_bolt = 0;
    for _ in 0..ticks {
        s.run_ticks(1);
        last_bolt = last_bolt.max(bolts_launched(s));
    }
    last_bolt
}

#[test]
fn perf_the_average_tick_with_the_tower_cap_300_monsters_and_four_archers_is_a_tenth_of_the_interval_at_most()
 {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let towers = defaults.spread_tower_sites(defaults.village.tower_limit);
    assert_eq!(towers.len(), cap, "the village has room for the cap");
    // 몬스터가 활에 맞아 죽으면 측정 상황이 달라지므로 HP를 가장 크게 합니다.
    let mut s = village_under_siege_with(towers, u32::MAX, |t| t.monsters.grunt.max_hp = u32::MAX);
    // 플레이어 넷이 등장 지점에서 동서남북으로 쉬지 않고 쏩니다.
    let aims = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];
    for (spawn, aim) in defaults.map.player_spawns.iter().zip(aims) {
        add_shooter(&mut s, WeaponKind::Bow, *spawn, aim);
    }
    until_the_crowd_is_there(&mut s);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");

    let measured = measure_with_arrows(&mut s, 0);

    assert_eq!(
        s.sim.monsters().count(),
        300,
        "300 monsters in every sampled tick"
    );
    assert!(
        measured.most_flying >= 1,
        "arrows flew during the measurement"
    );
    let interval = Duration::from_secs_f32(defaults.dt());
    check_limit(
        "average of 1000 ticks with four archers",
        measured.average,
        interval / 10,
    );
}

#[test]
fn perf_the_average_tick_with_the_tower_cap_300_monsters_and_over_a_hundred_arrows_in_the_air_is_a_tenth_of_the_interval_at_most()
 {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let towers = defaults.spread_tower_sites(defaults.village.tower_limit);
    // 열두 명이 한 Tick마다 쏘면 탄환 하나가 13 Tick 날아가므로 열린 땅에서 150개쯤이 동시에 날아다닙니다.
    // 규칙이 허용하는 네 명이 쏘는 것보다 훨씬 많은 탄환입니다.
    let archers = 12;
    let mut s = village_under_siege_with(towers, u32::MAX, |t| {
        t.monsters.grunt.max_hp = u32::MAX;
        t.max_players = archers;
        t.weapons.bow.cooldown_secs = t.dt();
    });
    // 몬스터가 오지 않는 서쪽 가장자리에서 북쪽으로 쏩니다. 탄환은 사거리를 다 날아갑니다.
    for i in 0..archers {
        add_shooter(
            &mut s,
            WeaponKind::Bow,
            Vec2::new(2.5, 3.0 + 4.0 * i as f32),
            (0.0, 1.0),
        );
    }
    until_the_crowd_is_there(&mut s);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");

    let measured = measure_with_arrows(&mut s, 0);

    assert_eq!(
        s.sim.monsters().count(),
        300,
        "300 monsters in every sampled tick"
    );
    assert!(
        measured.most_flying >= 100,
        "more than a hundred arrows flew at once, but only {}",
        measured.most_flying
    );
    let interval = Duration::from_secs_f32(defaults.dt());
    check_limit(
        "average of 1000 ticks with many arrows",
        measured.average,
        interval / 10,
    );
}

/// 무리가 쌓인 동쪽 바깥에 `weapon`을 든 플레이어 넷을 나란히 세우고 서쪽(무리와 마을)을 향해 쉬지 않고 쏘게 합니다.
/// 첫 무리는 동쪽에서 나타나서 마을의 동쪽 가장자리에 쌓이므로, 이 자리에서 쏘는 것은 무리를 가로지릅니다.
fn add_four_shooters_east_of_the_crowd(s: &mut Scenario, weapon: WeaponKind) {
    for y in [30.5, 31.5, 32.5, 33.5] {
        add_shooter(s, weapon, Vec2::new(56.0, y), (-1.0, 0.0));
    }
}

/// 몬스터가 지금까지 입은 피해의 합입니다. 몬스터의 HP가 가장 클 때(`u32::MAX`) 맞은 만큼 줄어듭니다.
fn damage_taken_by_monsters(s: &Scenario) -> u64 {
    s.sim
        .monsters()
        .map(|monster| u64::from(u32::MAX - monster.hp))
        .sum()
}

#[test]
fn perf_the_average_tick_with_the_tower_cap_300_monsters_and_four_crossbows_piercing_the_crowd_is_a_tenth_of_the_interval_at_most()
 {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let towers = defaults.spread_tower_sites(defaults.village.tower_limit);
    let mut s = village_under_siege_with(towers, u32::MAX, |t| t.monsters.grunt.max_hp = u32::MAX);
    add_four_shooters_east_of_the_crowd(&mut s, WeaponKind::Crossbow);
    until_the_crowd_is_there(&mut s);
    let last_bolt = run_and_count_bolts(&mut s, 600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");
    let damage_before = damage_taken_by_monsters(&s);

    let measured = measure_with_arrows(&mut s, last_bolt);

    assert_eq!(
        s.sim.monsters().count(),
        300,
        "300 monsters in every sampled tick"
    );
    // 탄환 하나가 몬스터를 하나만 맞히면 맞힌 수와 쏜 수가 같습니다. 1보다 커야 탄환이 무리를 꿰뚫은 것입니다.
    let hits = (damage_taken_by_monsters(&s) - damage_before)
        / u64::from(defaults.weapons.crossbow.damage);
    let bolts = measured.last_bolt - last_bolt;
    let per_bolt = hits as f32 / bolts as f32;
    println!("{bolts} bolts made {hits} hits during the measurement: {per_bolt:.2} hits per bolt");
    assert!(
        per_bolt > 1.5,
        "the bolts passed through the crowd: {per_bolt} hits per bolt"
    );
    let interval = Duration::from_secs_f32(defaults.dt());
    check_limit(
        "average of 1000 ticks with four crossbows",
        measured.average,
        interval / 10,
    );
}

#[test]
fn perf_the_average_tick_with_the_tower_cap_300_monsters_and_four_rifles_firing_is_a_tenth_of_the_interval_at_most()
 {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let towers = defaults.spread_tower_sites(defaults.village.tower_limit);
    let mut s = village_under_siege_with(towers, u32::MAX, |t| t.monsters.grunt.max_hp = u32::MAX);
    add_four_shooters_east_of_the_crowd(&mut s, WeaponKind::Rifle);
    until_the_crowd_is_there(&mut s);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");
    let damage_before = damage_taken_by_monsters(&s);

    let measured = measure_with_arrows(&mut s, 0);

    assert_eq!(
        s.sim.monsters().count(),
        300,
        "300 monsters in every sampled tick"
    );
    assert_eq!(measured.most_flying, 0, "a rifle shot does not fly");
    let hits =
        (damage_taken_by_monsters(&s) - damage_before) / u64::from(defaults.weapons.rifle.damage);
    // 1000 Tick은 50초이고 소총의 쿨다운은 1.5초이므로 플레이어 하나가 34발쯤 쏩니다.
    println!("{hits} rifle hits during the measurement");
    assert!(
        hits >= 100,
        "four rifles hit the crowd, but only {hits} hits"
    );
    let interval = Duration::from_secs_f32(defaults.dt());
    check_limit(
        "average of 1000 ticks with four rifles",
        measured.average,
        interval / 10,
    );
}

#[test]
fn perf_the_average_tick_with_the_tower_cap_300_monsters_and_four_rifles_firing_every_tick_is_a_tenth_of_the_interval_at_most()
 {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let towers = defaults.spread_tower_sites(defaults.village.tower_limit);
    // 쿨다운을 한 Tick으로 줄여서 규칙이 허용하는 것보다 훨씬 자주 쏘게 합니다. 한 Tick에 네 발입니다.
    let mut s = village_under_siege_with(towers, u32::MAX, |t| {
        t.monsters.grunt.max_hp = u32::MAX;
        t.weapons.rifle.cooldown_secs = t.dt();
    });
    add_four_shooters_east_of_the_crowd(&mut s, WeaponKind::Rifle);
    until_the_crowd_is_there(&mut s);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");
    let damage_before = damage_taken_by_monsters(&s);

    let measured = measure_with_arrows(&mut s, 0);

    let hits =
        (damage_taken_by_monsters(&s) - damage_before) / u64::from(defaults.weapons.rifle.damage);
    println!("{hits} rifle hits during the measurement");
    assert!(
        hits >= 3000,
        "four rifles hit the crowd every tick, but only {hits} hits"
    );
    let interval = Duration::from_secs_f32(defaults.dt());
    check_limit(
        "average of 1000 ticks with four rifles every tick",
        measured.average,
        interval / 10,
    );
}

#[test]
fn perf_the_tick_pair_around_opening_a_chest_with_the_tower_cap_and_300_monsters_fits_inside_one_interval()
 {
    let defaults = Tuning::default();
    let cap = defaults.village.tower_limit as usize;
    let mut s = Scenario::default_game(|t| {
        // 상자를 여는 플레이어를 지키는 몬스터가 막지 않게 하고, 측정하는 것은 상자가 열리는 Tick의 계산입니다.
        t.guards.per_chest = 0;
        t.village.start_towers = t.spread_tower_sites(t.village.tower_limit);
        t.structures.watchtower.max_hp = u32::MAX;
        t.structures.watchtower.damage = 0;
        t.wagon.max_hp = u32::MAX;
        t.player.max_hp = u32::MAX;
    });
    assert_eq!(standing_towers(&s), cap, "the village starts at the cap");
    // 낮에는 채집 지점을 지키는 몬스터만 있으므로, 나머지를 마차로 향하는 몬스터로 채워서 300마리로 맞춥니다.
    let guards = s.sim.monsters().count();
    for i in 0..(300 - guards) {
        s.monster_at(52.5 + (i % 10) as f32, 10.5 + (i / 10) as f32);
    }
    assert_eq!(s.sim.monsters().count(), 300);

    // 상자의 상하좌우 이웃 가운데 빈 타일에 서서 F를 누르고 있습니다.
    let chest = s.sim.chests().next().expect("the day has chests").tile;
    let stand = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(|(dx, dy)| chest.step(dx, dy))
        .find(|tile| s.sim.world_mut().tiles.is_free(*tile))
        .expect("a chest site has a free side tile");
    let opener = s.player_at(stand.center().x, stand.center().y);
    s.hold_interact(opener);

    let chests_before = s.sim.chests().count();
    let mut ticks: Vec<(Duration, usize)> = Vec::new();
    for _ in 0..60 {
        let took = timed_tick(&mut s);
        ticks.push((took, s.sim.chests().count()));
    }
    assert_eq!(s.sim.monsters().count(), 300, "300 monsters the whole time");
    let opened_at = ticks
        .iter()
        .position(|(_, chests)| *chests < chests_before)
        .expect("the chest was opened");
    assert_eq!(
        s.sim.ground_weapons().count(),
        1,
        "the opening left one weapon"
    );

    // 상자가 열린 Tick의 이동 경로 계산은 다음 Tick의 처음에 일어나므로, 두 Tick 가운데 느린 쪽을 열기의 시간으로 칩니다.
    let pair = ticks[opened_at].0.max(
        ticks
            .get(opened_at + 1)
            .map_or(Duration::ZERO, |next| next.0),
    );
    let slowest_of_all = ticks
        .iter()
        .map(|(took, _)| *took)
        .max()
        .unwrap_or_default();
    let interval = Duration::from_secs_f32(defaults.dt());
    println!(
        "{cap} towers and 300 monsters; one chest opened in 60 ticks; slowest of all 60 ticks: {slowest_of_all:?}"
    );
    check_limit("tick pair around the chest opening", pair, interval);
}
