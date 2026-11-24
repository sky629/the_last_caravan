//! 자원을 확인합니다: 채집, 든 자원의 상한, 마차에 넣기, 쓰러지거나 나갈 때 떨어뜨리기, 줍기, 낮마다 다시 채우기, 자원의 총량.
//! 테스트는 `&Sim`이 공개하는 값(채집 지점, 떨어진 자원, 플레이어가 든 자원, 보관함)만 확인합니다.
//!
//! 시나리오는 20 x 20 맵이고 마차는 (17, 17)에서 (19, 19)까지입니다. 채집 지점은 마을의 범위(10타일) 밖인 타일 (2, 2)에 있습니다.

use super::support::{Scenario, gathered_total, nights, resource_total};
use crate::expedition::{NightStep, Phase};
use crate::ids::{NodeId, PlayerId};
use crate::input::PlayerInput;
use crate::player::Interaction;
use crate::world::Tile;

const NODE: NodeId = NodeId(1);

/// 채집 지점 (2, 2)가 하나 있는 시나리오입니다.
fn field_with_a_node() -> Scenario {
    Scenario::with_tuning(|t| t.map.nodes = vec![Tile::new(2, 2)])
}

/// 채집 지점 (2, 2)의 동쪽 곁에서 F를 누르고 있는 플레이어입니다.
fn gatherer(s: &mut Scenario) -> PlayerId {
    let id = s.player_at(3.5, 2.5);
    s.hold_interact(id);
    id
}

/// 채집이 한 번 이루어지는 데 걸리는 Tick 수입니다.
fn pulse_ticks(s: &Scenario) -> u32 {
    s.ticks(s.sim.tuning().resources.gather_pulse_secs)
}

fn node_amount(s: &Scenario) -> u32 {
    s.sim.nodes().next().expect("the map has a node").amount
}

#[test]
fn holding_f_beside_a_node_moves_one_unit_from_the_node_to_the_player_each_pulse() {
    let mut s = field_with_a_node();
    let a = gatherer(&mut s);
    let pulse = pulse_ticks(&s);
    let full = s.sim.tuning().resources.node_amount;

    s.run_ticks(pulse - 1);
    assert_eq!(
        s.player(a).carried(),
        0,
        "one tick short of the first pulse"
    );
    assert_eq!(node_amount(&s), full);

    s.run_ticks(1);
    assert_eq!(s.player(a).carried(), 1);
    assert_eq!(
        node_amount(&s),
        full - 1,
        "what the player holds left the node"
    );

    s.run_ticks(pulse * 2);
    assert_eq!(s.player(a).carried(), 3);
    assert_eq!(node_amount(&s), full - 3);
}

#[test]
fn a_gatherer_can_neither_move_nor_attack() {
    let mut s = field_with_a_node();
    let a = s.player_at(3.5, 2.5);
    s.hold(
        a,
        PlayerInput {
            interact: true,
            attack: true,
            move_dir: crate::input::MoveDir::new(1.0, 0.0),
            aim: crate::input::AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    );
    let monster = s.monster_at(4.5, 2.5);
    let start = s.player(a).pos;

    s.run_ticks(pulse_ticks(&s) * 2);

    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Gathering { .. })
    ));
    assert_eq!(s.player(a).pos, start);
    assert_eq!(s.player(a).last_attack.map(|attack| attack.tick), None);
    assert!(s.monster(monster).is_some_and(|m| m.hp == 60));
}

#[test]
fn what_a_player_carries_never_passes_the_capacity_and_gathering_stops_when_full() {
    let mut s = field_with_a_node();
    let a = gatherer(&mut s);
    let capacity = s.sim.tuning().resources.carry_capacity;
    let full = s.sim.tuning().resources.node_amount;

    s.run_ticks(pulse_ticks(&s) * (capacity + 5));

    assert_eq!(s.player(a).carried(), capacity);
    assert_eq!(node_amount(&s), full - capacity, "no more left the node");
    assert!(
        s.player(a).interaction().is_none(),
        "the player is free to walk away"
    );
}

#[test]
fn a_pulse_that_is_bigger_than_the_room_left_moves_only_what_fits() {
    let mut s = Scenario::with_tuning(|t| {
        t.map.nodes = vec![Tile::new(2, 2)];
        t.resources.gather_per_pulse = 4;
    });
    let a = gatherer(&mut s);
    let full = s.sim.tuning().resources.node_amount;

    s.run_ticks(pulse_ticks(&s) * 4);

    assert_eq!(s.player(a).carried(), 10, "4, then 4, then the 2 that fit");
    assert_eq!(node_amount(&s), full - 10);
}

