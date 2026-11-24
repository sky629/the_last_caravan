//! 원정의 시작과 끝(입장과 퇴장, 성공과 실패, 새 원정)과 결정성을 확인합니다. 낮과 밤의 흐름은 `days.rs`가 확인합니다.

use super::support::Scenario;
use crate::expedition::{Outcome, Phase};
use crate::ids::PlayerId;
use crate::math::Vec2;
use crate::sim::{JoinError, Sim};
use crate::tuning::Tuning;

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
    let mut s = Scenario::with_cycle(|t| t.cycle.day_secs = 1.0);
    s.run_until(400, |sim| sim.monsters().count() > 0);
    assert!(
        matches!(s.sim.phase(), Phase::Night { .. }),
        "the expedition is in progress"
    );

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
    s.run_ticks(s.ticks(s.sim.tuning().cycle.restart_secs) + 1);

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
    s.run_ticks(s.ticks(s.sim.tuning().cycle.restart_secs) + 1);

    assert_eq!(s.sim.expedition_number(), 2);
    let mut positions: Vec<_> = staying.iter().map(|&id| s.player(id).pos).collect();
    positions.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(positions, spawns);
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
    assert!(matches!(s.sim.phase(), Phase::Day { .. }));

    s.knock_down(a);
    s.run_ticks(1);
    assert!(
        matches!(s.sim.phase(), Phase::Day { .. }),
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

    assert!(matches!(s.sim.phase(), Phase::Day { .. }));

    let a = s.player_at(2.0, 2.0);
    s.sim.remove_player(a);
    s.run_ticks(100);
    assert!(matches!(s.sim.phase(), Phase::Day { .. }));
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

    let restart = s.ticks(s.sim.tuning().cycle.restart_secs);
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
    let mut s = Scenario::with_cycle(|t| t.cycle.day_secs = 1.0);
    s.run_until(400, |sim| sim.monsters().count() > 0);
    let old_ids: Vec<_> = s.sim.monsters().map(|m| m.id).collect();
    s.sim.world_mut().wagon.hp = 0;
    s.player_at(2.0, 2.0);
    s.run_ticks(s.ticks(s.sim.tuning().cycle.restart_secs) + 2);
    assert_eq!(s.sim.expedition_number(), 2);

    s.run_until(400, |sim| sim.monsters().count() > 0);

    let new_ids: Vec<_> = s.sim.monsters().map(|m| m.id).collect();
    assert!(new_ids.iter().all(|id| !old_ids.contains(id)));
}

/// 두 플레이어가 움직이고, 달리고, 싸우고, 부활과 수리와 건설을 하는 원정을 800 Tick 진행하고, 50 Tick마다의 몬스터와 마지막 상태를 글자로 돌려줍니다.
fn play_a_scripted_expedition() -> String {
    let mut s = Scenario::default_game(|t| t.cycle.day_secs = 3.0);
    let a = s.sim.add_player().unwrap();
    let b = s.sim.add_player().unwrap();
    let mut trace = String::new();
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
                    sprint: phase % 3 != 1,
                },
            );
        }
        s.run_ticks(1);
        if tick % 50 == 0 {
            let monsters: Vec<_> = s.sim.monsters().collect();
            trace += &format!("{monsters:?} ");
        }
    }
    let players: Vec<_> = s.sim.players().collect();
    let monsters: Vec<_> = s.sim.monsters().collect();
    let towers: Vec<_> = s.sim.towers().collect();
    format!(
        "{trace}{players:?} {monsters:?} {towers:?} {:?} {:?}",
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
