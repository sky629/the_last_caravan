//! 원정의 진행(무리, 성공과 실패, 새 원정, 입장과 퇴장)을 확인합니다.

use super::support::Scenario;
use crate::expedition::{Outcome, Phase};
use crate::ids::PlayerId;
use crate::math::Vec2;
use crate::sim::{JoinError, Sim};
use crate::tuning::Tuning;

/// 등장하지 않은 몬스터 없이 무리가 모두 나타난 상태가 될 때까지 진행합니다.
fn run_until_fully_spawned(s: &mut Scenario) {
    s.run_until(1000, |sim| {
        matches!(sim.phase(), Phase::Fighting { to_spawn: 0, .. })
    });
}

/// 한 무리가 모두 나타난 뒤 그 몬스터를 모두 처치한 상황을 만들고, 한 Tick 진행합니다.
fn defeat_the_current_wave(s: &mut Scenario) {
    run_until_fully_spawned(s);
    s.clear_monsters();
    s.run_ticks(1);
}

fn monster_count(s: &Scenario) -> usize {
    s.sim.monsters().count()
}

#[test]
fn players_get_increasing_ids_until_the_room_is_full() {
    let mut sim = Sim::new(Tuning {
        max_players: 2,
        ..Tuning::default()
    });

    assert_eq!(sim.add_player(), Ok(PlayerId(1)));
    assert_eq!(sim.add_player(), Ok(PlayerId(2)));
    assert_eq!(sim.add_player(), Err(JoinError::RoomFull { capacity: 2 }));
}

#[test]
fn a_removed_player_frees_a_seat_but_its_id_is_not_reused() {
    let mut sim = Sim::new(Tuning {
        max_players: 2,
        ..Tuning::default()
    });
    let first = sim.add_player().unwrap();
    sim.add_player().unwrap();

    sim.remove_player(first);
    sim.remove_player(first);

    assert_eq!(sim.add_player(), Ok(PlayerId(3)));
    let ids: Vec<_> = sim.players().map(|p| p.id).collect();
    assert_eq!(ids, [PlayerId(2), PlayerId(3)]);
}

#[test]
fn a_default_room_takes_four_players_and_refuses_the_fifth() {
    let mut sim = Sim::new(Tuning::default());

    for _ in 0..4 {
        assert!(sim.add_player().is_ok());
    }

    assert_eq!(sim.add_player(), Err(JoinError::RoomFull { capacity: 4 }));
}

#[test]
fn the_capacity_is_not_fixed_at_four() {
    let mut sim = Sim::new(Tuning {
        max_players: 6,
        ..Tuning::default()
    });
    for _ in 0..6 {
        assert!(sim.add_player().is_ok());
    }
    assert!(sim.add_player().is_err());
}

#[test]
fn leaving_removes_the_player_at_once() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);

    s.sim.remove_player(a);

    assert!(s.sim.player(a).is_none());
    assert_eq!(s.sim.player_count(), 0);
}

#[test]
fn a_player_who_joins_in_the_middle_of_an_expedition_is_alive_at_a_spawn_point() {
    let mut s = Scenario::with_waves(|_| {});
    s.run_ticks(s.ticks(5.0));
    assert!(monster_count(&s) > 0, "the expedition is in progress");

    let late = s.sim.add_player().unwrap();

    let spawn = s.sim.tuning().map.player_spawns[0];
    assert!(s.player(late).is_alive());
    assert_eq!(s.player(late).hp(), 100);
    assert_eq!(s.player(late).pos, spawn);
}

