//! 무기 상자를 확인합니다: 낮마다 놓이는 자리와 내용, 씨앗이 정하는 것과 정하지 않는 것, 상자가 막는 타일, 상자를 지키는 몬스터, 밤이 되면 사라지는 상자.
//!
//! 시험하는 것은 `&Sim`이 공개하는 값(상자, 몬스터, 플레이어의 위치, 단계)뿐입니다.
//! 20 x 20 시나리오에서 후보 자리 (2, 2)는 마을의 범위(10타일) 밖에 있고, 상자를 지키는 자리는 동쪽 (3, 2), 서쪽 (1, 2), 남쪽 (2, 3)입니다.

use std::collections::BTreeSet;

use super::support::{Scenario, chest_tuning, nights, phase};
use crate::expedition::Phase;
use crate::input::PlayerInputs;
use crate::kind::WeaponKind;
use crate::loot::Seed;
use crate::monster::{Monster, Order};
use crate::sim::Sim;
use crate::tuning::Tuning;
use crate::world::Tile;

/// 상자의 자리와 내용입니다. 번호 순서입니다.
type Layout = Vec<(Tile, WeaponKind)>;

fn layout(sim: &Sim) -> Layout {
    sim.chests()
        .map(|chest| (chest.tile, chest.contents))
        .collect()
}

/// 기본 맵과 기본 수치의 방을 씨앗 `seed`로 만듭니다.
fn default_sim(seed: u64) -> Sim {
    Sim::new(Tuning::default(), Seed(seed))
}

/// 낮이 1초이고 밤마다 몬스터가 하나인 기본 맵의 방입니다.
fn quick_days(seed: u64) -> Sim {
    let mut tuning = Tuning::default();
    tuning.cycle.day_secs = 1.0;
    tuning.nights.plans = nights(&[&[1], &[1], &[1]]);
    Sim::new(tuning, Seed(seed))
}

/// 밤마다 몬스터를 모두 처치하면서 `days`일째 낮까지 진행하고, 날마다 낮이 시작된 때의 상자를 모읍니다.
fn layouts_of_days(mut sim: Sim, days: u32) -> Vec<Layout> {
    let inputs = PlayerInputs::new();
    let mut found = vec![layout(&sim)];
    for day in 2..=days {
        let mut ticks = 0;
        while !matches!(phase(&sim), Phase::Day { day: today, .. } if today == day) {
            sim.world_mut().monsters.clear();
            sim.step(&inputs);
            ticks += 1;
            assert!(ticks < 5000, "day {day} did not start");
        }
        found.push(layout(&sim));
    }
    found
}

fn post_of(monster: &Monster) -> Tile {
    match monster.order {
        Order::Guard { post } => post,
        Order::Raid | Order::Stand => panic!("{monster:?} is not a guard"),
    }
}

#[test]
fn a_day_places_three_chests_on_three_different_candidate_sites() {
    for seed in 0..40 {
        let sim = default_sim(seed);

        let tiles: Vec<Tile> = sim.chests().map(|chest| chest.tile).collect();

        assert_eq!(tiles.len(), 3, "seed {seed}");
        let distinct: BTreeSet<Tile> = tiles.iter().copied().collect();
        assert_eq!(distinct.len(), 3, "seed {seed}: no two chests share a site");
        assert!(
            tiles
                .iter()
                .all(|tile| sim.tuning().map.chest_sites.contains(tile)),
            "seed {seed}: every chest is on a candidate site"
        );
    }
}

#[test]
fn a_chest_never_holds_the_sword_and_every_other_weapon_comes_out_of_some_chest() {
    let mut seen: BTreeSet<WeaponKind> = BTreeSet::new();

    for seed in 0..60 {
        seen.extend(layout(&default_sim(seed)).into_iter().map(|(_, kind)| kind));
    }

    assert_eq!(
        seen,
        BTreeSet::from([
            WeaponKind::Bow,
            WeaponKind::Greatsword,
            WeaponKind::Spear,
            WeaponKind::Crossbow,
            WeaponKind::Rifle
        ])
    );
}

#[test]
fn the_same_seed_places_the_same_chests_and_other_seeds_place_other_ones() {
    assert_eq!(layout(&default_sim(7)), layout(&default_sim(7)));

    let layouts: BTreeSet<Layout> = (0..20).map(|seed| layout(&default_sim(seed))).collect();
    assert!(layouts.len() > 10, "{} different layouts", layouts.len());
}