#[test]
fn an_empty_node_gives_nothing_and_does_not_hold_the_player() {
    let mut s = field_with_a_node();
    let a = gatherer(&mut s);
    s.set_node_amount(NODE, 0);

    s.run_ticks(pulse_ticks(&s) * 2);

    assert_eq!(s.player(a).carried(), 0);
    assert!(s.player(a).interaction().is_none());
}

#[test]
fn the_last_unit_of_a_node_goes_to_one_player_only() {
    let mut s = field_with_a_node();
    let first = gatherer(&mut s);
    let second = s.player_at(2.5, 3.5);
    s.hold_interact(second);
    s.set_node_amount(NODE, 1);

    s.run_ticks(pulse_ticks(&s));

    assert_eq!(node_amount(&s), 0);
    assert_eq!(s.player(first).carried() + s.player(second).carried(), 1);
}

#[test]
fn a_node_out_of_reach_cannot_be_gathered() {
    let mut s = field_with_a_node();
    let a = s.player_at(6.5, 2.5);
    s.hold_interact(a);

    s.run_ticks(pulse_ticks(&s) * 2);

    assert_eq!(s.player(a).carried(), 0);
    assert!(s.player(a).interaction().is_none());
}

#[test]
fn a_node_blocks_walkers() {
    let mut s = field_with_a_node();
    let walker = s.player_at(3.5, 2.5);
    s.hold_move(walker, -1.0, 0.0);

    s.run_ticks(s.ticks(2.0));

    let radius = s.sim.tuning().player.radius;
    assert!(
        s.player(walker).pos.x >= 3.0 + radius - 1e-3,
        "the walker stopped at the node's east edge, got {:?}",
        s.player(walker).pos
    );
}

#[test]
fn a_downed_ally_is_revived_before_a_node_is_gathered() {
    let mut s = field_with_a_node();
    let a = gatherer(&mut s);
    let downed = s.player_at(3.5, 3.5);
    s.knock_down(downed);

    s.run_ticks(5);

    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Reviving { .. })
    ));
    assert_eq!(s.player(a).carried(), 0);
}

#[test]
fn a_damaged_tower_is_repaired_before_a_node_is_gathered() {
    let mut s = field_with_a_node();
    s.tower_at(4, 2);
    s.set_tower_hp(100);
    let a = gatherer(&mut s);

    s.run_ticks(5);

    assert!(matches!(
        s.player(a).interaction(),
        Some(Interaction::Repairing { .. })
    ));
}

// 마차에 넣기

/// 마차의 서쪽 곁입니다. 마차의 가장자리까지 0.15타일이라 든 자원을 넣을 수 있는 거리 안입니다.
const BESIDE_THE_WAGON: (f32, f32) = (16.5, 18.0);

#[test]
fn a_carrier_beside_the_wagon_puts_everything_in_the_stash() {
    let mut s = Scenario::open_field();
    let a = s.player_at(BESIDE_THE_WAGON.0, BESIDE_THE_WAGON.1);
    s.set_carried(a, 7);
    let stash = s.sim.stash();

    s.run_ticks(1);

    assert_eq!(s.sim.stash(), stash + 7);
    assert_eq!(s.player(a).carried(), 0);
}

#[test]
fn a_carrier_outside_the_deposit_reach_keeps_the_resource_until_it_gets_closer() {
    let mut s = Scenario::open_field();
    let a = s.player_at(15.0, 18.0);
    s.set_carried(a, 7);
    let stash = s.sim.stash();

    s.run_ticks(3);
    assert_eq!(
        s.player(a).carried(),
        7,
        "1.65 tiles from the wagon is too far"
    );
    assert_eq!(s.sim.stash(), stash);

    s.hold_move(a, 1.0, 0.0);
    s.run_ticks(s.ticks(0.3));
    assert_eq!(s.player(a).carried(), 0, "it walked into the reach");
    assert_eq!(s.sim.stash(), stash + 7);
}

#[test]
fn what_was_put_in_the_stash_pays_for_a_tower() {
    let mut s = Scenario::open_field();
    s.set_stash(0);
    let carrier = s.player_at(BESIDE_THE_WAGON.0, BESIDE_THE_WAGON.1);
    s.set_carried(carrier, 10);
    let builder = s.player_at(14.5, 14.5);
    s.hold_build(builder, 1.0, 0.0);

    s.run_ticks(1);
    assert_eq!(s.sim.towers().count(), 0, "the stash was still empty");

    s.run_ticks(1);
    assert_eq!(s.sim.towers().count(), 1, "the deposit paid for the site");
    assert_eq!(s.sim.stash(), 0);
}