#[test]
fn a_player_who_joins_after_someone_left_takes_the_spawn_point_nobody_in_the_room_uses() {
    let spawns = [
        Vec2::new(10.0, 10.0),
        Vec2::new(11.0, 10.0),
        Vec2::new(12.0, 10.0),
        Vec2::new(13.0, 10.0),
    ];
    let mut s = Scenario::with_tuning(|t| t.map.player_spawns = spawns.to_vec());
    let first = s.sim.add_player().unwrap();
    let second = s.sim.add_player().unwrap();
    let third = s.sim.add_player().unwrap();

    s.sim.remove_player(first);
    let newcomer = s.sim.add_player().unwrap();
    assert_eq!(
        s.player(newcomer).pos,
        spawns[0],
        "the free point is reused"
    );
    assert_eq!(s.player(second).pos, spawns[1]);
    assert_eq!(s.player(third).pos, spawns[2]);

    s.sim.remove_player(second);
    s.sim.remove_player(third);
    let next = s.sim.add_player().unwrap();
    assert_eq!(
        s.player(next).pos,
        spawns[1],
        "the lowest free point comes first"
    );
}

#[test]
fn a_new_expedition_returns_each_player_to_their_own_spawn_point_after_someone_left() {
    let spawns = [
        Vec2::new(10.0, 10.0),
        Vec2::new(11.0, 10.0),
        Vec2::new(12.0, 10.0),
    ];
    let mut s = Scenario::with_tuning(|t| t.map.player_spawns = spawns.to_vec());
    let first = s.player_at(3.0, 3.0);
    let second = s.player_at(4.0, 4.0);
    let third = s.player_at(5.0, 5.0);
    s.sim.remove_player(first);

    s.set_wagon_hp(0);
    s.run_ticks(1);
    s.run_ticks(s.ticks(s.sim.tuning().waves.restart_secs) + 1);

    assert_eq!(s.sim.expedition_number(), 2);
    assert_eq!(s.player(second).pos, spawns[1]);
    assert_eq!(s.player(third).pos, spawns[2]);
}

/// 등장 지점 네 곳에 플레이어 다섯 명이 들어갔다가 한 명이 나간 방입니다. 남은 네 명 가운데 둘이 같은 지점에 있습니다.
fn five_players_on_four_points_after_one_left() -> (Scenario, [Vec2; 4], Vec<PlayerId>) {
    let spawns = [
        Vec2::new(10.0, 10.0),
        Vec2::new(11.0, 10.0),
        Vec2::new(12.0, 10.0),
        Vec2::new(13.0, 10.0),
    ];
    let mut s = Scenario::with_tuning(|t| {
        t.max_players = 6;
        t.map.player_spawns = spawns.to_vec();
    });
    let ids: Vec<_> = (0..5).map(|_| s.sim.add_player().unwrap()).collect();
    s.sim.remove_player(ids[2]);
    let staying = [0, 1, 3, 4].iter().map(|&i| ids[i]).collect();
    (s, spawns, staying)
}

#[test]
fn players_beyond_the_spawn_points_are_spread_evenly_when_they_join() {
    let (mut s, spawns, staying) = five_players_on_four_points_after_one_left();

    let newcomer = s.sim.add_player().unwrap();

    assert_eq!(
        s.player(newcomer).pos,
        spawns[2],
        "the point nobody uses comes first"
    );
    let sixth = s.sim.add_player().unwrap();
    let mut counts = [0; 4];
    for id in staying.into_iter().chain([newcomer, sixth]) {
        let point = spawns.iter().position(|p| *p == s.player(id).pos).unwrap();
        counts[point] += 1;
    }
    assert_eq!(counts.iter().max(), Some(&2));
    assert_eq!(counts.iter().sum::<i32>(), 6);
}

#[test]
fn a_new_expedition_does_not_stack_two_players_on_one_point_while_another_is_free() {
    let (mut s, spawns, staying) = five_players_on_four_points_after_one_left();

    s.set_wagon_hp(0);
    s.run_ticks(1);
    s.run_ticks(s.ticks(s.sim.tuning().waves.restart_secs) + 1);

    assert_eq!(s.sim.expedition_number(), 2);
    let mut positions: Vec<_> = staying.iter().map(|&id| s.player(id).pos).collect();
    positions.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(positions, spawns);
}

