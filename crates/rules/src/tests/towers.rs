//! 감시탑의 사격, 피해와 파괴, 수리를 확인합니다.
//!
//! 시나리오의 감시탑은 테스트가 놓은 타일 (1, 17)이고, 사각형은 (1, 17)에서 (2, 18)까지, 중심은 (1.5, 17.5)입니다.

use super::support::Scenario;
use crate::build::CannotBuild;
use crate::expedition::{Outcome, Phase};
use crate::input::PlayerInput;
use crate::math::Rect;
use crate::structures::{Shot, TowerState};
use crate::tuning::Tuning;
use crate::world::Tile;

/// 타일 (1, 17)에 서 있는 감시탑이 하나 있는 빈 들판입니다.
fn field_with_tower() -> Scenario {
    field_with_tower_tuned(|_| {})
}

fn field_with_tower_tuned(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
    let mut s = Scenario::with_tuning(adjust);
    s.tower_at(1, 17);
    s
}

fn press_f() -> PlayerInput {
    PlayerInput {
        interact: true,
        ..PlayerInput::default()
    }
}

/// 감시탑 곁에서 F를 누르고 있는 플레이어와, HP가 100인 감시탑입니다.
fn repairer_at_a_damaged_tower() -> (Scenario, crate::ids::PlayerId) {
    let mut s = field_with_tower();
    let a = s.player_at(2.5, 17.5);
    s.set_tower_hp(100);
    s.hold(a, press_f());
    (s, a)
}

#[test]
fn a_tower_shoots_the_nearest_monster_in_range() {
    let mut s = field_with_tower_tuned(|t| t.monsters.grunt.max_hp = 1000);
    let near = s.monster_at(4.0, 17.5);
    let far = s.monster_at(6.0, 17.5);

    s.run_ticks(1);

    assert_eq!(s.monster(near).unwrap().hp, 1000 - 15);
    assert_eq!(s.monster(far).unwrap().hp, 1000);
    let TowerState::Standing { last_shot, .. } = &s.tower().state else {
        panic!("the tower stands");
    };
    assert_eq!(
        *last_shot,
        Some(Shot {
            target: near,
            tick: 1
        }),
        "visible in the state"
    );
}

#[test]
fn a_tower_ignores_monsters_beyond_its_range() {
    let mut s = field_with_tower_tuned(|t| t.monsters.grunt.aggro_range = 0.0);
    let monster = s.monster_at(9.0, 17.5);

    s.run_ticks(1);

    assert_eq!(
        s.monster(monster).unwrap().hp,
        60,
        "7.5 tiles is beyond the range of 6"
    );
}

#[test]
fn a_tower_shoots_again_after_its_cooldown() {
    let mut s = field_with_tower_tuned(|t| {
        t.monsters.grunt.max_hp = 1000;
        t.monsters.grunt.aggro_range = 0.0;
        t.monsters.grunt.speed = 0.5;
    });
    let monster = s.monster_at(3.0, 17.5);

    s.run_ticks(s.ticks(0.8));
    assert_eq!(
        s.monster(monster).unwrap().hp,
        1000 - 15,
        "one shot in the first 16 ticks"
    );

    s.run_ticks(1);
    assert_eq!(s.monster(monster).unwrap().hp, 1000 - 30, "the second shot");
}

#[test]
fn towers_are_kept_by_id_and_each_one_shoots() {
    let mut s = field_with_tower_tuned(|t| t.monsters.grunt.max_hp = 1000);
    s.tower_at(1, 14);
    let monster = s.monster_at(3.0, 16.0);

    s.run_ticks(1);

    assert_eq!(s.sim.towers().count(), 2);
    let ids: Vec<_> = s.sim.towers().map(|tower| tower.id).collect();
    assert_ne!(ids[0], ids[1]);
    assert_eq!(
        s.monster(monster).unwrap().hp,
        1000 - 2 * 15,
        "both towers are in range"
    );
}

#[test]
fn a_monster_in_range_goes_for_the_tower_and_hurts_it() {
    let mut s = field_with_tower();
    s.monster_at(3.0, 17.5);
    let max_hp = s.sim.tuning().structures.watchtower.max_hp;

    s.run_ticks(s.ticks(1.0));

    assert!(s.tower().hp() < max_hp);
    assert_eq!(
        s.sim.wagon().hp,
        500,
        "the monster chose the tower, not the wagon"
    );
}