// 떨어뜨리기와 줍기

/// 한 번 맞으면 쓰러지는 몬스터가 있는 시나리오입니다.
fn deadly_field() -> Scenario {
    Scenario::with_tuning(|t| t.monsters.grunt.attack_damage = 1000)
}

#[test]
fn a_player_who_goes_down_drops_what_they_carried_where_they_fell() {
    let mut s = deadly_field();
    let a = s.player_at(5.0, 5.0);
    s.player_at(1.0, 1.0);
    s.set_carried(a, 6);
    s.monster_at(5.6, 5.0);

    s.run_until(100, |sim| sim.player(a).is_some_and(|p| p.is_downed()));

    assert_eq!(s.player(a).carried(), 0);
    let pile = s.sim.piles().next().expect("a pile was left");
    assert_eq!(pile.amount, 6);
    assert_eq!(pile.pos, s.player(a).pos);
}

#[test]
fn a_player_who_carries_nothing_drops_no_pile() {
    let mut s = deadly_field();
    let a = s.player_at(5.0, 5.0);
    s.player_at(1.0, 1.0);
    s.monster_at(5.6, 5.0);

    s.run_until(100, |sim| sim.player(a).is_some_and(|p| p.is_downed()));

    assert_eq!(s.sim.piles().count(), 0);
}

#[test]
fn a_player_who_leaves_the_room_drops_what_they_carried() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    s.set_carried(a, 4);

    s.sim.remove_player(a);

    let pile = s.sim.piles().next().expect("a pile was left");
    assert_eq!((pile.amount, pile.pos.x, pile.pos.y), (4, 5.0, 5.0));
}

#[test]
fn walking_over_a_pile_picks_up_what_fits_and_leaves_the_rest() {
    let mut s = Scenario::open_field();
    s.pile_at(5.0, 5.0, 15);
    let a = s.player_at(5.5, 5.0);
    s.set_carried(a, 4);

    s.run_ticks(1);

    assert_eq!(s.player(a).carried(), 10, "up to the capacity");
    assert_eq!(s.sim.piles().next().expect("the rest stays").amount, 9);
}

#[test]
fn a_pile_is_picked_up_only_from_inside_the_pickup_reach() {
    let mut s = Scenario::open_field();
    s.pile_at(5.0, 5.0, 3);
    let a = s.player_at(7.0, 5.0);

    s.run_ticks(3);
    assert_eq!(
        s.player(a).carried(),
        0,
        "1.65 tiles from the pile is too far"
    );

    s.hold_move(a, -1.0, 0.0);
    s.run_ticks(s.ticks(0.5));
    assert_eq!(s.player(a).carried(), 3);
    assert_eq!(
        s.sim.piles().count(),
        0,
        "a pile that is picked clean is gone"
    );
}

#[test]
fn a_downed_player_does_not_pick_up_a_pile() {
    let mut s = Scenario::open_field();
    s.pile_at(5.0, 5.0, 3);
    let a = s.player_at(5.0, 5.0);
    s.player_at(1.0, 1.0);
    s.knock_down(a);

    s.run_ticks(3);

    assert_eq!(s.sim.piles().next().expect("untouched").amount, 3);
}

#[test]
fn a_player_whose_hands_are_full_leaves_the_pile_alone() {
    let mut s = Scenario::open_field();
    s.pile_at(5.0, 5.0, 3);
    let a = s.player_at(5.0, 5.0);
    s.set_carried(a, 10);

    s.run_ticks(3);

    assert_eq!(s.player(a).carried(), 10);
    assert_eq!(s.sim.piles().next().expect("untouched").amount, 3);
}

#[test]
fn of_two_players_on_one_pile_the_lower_id_takes_it_first() {
    let mut s = Scenario::open_field();
    s.pile_at(5.0, 5.0, 12);
    let first = s.player_at(5.2, 5.0);
    let second = s.player_at(4.8, 5.0);

    s.run_ticks(1);

    assert_eq!(s.player(first).carried(), 10);
    assert_eq!(s.player(second).carried(), 2);
}

#[test]
fn a_revived_player_comes_back_with_empty_hands() {
    let mut s = Scenario::open_field();
    let a = s.player_at(5.0, 5.0);
    let b = s.player_at(5.8, 5.0);
    s.set_carried(a, 5);
    s.knock_down(a);
    s.hold_interact(b);

    s.run_ticks(s.ticks(s.sim.tuning().revive.revive_secs));

    assert!(s.player(a).is_alive());
    assert_eq!(
        s.player(a).carried(),
        0,
        "the resource was lost when they fell"
    );
}