#[test]
fn the_first_wave_appears_after_the_delay_one_monster_at_a_time() {
    let mut s = Scenario::with_waves(|_| {});
    let delay = s.ticks(s.sim.tuning().waves.first_delay_secs);
    let interval = s.ticks(s.sim.tuning().waves.spawn_interval_secs);

    s.run_ticks(delay - 1);
    assert_eq!(monster_count(&s), 0, "nothing before the delay");

    s.run_ticks(1);
    assert_eq!(monster_count(&s), 1, "the stream starts with one monster");
    s.run_ticks(interval - 1);
    assert_eq!(monster_count(&s), 1);
    s.run_ticks(1);
    assert_eq!(monster_count(&s), 2);

    s.run_ticks(interval * 10);
    assert_eq!(monster_count(&s), 6, "the first wave has six monsters");
}

#[test]
fn monsters_appear_at_the_map_edge_and_at_different_spots() {
    let mut s = Scenario::with_waves(|_| {});
    let lanes = s.sim.tuning().waves.spawn_lanes as usize;
    let mut spawn_points: Vec<Vec2> = Vec::new();
    while spawn_points.len() < lanes {
        s.run_until(100, |sim| sim.monsters().count() > spawn_points.len());
        let newest = s.sim.monsters().max_by_key(|m| m.id).unwrap();
        spawn_points.push(newest.pos);
    }

    assert!(
        spawn_points.iter().all(|p| p.y < 1.0),
        "south edge, {spawn_points:?}"
    );
    for (i, a) in spawn_points.iter().enumerate() {
        for b in &spawn_points[i + 1..] {
            assert!(
                a.distance(*b) > 0.3,
                "two monsters appeared on the same spot"
            );
        }
    }
}

#[test]
fn a_wave_is_not_cleared_while_monsters_are_still_to_come() {
    let mut s = Scenario::with_waves(|_| {});
    s.run_until(200, |sim| sim.monsters().count() == 1);

    s.clear_monsters();
    s.run_ticks(1);

    assert!(matches!(s.sim.phase(), Phase::Fighting { wave: 1, .. }));
}

#[test]
fn the_next_larger_wave_comes_after_a_pause_once_the_wave_is_cleared() {
    let mut s = Scenario::with_waves(|_| {});
    defeat_the_current_wave(&mut s);

    let pause = s.ticks(s.sim.tuning().waves.between_secs);
    assert!(matches!(
        s.sim.phase(),
        Phase::Intermission { next_wave: 2, .. }
    ));
    s.run_ticks(pause - 2);
    assert_eq!(monster_count(&s), 0, "still waiting");

    s.run_ticks(2);
    assert!(matches!(s.sim.phase(), Phase::Fighting { wave: 2, .. }));
    run_until_fully_spawned(&mut s);
    assert_eq!(monster_count(&s), 10, "the second wave is larger");
}

#[test]
fn the_wave_sizes_come_from_the_tuning() {
    let mut s = Scenario::with_waves(|t| t.waves.sizes = vec![3, 5]);
    run_until_fully_spawned(&mut s);
    assert_eq!(monster_count(&s), 3);

    s.clear_monsters();
    s.run_until(1000, |sim| {
        matches!(
            sim.phase(),
            Phase::Fighting {
                wave: 2,
                to_spawn: 0,
                ..
            }
        )
    });
    assert_eq!(monster_count(&s), 5);
}

#[test]
fn clearing_every_wave_is_a_victory_and_a_new_expedition_follows() {
    let mut s = Scenario::with_waves(|_| {});
    s.player_at(2.0, 2.0);
    for _ in 0..3 {
        defeat_the_current_wave(&mut s);
    }

    let restart = s.ticks(s.sim.tuning().waves.restart_secs);
    assert!(
        matches!(
            s.sim.phase(),
            Phase::Over {
                outcome: Outcome::Victory,
                ..
            }
        ),
        "got {:?}",
        s.sim.phase()
    );

    s.run_ticks(restart - 2);
    assert_eq!(
        s.sim.expedition_number(),
        1,
        "the result is still on the screen"
    );
    s.run_ticks(2);
    assert_eq!(s.sim.expedition_number(), 2);
    assert!(matches!(
        s.sim.phase(),
        Phase::Intermission { next_wave: 1, .. }
    ));
}