#[test]
fn zero_hp_removes_the_tower_and_frees_its_tile() {
    let mut s = field_with_tower();
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);

    s.run_until(100, |sim| sim.towers().count() == 0);

    assert_eq!(s.sim.towers().count(), 0, "no rubble is left");
}

#[test]
fn a_destroyed_tower_stops_shooting() {
    let mut s = field_with_tower_tuned(|t| t.monsters.grunt.max_hp = 1000);
    s.set_tower_hp(10);
    let attacker = s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| sim.towers().count() == 0);
    let hp_when_destroyed = s.monster(attacker).unwrap().hp;

    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.monster(attacker).unwrap().hp, hp_when_destroyed);
}

#[test]
fn monsters_do_not_attack_a_tower_that_is_gone() {
    let mut s = field_with_tower();
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| sim.towers().count() == 0);
    s.clear_monsters();

    let fresh = s.monster_at(3.0, 17.5);
    s.run_ticks(s.ticks(3.0));

    assert_eq!(s.monster(fresh).unwrap().last_attack_tick, None);
    assert!(
        s.monster(fresh).unwrap().pos.x > 4.0,
        "it walks on toward the wagon"
    );
}

#[test]
fn a_destroyed_tower_leaves_a_tile_that_players_can_walk_through() {
    let mut s = field_with_tower();
    let a = s.player_at(5.0, 17.5);
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| sim.towers().count() == 0);
    s.clear_monsters();

    s.hold_move(a, -1.0, 0.0);
    s.run_ticks(s.ticks(3.0));

    assert!(
        s.player(a).pos.x < 1.0,
        "it walked across the tile where the tower stood, {:?}",
        s.player(a).pos
    );
}

#[test]
fn a_tower_can_be_built_again_where_one_was_destroyed() {
    let mut s = field_with_tower_tuned(|t| t.map.wagon = Rect::new(6.0, 17.0, 8.0, 19.0));
    let a = s.player_at(0.5, 16.5);
    s.set_tower_hp(10);
    s.monster_at(3.0, 17.5);
    s.run_until(100, |sim| sim.towers().count() == 0);
    s.clear_monsters();

    s.hold_build(a, 1.0, 1.0);
    s.run_ticks(s.ticks(3.0));

    let tower = s.sim.towers().next().expect("the new tower stands");
    assert_eq!(tower.tile, crate::world::Tile::new(1, 17));
}

#[test]
fn holding_f_next_to_a_damaged_tower_repairs_it_in_pulses() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    let pulse = s.ticks(s.sim.tuning().structures.watchtower.repair.pulse_secs);

    s.run_ticks(pulse - 1);
    assert_eq!(s.tower().hp(), 100, "no HP before the first pulse");

    s.run_ticks(1);
    assert_eq!(s.tower().hp(), 110, "10 HP per pulse");
    s.run_ticks(pulse);
    assert_eq!(s.tower().hp(), 120);
    assert!(s.player(a).interaction().is_some());
}

#[test]
fn a_repair_ends_at_full_hp_and_never_goes_above_it() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    s.set_tower_hp(295);

    s.run_ticks(s.ticks(2.0));

    assert_eq!(s.tower().hp(), 300);
    assert!(
        s.player(a).interaction().is_none(),
        "nothing left to repair"
    );
}

#[test]
fn releasing_f_stops_the_repair_and_keeps_the_hp_restored_so_far() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    s.run_ticks(25);
    assert_eq!(s.tower().hp(), 120);

    s.hold(a, PlayerInput::default());
    s.run_ticks(60);

    assert_eq!(
        s.tower().hp(),
        120,
        "the restored HP stays and nothing more arrives"
    );
}