/// 씨앗 7의 첫날 상자입니다. 난수를 쓰는 순서나 방법을 바꾸면 같은 씨앗이 다른 맵을 만들게 되므로 이 값이 바뀝니다.
/// 상자가 놓이는 자리는 무기 표와 관계없이 같고, 내용은 상자에서 나오는 무기의 목록이 바뀌면 달라집니다.
const SEED_7_FIRST_DAY: [(Tile, WeaponKind); 3] = [
    (Tile::new(17, 32), WeaponKind::Bow),
    (Tile::new(32, 17), WeaponKind::Rifle),
    (Tile::new(49, 49), WeaponKind::Bow),
];

#[test]
fn seed_7_places_these_three_chests_on_the_first_day() {
    assert_eq!(layout(&default_sim(7)), SEED_7_FIRST_DAY);
}

#[test]
fn the_sites_do_not_depend_on_the_weapon_table() {
    for seed in 0..20 {
        let mut other = Tuning::default();
        other.weapons.greatsword.chest_weight = 9;
        other.weapons.spear.chest_weight = 0;

        let sites = |sim: &Sim| -> Vec<Tile> { sim.chests().map(|chest| chest.tile).collect() };

        assert_eq!(
            sites(&default_sim(seed)),
            sites(&Sim::new(other, Seed(seed))),
            "seed {seed}"
        );
    }
}

#[test]
fn each_day_draws_again_and_the_same_room_seed_repeats_every_day() {
    let first = layouts_of_days(quick_days(1), 3);
    let again = layouts_of_days(quick_days(1), 3);

    assert_eq!(first, again, "the same seed gives the same three days");
    let different: BTreeSet<&Layout> = first.iter().collect();
    assert!(
        different.len() > 1,
        "the days do not all place the same chests"
    );
}

#[test]
fn chest_ids_are_not_reused_on_the_next_day() {
    let mut sim = quick_days(1);
    let before: Vec<_> = sim.chests().map(|chest| chest.id).collect();
    let inputs = PlayerInputs::new();

    while !matches!(phase(&sim), Phase::Day { day: 2, .. }) {
        sim.world_mut().monsters.clear();
        sim.step(&inputs);
    }

    let after: Vec<_> = sim.chests().map(|chest| chest.id).collect();
    assert_eq!(before.len(), 3);
    assert_eq!(after.len(), 3);
    assert!(
        after.iter().all(|id| before.iter().all(|old| id > old)),
        "{before:?} then {after:?}"
    );
}

#[test]
fn a_new_expedition_draws_its_own_chests_and_repeats_for_the_same_room_seed() {
    let second_expedition = || {
        let mut s = Scenario::default_game(|t| t.camp.debrief_secs = 0.1);
        let first = layout(&s.sim);
        s.player_at(32.0, 29.5);
        s.set_wagon_hp(0);
        s.run_until(200, |sim| sim.expedition_number() == 2);
        (first, layout(&s.sim))
    };

    let (first, second) = second_expedition();
    let (first_again, second_again) = second_expedition();

    assert_eq!(first, first_again);
    assert_eq!(second, second_again, "the same room seed repeats");
    assert_ne!(first, second, "another expedition draws again");
}

#[test]
fn a_new_expedition_removes_the_old_chests_and_places_new_ones_with_new_ids() {
    let mut s = Scenario::default_game(|t| t.camp.debrief_secs = 0.1);
    let old: Vec<_> = s.sim.chests().map(|chest| chest.id).collect();
    s.player_at(32.0, 29.5);
    s.set_wagon_hp(0);

    s.run_until(200, |sim| sim.expedition_number() == 2);

    let new: Vec<_> = s.sim.chests().map(|chest| chest.id).collect();
    assert_eq!(old.len(), 3);
    assert_eq!(
        new.len(),
        3,
        "the new expedition's first day has its own chests"
    );
    assert!(new.iter().all(|id| old.iter().all(|before| id > before)));
}

#[test]
fn a_chest_blocks_its_tile_for_a_walking_player() {
    let mut s = Scenario::with_chest(WeaponKind::Spear, 0);
    let a = s.player_at(2.5, 4.5);
    s.hold_move(a, 0.0, -1.0);

    s.run_ticks(40);

    let radius = s.sim.tuning().player.radius;
    assert!(
        s.player(a).pos.y >= 3.0 + radius - 1e-3,
        "the player stopped at the chest, at {:?}",
        s.player(a).pos
    );
}