#[test]
fn a_destroyed_wagon_fails_the_expedition() {
    let mut s = Scenario::open_field();
    s.player_at(2.0, 2.0);
    s.set_wagon_hp(10);
    s.monster_at(16.3, 18.0);

    s.run_ticks(1);

    assert_eq!(s.sim.wagon().hp, 0);
    assert!(matches!(
        s.sim.phase(),
        Phase::Over {
            outcome: Outcome::Defeat,
            ..
        }
    ));
}

#[test]
fn the_expedition_fails_when_every_player_is_downed_or_dead() {
    let mut s = Scenario::open_field();
    let a = s.player_at(2.0, 2.0);
    let b = s.player_at(3.0, 2.0);
    s.run_ticks(1);
    assert!(matches!(s.sim.phase(), Phase::Intermission { .. }));

    s.knock_down(a);
    s.run_ticks(1);
    assert!(
        matches!(s.sim.phase(), Phase::Intermission { .. }),
        "one is still standing"
    );

    s.kill(b);
    s.run_ticks(1);
    assert!(
        matches!(
            s.sim.phase(),
            Phase::Over {
                outcome: Outcome::Defeat,
                ..
            }
        ),
        "one downed and one dead"
    );
}

#[test]
fn an_empty_room_never_fails() {
    let mut s = Scenario::open_field();
    s.set_wagon_hp(0);

    s.run_ticks(100);

    assert!(matches!(s.sim.phase(), Phase::Intermission { .. }));

    let a = s.player_at(2.0, 2.0);
    s.sim.remove_player(a);
    s.run_ticks(100);
    assert!(matches!(s.sim.phase(), Phase::Intermission { .. }));
}

/// 마지막 몬스터를 플레이어가 처치하는 Tick에, 그 몬스터가 마차에 마지막 피해를 줍니다.
fn last_monster_dies_while_hitting_the_wagon(wagon_hp: u32) -> Scenario {
    let mut s = Scenario::with_waves(|t| {
        t.waves.sizes = vec![1];
        t.waves.first_delay_secs = 0.05;
        t.waves.spawn_lanes = 1;
        t.map.monster_spawns = vec![Vec2::new(16.3, 18.0)];
        t.monsters.grunt.max_hp = 25;
        t.monsters.grunt.aggro_range = 0.0;
    });
    let a = s.player_at(15.5, 18.0);
    s.hold(
        a,
        crate::input::PlayerInput {
            aim: crate::input::AimDir::new(1.0, 0.0),
            attack: true,
            ..Default::default()
        },
    );
    s.set_wagon_hp(wagon_hp);
    s.run_ticks(1);
    s
}

#[test]
fn clearing_the_last_wave_in_the_same_tick_the_wagon_falls_is_a_defeat() {
    let wagon_survives = last_monster_dies_while_hitting_the_wagon(500);
    assert!(
        matches!(
            wagon_survives.sim.phase(),
            Phase::Over {
                outcome: Outcome::Victory,
                ..
            }
        ),
        "with the wagon standing the same tick is a victory"
    );

    let wagon_falls = last_monster_dies_while_hitting_the_wagon(10);
    assert_eq!(wagon_falls.sim.monsters().count(), 0);
    assert!(matches!(
        wagon_falls.sim.phase(),
        Phase::Over {
            outcome: Outcome::Defeat,
            ..
        }
    ));
}

#[test]
fn the_world_stands_still_while_the_result_is_shown() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    let monster = s.monster_at(10.0, 3.0);
    s.set_wagon_hp(0);
    s.run_ticks(1);
    assert!(matches!(s.sim.phase(), Phase::Over { .. }));
    let player_before = s.player(a).pos;
    let monster_before = s.monster(monster).unwrap().pos;

    s.hold_move(a, 1.0, 0.0);
    s.run_ticks(20);

    assert_eq!(s.player(a).pos, player_before);
    assert_eq!(s.monster(monster).unwrap().pos, monster_before);
}

