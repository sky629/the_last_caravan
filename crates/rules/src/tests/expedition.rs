//! 입장과 퇴장 (D24, D25).

use crate::ids::PlayerId;
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