// 낮마다 다시 채우기와 이어지는 것

/// 낮이 짧고 밤마다 몬스터가 하나인 두 날짜리 원정에, 채집 지점 (2, 2)가 있는 시나리오입니다.
fn two_short_days() -> Scenario {
    let plans = nights(&[&[1], &[1]]);
    Scenario::with_cycle(|t| {
        t.cycle.day_secs = 0.5;
        t.nights.plans = plans;
        t.map.nodes = vec![Tile::new(2, 2)];
    })
}

/// 밤의 무리를 모두 처치한 상황을 만들고 다음 날의 낮이 시작될 때까지 진행합니다.
fn clear_the_night_and_wait_for_the_day(s: &mut Scenario, day: u32) {
    s.run_until(2000, |sim| {
        matches!(
            sim.phase(),
            Phase::Night {
                step: NightStep::Fighting { to_spawn: 0, .. },
                ..
            }
        )
    });
    s.clear_monsters();
    s.run_until(
        2000,
        |sim| matches!(sim.phase(), Phase::Day { day: d, .. } if d == day),
    );
}

#[test]
fn every_node_is_filled_again_when_a_day_begins() {
    let mut s = two_short_days();
    let full = s.sim.tuning().resources.node_amount;
    s.set_node_amount(NODE, 3);

    clear_the_night_and_wait_for_the_day(&mut s, 2);

    assert_eq!(node_amount(&s), full);
}

#[test]
fn what_players_carry_and_what_lies_on_the_ground_carries_over_to_the_next_day() {
    let mut s = two_short_days();
    let a = s.player_at(10.0, 10.0);
    s.set_carried(a, 6);
    s.pile_at(5.0, 5.0, 4);

    clear_the_night_and_wait_for_the_day(&mut s, 2);

    assert_eq!(s.player(a).carried(), 6);
    assert_eq!(s.sim.piles().next().expect("the pile stays").amount, 4);
}

#[test]
fn a_new_expedition_starts_with_full_nodes_and_no_piles() {
    let mut s = two_short_days();
    let a = s.player_at(10.0, 10.0);
    s.set_carried(a, 6);
    s.pile_at(5.0, 5.0, 4);
    s.set_node_amount(NODE, 1);
    s.set_wagon_hp(0);

    s.run_until(2000, |sim| sim.expedition_number() == 2);

    assert_eq!(node_amount(&s), s.sim.tuning().resources.node_amount);
    assert_eq!(s.sim.piles().count(), 0);
    assert_eq!(s.player(a).carried(), 0);
}

// 총량

#[test]
fn the_total_amount_changes_only_when_the_players_gather() {
    // 채집 지점은 마차에서 7타일 떨어져 있고 마을의 범위는 3타일이라, 지점은 마을 밖입니다.
    let mut s = Scenario::with_tuning(|t| {
        t.map.nodes = vec![Tile::new(9, 17)];
        t.village.build_range = 3.0;
        t.monsters.grunt.attack_damage = 1000;
        t.wagon.max_hp = u32::MAX;
    });
    let start = s.sim.stash();
    let worker = s.player_at(10.8, 17.5);
    s.hold_interact(worker);
    let builder = s.player_at(14.5, 14.5);
    s.hold_build(builder, 1.0, 0.0);
    let bystander = s.player_at(11.5, 15.5);

    for tick in 0..400 {
        match tick {
            // 가득 채집한 플레이어가 마차를 향해 걷다가 몬스터에게 맞아 쓰러지고, 든 자원을 떨어뜨립니다.
            120 => s.hold_move(worker, 1.0, 0.0),
            125 => {
                let pos = s.player(worker).pos;
                s.monster_at(pos.x + 0.6, pos.y);
            }
            130 => s.clear_monsters(),
            // 다른 플레이어가 떨어진 자원을 주워서 마차에 넣습니다.
            200 => s.hold_move(bystander, 0.0, 1.0),
            _ if s.player(bystander).carried() > 0 => s.hold_move(bystander, 1.0, 0.0),
            _ => {}
        }
        s.run_ticks(1);
        assert_eq!(
            resource_total(&s.sim),
            start + gathered_total(&s.sim),
            "tick {tick}: the total is the start plus what was gathered"
        );
    }

    assert_eq!(gathered_total(&s.sim), 10, "one full load was gathered");
    assert!(s.player(worker).is_downed());
    assert_eq!(s.sim.piles().count(), 0, "the dropped load was picked up");
    assert_eq!(s.player(bystander).carried(), 0);
    assert_eq!(
        s.sim.stash(),
        10,
        "and put in the stash, after the tower took the first 10"
    );
}