#[test]
fn a_new_expedition_restores_the_players_the_wagon_and_the_field() {
    let mut s = Scenario::with_tuning(|t| {
        t.map.player_spawns = vec![Vec2::new(10.0, 10.0), Vec2::new(11.0, 10.0)];
    });
    let dead = s.player_at(3.0, 3.0);
    let downed = s.player_at(4.0, 4.0);
    s.kill(dead);
    s.knock_down(downed);
    s.set_wagon_hp(7);
    s.monster_at(8.0, 8.0);
    let old_monster_count = s.sim.monsters().count();
    assert_eq!(old_monster_count, 1);

    let restart = s.ticks(s.sim.tuning().waves.restart_secs);
    s.run_ticks(restart + 1);

    assert_eq!(s.sim.expedition_number(), 2);
    assert_eq!(s.sim.wagon().hp, 500);
    assert_eq!(s.sim.monsters().count(), 0);
    for (id, spawn) in [
        (dead, Vec2::new(10.0, 10.0)),
        (downed, Vec2::new(11.0, 10.0)),
    ] {
        assert!(s.player(id).is_alive(), "the dead state is cleared too");
        assert_eq!(s.player(id).hp(), 100);
        assert_eq!(s.player(id).pos, spawn);
    }
}

#[test]
fn monster_ids_are_not_reused_in_a_new_expedition() {
    let mut s = Scenario::with_waves(|_| {});
    s.run_until(200, |sim| sim.monsters().count() > 0);
    let old_ids: Vec<_> = s.sim.monsters().map(|m| m.id).collect();
    s.sim.world_mut().wagon.hp = 0;
    s.player_at(2.0, 2.0);
    s.run_ticks(s.ticks(s.sim.tuning().waves.restart_secs) + 2);
    assert_eq!(s.sim.expedition_number(), 2);

    s.run_until(200, |sim| sim.monsters().count() > 0);

    let new_ids: Vec<_> = s.sim.monsters().map(|m| m.id).collect();
    assert!(new_ids.iter().all(|id| !old_ids.contains(id)));
}

#[test]
fn the_default_game_has_three_waves_and_each_one_is_larger() {
    let waves = Tuning::default().waves.sizes;

    assert_eq!(waves.len(), 3);
    assert!(waves.windows(2).all(|pair| pair[0] < pair[1]));
}

/// 두 플레이어가 움직이고, 싸우고, 부활과 수리와 건설을 하는 원정을 800 Tick 진행하고 모든 상태를 글자로 돌려줍니다.
fn play_a_scripted_expedition() -> String {
    let mut s = Scenario::with_waves(|t| t.map = crate::tuning::MapSpec::stage1());
    let a = s.sim.add_player().unwrap();
    let b = s.sim.add_player().unwrap();
    for tick in 0..800 {
        let phase = tick / 100;
        for (id, offset) in [(a, 0.0), (b, 1.7)] {
            s.hold(
                id,
                crate::input::PlayerInput {
                    seq: tick,
                    move_dir: crate::input::MoveDir::new(
                        (phase as f32 + offset).sin(),
                        (phase as f32).cos(),
                    ),
                    aim: crate::input::AimDir::new((tick as f32 * 0.1 + offset).cos(), 0.5),
                    attack: tick % 3 != 0,
                    interact: phase % 2 == 1,
                    build: phase % 4 == 3,
                },
            );
        }
        s.run_ticks(1);
    }
    let players: Vec<_> = s.sim.players().collect();
    let monsters: Vec<_> = s.sim.monsters().collect();
    let towers: Vec<_> = s.sim.towers().collect();
    format!(
        "{players:?} {monsters:?} {towers:?} {:?} {:?}",
        s.sim.wagon(),
        s.sim.phase()
    )
}

#[test]
fn the_same_inputs_always_produce_the_same_game() {
    let first = play_a_scripted_expedition();
    let second = play_a_scripted_expedition();

    assert_eq!(first, second);
    assert!(first.contains("Monster"), "the script reached a fight");
}