#[test]
fn a_repairer_can_neither_move_nor_attack() {
    let (mut s, a) = repairer_at_a_damaged_tower();
    let monster = s.monster_at(3.4, 17.5);
    s.hold(
        a,
        PlayerInput {
            interact: true,
            attack: true,
            move_dir: crate::input::MoveDir::new(0.0, -1.0),
            aim: crate::input::AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let start = s.player(a).pos;

    s.run_ticks(20);

    assert_eq!(s.player(a).pos, start);
    assert_eq!(s.player(a).last_attack_tick, None);
    assert!(s.monster(monster).is_some_and(|m| m.hp > 0));
}

#[test]
fn the_repair_stops_when_the_repairer_goes_down() {
    let mut s = field_with_tower_tuned(|t| t.monsters.grunt.attack_damage = 100);
    let a = s.player_at(2.5, 17.5);
    s.player_at(15.0, 3.0);
    s.set_tower_hp(100);
    s.hold(a, press_f());
    s.run_ticks(5);
    assert!(s.player(a).interaction().is_some());

    s.monster_at(3.4, 17.5);
    s.run_ticks(2);
    let hp = s.tower().hp();
    s.run_ticks(60);

    assert!(s.player(a).is_downed());
    assert!(s.player(a).interaction().is_none());
    assert!(
        s.sim.towers().next().is_none_or(|tower| tower.hp() <= hp),
        "no more HP is restored once the repairer is down"
    );
}

#[test]
fn several_repairers_add_up() {
    let (mut s, _) = repairer_at_a_damaged_tower();
    let b = s.player_at(2.5, 17.0);
    s.hold(b, press_f());
    let pulse = s.ticks(s.sim.tuning().structures.watchtower.repair.pulse_secs);

    s.run_ticks(pulse);

    assert_eq!(s.tower().hp(), 100 + 2 * 10);
}

#[test]
fn a_repair_needs_no_resource_and_a_far_player_cannot_repair() {
    let mut s = field_with_tower();
    let near = s.player_at(2.5, 17.5);
    let far = s.player_at(6.0, 17.5);
    s.set_tower_hp(100);
    s.hold(far, press_f());

    s.run_ticks(40);
    assert_eq!(s.tower().hp(), 100, "out of the repair range");
    assert!(s.player(far).interaction().is_none());

    s.hold(near, press_f());
    s.run_ticks(10);
    assert_eq!(
        s.tower().hp(),
        110,
        "a player in range repairs with no cost"
    );
}

#[test]
fn a_destroyed_tower_cannot_be_repaired() {
    let mut s = field_with_tower();
    let a = s.player_at(1.5, 18.4);
    s.set_tower_hp(10);
    s.monster_at(1.5, 16.6);
    s.run_until(100, |sim| sim.towers().count() == 0);
    s.clear_monsters();

    s.hold(a, press_f());
    s.run_ticks(s.ticks(3.0));

    assert!(s.player(a).interaction().is_none());
}

#[test]
fn a_downed_ally_in_range_comes_before_a_damaged_tower() {
    let mut s = field_with_tower();
    let reviver = s.player_at(2.5, 17.5);
    let downed = s.player_at(3.5, 17.5);
    s.player_at(15.0, 3.0);
    s.set_tower_hp(100);
    s.knock_down(downed);
    s.hold(reviver, press_f());
    let revive_ticks = s.ticks(s.sim.tuning().revive.revive_secs);

    s.run_ticks(revive_ticks / 2);
    assert!(s.sim.revive_progress(downed).is_some(), "revive first");
    assert_eq!(s.tower().hp(), 100, "no repair while reviving");

    s.run_ticks(revive_ticks);
    assert!(s.player(downed).is_alive());
    assert!(s.tower().hp() > 100, "then the repair begins");
}

#[test]
fn a_repairer_switches_to_reviving_when_an_ally_goes_down_beside_them() {
    let mut s = field_with_tower();
    let reviver = s.player_at(2.5, 17.5);
    let ally = s.player_at(3.5, 17.5);
    s.player_at(15.0, 3.0);
    s.set_tower_hp(100);
    s.hold(reviver, press_f());
    s.run_ticks(5);
    assert!(s.sim.revive_progress(ally).is_none());

    s.knock_down(ally);
    s.run_ticks(2);

    assert!(s.sim.revive_progress(ally).is_some());
}

#[test]
fn a_new_expedition_removes_the_towers_and_refills_the_stash() {
    let mut s = field_with_tower();
    s.player_at(15.0, 3.0);
    s.set_stash(3);
    s.set_wagon_hp(0);

    s.run_until(400, |sim| sim.expedition_number() == 2);

    assert_eq!(s.sim.towers().count(), 0);
    assert_eq!(s.sim.stash(), s.sim.tuning().wagon.start_stash);
}

#[test]
fn a_new_expedition_frees_the_tiles_the_towers_stood_on() {
    let mut s = field_with_tower();
    let a = s.player_at(5.0, 17.5);
    s.set_wagon_hp(0);
    s.run_until(400, |sim| sim.expedition_number() == 2);
    // 원정이 시작되면 플레이어는 등장 지점으로 돌아갑니다. 다시 (5, 17.5)로 옮겨서 감시탑이 있던 자리를 지나 걷게 합니다.
    s.sim.world_mut().players.get_mut(&a).unwrap().pos = crate::math::Vec2::new(5.0, 17.5);

    s.hold_move(a, -1.0, 0.0);
    s.run_ticks(s.ticks(3.0));

    assert!(s.player(a).pos.x < 1.0);
}

// 원정이 시작될 때 이미 서 있는 감시탑

#[test]
fn start_towers_stand_at_full_hp_when_the_expedition_begins_and_again_after_a_new_one() {
    let sites = vec![Tile::new(14, 14), Tile::new(16, 14)];
    let mut s = Scenario::with_tuning(|t| t.village.start_towers = sites.clone());

    let standing: Vec<(Tile, u32)> = s
        .sim
        .towers()
        .map(|tower| (tower.tile, tower.hp()))
        .collect();
    assert_eq!(standing, vec![(sites[0], 300), (sites[1], 300)]);

    s.set_tower_hp(10);
    s.sim.set_phase(Phase::Over {
        outcome: Outcome::Defeat,
        ticks_left: 1,
    });
    s.run_ticks(1);

    assert_eq!(s.sim.expedition_number(), 2);
    let standing: Vec<(Tile, u32)> = s
        .sim
        .towers()
        .map(|tower| (tower.tile, tower.hp()))
        .collect();
    assert_eq!(standing, vec![(sites[0], 300), (sites[1], 300)]);
}

#[test]
fn start_towers_count_toward_the_limit() {
    let mut s = Scenario::with_tuning(|t| {
        t.village.start_towers = vec![Tile::new(14, 14)];
        t.village.tower_limit = 1;
    });
    let a = s.player_at(14.5, 12.5);

    s.hold_build(a, 1.0, 0.0);
    s.run_ticks(1);

    assert_eq!(
        s.player(a).build_refusal().map(|refusal| refusal.why),
        Some(CannotBuild::TowerLimit)
    );
}

#[test]
fn start_towers_take_their_tiles() {
    let mut s = Scenario::with_tuning(|t| t.village.start_towers = vec![Tile::new(14, 14)]);
    let a = s.player_at(13.5, 14.5);

    s.hold_build(a, 1.0, 0.0);
    s.run_ticks(1);

    assert_eq!(
        s.player(a).build_refusal().map(|refusal| refusal.why),
        Some(CannotBuild::TileTaken)
    );
}

#[test]
fn the_default_map_takes_a_full_set_of_spread_out_towers() {
    let mut t = Tuning::default();
    t.village.start_towers = t.spread_tower_sites(t.village.tower_limit);
    assert_eq!(t.village.start_towers.len(), t.village.tower_limit as usize);
    assert_eq!(
        t.validate(),
        Ok(()),
        "the towers leave every tile reachable"
    );

    let s = Scenario::default_game(|tuning| *tuning = t.clone());

    assert_eq!(s.sim.towers().count(), t.village.tower_limit as usize);
    assert!(s.sim.towers().all(|tower| tower.is_standing()));
    for tower in s.sim.towers() {
        let gap = s.sim.wagon().rect.distance_to(tower.tile.center());
        assert!(
            gap <= t.village.build_range,
            "{:?} is outside the village",
            tower.tile
        );
    }
}

#[test]
fn spread_out_sites_leave_a_free_tile_between_neighbours_and_keep_the_player_spawns_clear() {
    let t = Tuning::default();
    let sites = t.spread_tower_sites(t.village.tower_limit);
    let distinct: std::collections::BTreeSet<Tile> = sites.iter().copied().collect();
    assert_eq!(distinct.len(), sites.len(), "no tile twice");
    for (i, a) in sites.iter().enumerate() {
        for b in &sites[i + 1..] {
            let (dx, dy) = (a.x.abs_diff(b.x), a.y.abs_diff(b.y));
            assert!(dx >= 2 || dy >= 2, "{a:?} and {b:?} touch");
        }
    }
    for spawn in &t.map.player_spawns {
        for site in &sites {
            assert!(
                site.rect().distance_to(*spawn) >= t.player.radius,
                "{site:?} sits on a player spawn {spawn:?}"
            );
        }
    }
}

#[test]
fn asking_for_more_sites_than_the_village_has_gives_what_fits() {
    let t = Tuning::default();
    let all = t.spread_tower_sites(u32::MAX);
    assert!(all.len() > t.village.tower_limit as usize);
    assert!(
        all.len() < 10_000,
        "bounded by the village, got {}",
        all.len()
    );
}
