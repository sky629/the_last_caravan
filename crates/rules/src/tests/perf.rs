//! 몬스터 300마리가 있고 감시탑이 상한만큼 서 있을 때 Tick 하나를 계산하는 데 걸리는 시간을 잽니다.
//!
//! 시간의 한계는 릴리스 빌드에서만 확인합니다. 최적화하지 않은 빌드의 시간은 한계와 관계가 없기 때문입니다.
//! 일반 테스트에서는 측정 상황이 말한 대로 만들어졌는지(몬스터 300마리, 감시탑 상한, 건설과 파괴가 실제로 일어남, 탄환이 실제로 날아다님)만 확인합니다.
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

/// 활을 든 플레이어를 `pos`에 세우고 `aim` 방향으로 쉬지 않고 쏘게 합니다.
fn add_archer(s: &mut Scenario, pos: Vec2, aim: (f32, f32)) {
    let id = s.player_at(pos.x, pos.y);
    s.give_weapon(id, WeaponKind::Bow);
    s.hold(
        id,
        PlayerInput {
            aim: AimDir::new(aim.0, aim.1),
            attack: true,
            ..PlayerInput::default()
        },
    );
}

/// 1000 Tick을 진행하고 Tick 하나의 평균 시간과 가장 느린 시간, 날아다니는 탄환의 평균과 최대 수를 출력한 뒤 평균 시간을 돌려줍니다.
/// 탄환이 실제로 날아다니는지 알려면 숫자를 보기 전에 탄환의 수를 확인해야 하므로, 탄환의 최대 수도 함께 돌려줍니다.
fn measure_with_arrows(s: &mut Scenario) -> (Duration, usize) {
    let mut total = Duration::ZERO;
    let mut slowest = Duration::ZERO;
    let mut flying_sum = 0;
    let mut most_flying = 0;
    for _ in 0..1000 {
        let took = timed_tick(s);
        total += took;
        slowest = slowest.max(took);
        flying_sum += s.flying();
        most_flying = most_flying.max(s.flying());
    }
    println!(
        "slowest of 1000 ticks: {slowest:?}; arrows in the air: average {:.1}, most {most_flying}",
        flying_sum as f32 / 1000.0
    );
    (total / 1000, most_flying)
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
        add_archer(&mut s, *spawn, aim);
    }
    until_the_crowd_is_there(&mut s);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");

    let (average, most_flying) = measure_with_arrows(&mut s);

    assert_eq!(
        s.sim.monsters().count(),
        300,
        "300 monsters in every sampled tick"
    );
    assert!(most_flying >= 1, "arrows flew during the measurement");
    let interval = Duration::from_secs_f32(defaults.dt());
    check_limit(
        "average of 1000 ticks with four archers",
        average,
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
        add_archer(&mut s, Vec2::new(2.5, 3.0 + 4.0 * i as f32), (0.0, 1.0));
    }
    until_the_crowd_is_there(&mut s);
    s.run_ticks(600);
    assert_eq!(s.sim.monsters().count(), 300, "nobody died");
    assert_eq!(standing_towers(&s), cap, "the tower cap is reached");

    let (average, most_flying) = measure_with_arrows(&mut s);

    assert_eq!(
        s.sim.monsters().count(),
        300,
        "300 monsters in every sampled tick"
    );
    assert!(
        most_flying >= 100,
        "more than a hundred arrows flew at once, but only {most_flying}"
    );
    let interval = Duration::from_secs_f32(defaults.dt());
    check_limit(
        "average of 1000 ticks with many arrows",
        average,
        interval / 10,
    );
}