#[test]
fn after_nightfall_a_player_walks_onto_the_tile_the_chest_stood_on() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.nights.plans = nights(&[&[1], &[1]]);
        chest_tuning(t, WeaponKind::Spear, 0);
    });
    s.run_until(200, |sim| matches!(phase(sim), Phase::Night { .. }));
    let a = s.player_at(2.5, 4.5);
    s.hold_move(a, 0.0, -1.0);

    s.run_ticks(40);

    assert!(
        s.player(a).pos.y < 3.0,
        "the player walked onto the freed tile, at {:?}",
        s.player(a).pos
    );
}

#[test]
fn a_player_standing_on_a_site_when_the_day_starts_is_pushed_off_the_chest() {
    let mut s = Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.nights.plans = nights(&[&[1], &[1]]);
        chest_tuning(t, WeaponKind::Spear, 0);
    });
    s.run_until(200, |sim| matches!(phase(sim), Phase::Night { .. }));
    assert_eq!(s.sim.chests().count(), 0, "the chest left at nightfall");
    let a = s.player_at(2.5, 2.5);
    s.clear_monsters();

    s.run_ticks(10);
    assert!(
        (s.player(a).pos.x - 2.5).abs() < 1e-3 && (s.player(a).pos.y - 2.5).abs() < 1e-3,
        "a player who gives no input stays where they stand at night"
    );
    while !matches!(phase(&s.sim), Phase::Day { day: 2, .. }) {
        s.clear_monsters();
        s.run_ticks(1);
    }

    assert_eq!(s.sim.chests().count(), 1);
    let gap = Tile::new(2, 2).rect().distance_to(s.player(a).pos);
    assert!(
        gap >= s.sim.tuning().player.radius - 1e-3,
        "the player is outside the chest tile, {gap} tiles from it"
    );
}

#[test]
fn three_guards_stand_on_free_tiles_beside_every_chest() {
    let s = Scenario::default_game(|t| t.guards.per_node = 0);

    let guards: Vec<Monster> = s.sim.monsters().cloned().collect();

    assert_eq!(guards.len(), 9, "three for each of the three chests");
    for chest in s.sim.chests() {
        let beside: Vec<&Monster> = guards
            .iter()
            .filter(|guard| {
                let post = post_of(guard);
                post.x.abs_diff(chest.tile.x) <= 1 && post.y.abs_diff(chest.tile.y) <= 1
            })
            .collect();
        assert_eq!(beside.len(), 3, "chest at {:?}", chest.tile);
    }
    for guard in &guards {
        let post = post_of(guard);
        assert!(
            s.sim.chests().all(|chest| chest.tile != post),
            "{guard:?} does not stand on a chest"
        );
        assert!(guard.pos.distance(post.center()) < 0.1, "{guard:?}");
    }
}

#[test]
fn a_site_without_a_chest_today_has_no_guards() {
    let s = Scenario::default_game(|t| t.guards.per_node = 0);

    let chest_tiles: Vec<Tile> = s.sim.chests().map(|chest| chest.tile).collect();
    let empty_sites: Vec<Tile> = s
        .sim
        .tuning()
        .map
        .chest_sites
        .iter()
        .copied()
        .filter(|site| !chest_tiles.contains(site))
        .collect();

    assert_eq!(empty_sites.len(), 5);
    for guard in s.sim.monsters() {
        let post = post_of(guard);
        assert!(
            empty_sites
                .iter()
                .all(|site| post.x.abs_diff(site.x) > 1 || post.y.abs_diff(site.y) > 1),
            "{guard:?} stands beside a site that has no chest"
        );
    }
}

#[test]
fn at_nightfall_the_unopened_chests_vanish_and_their_guards_march_to_the_wagon() {
    let mut s = Scenario::default_game(|t| {
        t.guards.per_node = 0;
        t.cycle.day_secs = 0.5;
        t.nights.plans = nights(&[&[1]]);
    });
    assert_eq!(s.sim.chests().count(), 3);
    assert_eq!(s.sim.monsters().count(), 9);

    s.run_until(100, |sim| matches!(phase(sim), Phase::Night { .. }));

    assert_eq!(s.sim.chests().count(), 0);
    assert!(
        s.sim.monsters().all(|monster| monster.order == Order::Raid),
        "every guard now marches to the wagon"
    );
    assert_eq!(s.sim.monsters().count(), 9, "none of them left or stayed");
}
