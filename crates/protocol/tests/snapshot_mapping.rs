//! `Snapshot::from_sim`과 `Welcome::from_sim`이 실제 규칙(`Sim`)의 상태를 전송 값으로 옮기는지 확인합니다.
//! 규칙은 공개 함수(`add_player`, `step`)로만 움직이고, 기대하는 값은 규칙의 약속(수치표의 값)에서 구합니다.

use caravan_protocol::{
    Snapshot, Welcome, WireInteraction, WireLife, WireMonsterKind, WireOutcome, WirePhase, WirePos,
    WireTowerState,
};
use caravan_rules::{PlayerInput, PlayerInputs, Sim, Tuning, Vec2};

fn step(sim: &mut Sim, inputs: &PlayerInputs, ticks: u32) {
    for _ in 0..ticks {
        sim.step(inputs);
    }
}

/// 규칙이 `Tuning::validate`를 통과하는 수치만 받으므로, 바꾼 수치도 같은 검사를 거칩니다.
fn sim_with(adjust: impl FnOnce(&mut Tuning)) -> Sim {
    let mut tuning = Tuning::default();
    adjust(&mut tuning);
    tuning.validate().expect("the adjusted numbers are valid");
    Sim::new(tuning)
}

#[test]
fn a_fresh_room_maps_to_the_numbers_in_the_tuning_table() {
    let mut sim = Sim::new(Tuning::default());
    sim.add_player().unwrap();
    let snapshot = Snapshot::from_sim(&sim);

    assert_eq!(snapshot.tick, 0);
    assert_eq!(snapshot.expedition, 1);
    assert_eq!(
        snapshot.phase,
        WirePhase::Intermission {
            next_wave: 1,
            secs_left: 3.0
        },
        "first_delay_secs is 3, shown as seconds and not as ticks"
    );
    assert_eq!(snapshot.wave_count, 3);
    assert_eq!((snapshot.wagon.hp, snapshot.wagon.max_hp), (500, 500));
    assert!(snapshot.monsters.is_empty());

    let [tower] = snapshot.towers[..] else {
        panic!("one tower expected, got {:?}", snapshot.towers);
    };
    assert_eq!((tower.id, tower.max_hp), (1, 300));
    assert_eq!(
        tower.state,
        WireTowerState::Standing {
            hp: 300,
            last_shot: None
        }
    );

    let [player] = snapshot.players[..] else {
        panic!("one player expected, got {:?}", snapshot.players);
    };
    assert_eq!((player.id, player.team), (1, 0));
    assert_eq!(
        player.pos.to_tiles(),
        Vec2::new(12.0, 9.5),
        "first spawn point"
    );
    assert_eq!(
        player.life,
        WireLife::Alive {
            hp: 100,
            max_hp: 100,
            interaction: None
        }
    );
    assert_eq!((player.last_input_seq, player.last_attack_tick), (0, None));
}

#[test]
fn welcome_describes_the_fixed_world_and_the_joining_player() {
    let mut sim = Sim::new(Tuning::default());
    sim.add_player().unwrap();
    let second = sim.add_player().unwrap();
    let me = sim.player(second).unwrap();
    let welcome = Welcome::from_sim(&sim, me);

    assert_eq!((welcome.you, welcome.team), (2, 0));
    assert_eq!((welcome.tick_hz, welcome.capacity), (20, 4));
    let world = welcome.world;
    assert_eq!(world.size.to_tiles(), Vec2::new(24.0, 24.0));
    assert_eq!(world.obstacles.len(), 5);
    assert_eq!(world.wagon.to_rect(), Tuning::default().map.wagon);
    assert_eq!(world.towers.len(), 1);
    assert_eq!(world.towers[0].id, 1);
    assert_eq!(
        world.towers[0].rect.to_rect(),
        Tuning::default().map.structures[0].rect
    );
    assert_eq!(world.player_radius, 0.35);
    let [grunt] = world.monster_kinds[..] else {
        panic!("one monster kind expected");
    };
    assert_eq!(grunt.kind, WireMonsterKind::Grunt);
    assert_eq!((grunt.radius, grunt.max_hp), (0.4, 60));
}

#[test]
fn monsters_and_the_tower_shots_at_them_appear_once_the_first_wave_comes() {
    let mut sim = Sim::new(Tuning::default());
    let none = PlayerInputs::new();
    step(&mut sim, &none, 60);
    let snapshot = Snapshot::from_sim(&sim);
    assert_eq!(snapshot.phase, WirePhase::Fighting { wave: 1 });
    assert_eq!(snapshot.tick, 60);

    // 규칙의 몬스터와 전송된 몬스터는 같은 순서, 같은 번호, 같은 HP이고 위치는 오차 안에서 같습니다.
    let mut shot = None;
    for _ in 0..400 {
        sim.step(&none);
        let snapshot = Snapshot::from_sim(&sim);
        let rules: Vec<_> = sim.monsters().collect();
        assert_eq!(snapshot.monsters.len(), rules.len());
        for (wire, rule) in snapshot.monsters.iter().zip(&rules) {
            assert_eq!((wire.id, wire.hp), (rule.id.0, rule.hp));
            assert_eq!(wire.kind, WireMonsterKind::Grunt);
            let back = wire.pos.to_tiles();
            assert!((back.x - rule.pos.x).abs() <= WirePos::MAX_ERROR_TILES);
            assert!((back.y - rule.pos.y).abs() <= WirePos::MAX_ERROR_TILES);
        }
        if let WireTowerState::Standing {
            last_shot: Some(last),
            ..
        } = snapshot.towers[0].state
        {
            shot = Some((last, snapshot.tick));
            break;
        }
    }
    let (shot, seen_at) = shot.expect("the tower shoots a monster that comes into range");
    assert!(shot.tick <= seen_at, "a shot is never from the future");
}

#[test]
fn a_destroyed_tower_has_no_hp_on_the_wire() {
    // 몬스터가 감시탑 바로 옆에서 나타나서 한 번에 부숩니다.
    let mut sim = sim_with(|t| {
        t.structures.watchtower.max_hp = 10;
        t.map.monster_spawns = vec![Vec2::new(15.5, 10.5)];
        t.waves.sizes = vec![1];
        t.waves.spawn_lanes = 1;
        t.waves.first_delay_secs = 0.1;
    });
    step(&mut sim, &PlayerInputs::new(), 10);
    assert_eq!(
        Snapshot::from_sim(&sim).towers[0].state,
        WireTowerState::Destroyed
    );
}

#[test]
fn a_downed_player_and_the_reviver_show_the_same_progress() {
    // 몬스터가 첫 플레이어 옆에서 나타나서 한 번에 쓰러뜨리고, 쿨다운이 길어서 다시 공격하지 않습니다.
    // 두 번째 플레이어는 처음부터 상호작용 키를 누르고 있습니다.
    let mut sim = sim_with(|t| {
        t.max_players = 2;
        t.map.player_spawns = vec![Vec2::new(10.0, 2.0), Vec2::new(11.0, 2.0)];
        t.map.monster_spawns = vec![Vec2::new(10.0, 2.5)];
        t.waves.sizes = vec![1];
        t.waves.spawn_lanes = 1;
        t.waves.first_delay_secs = 0.1;
        t.monsters.grunt.attack_damage = 100;
        t.monsters.grunt.attack_cooldown_secs = 100.0;
        t.wagon.max_hp = u32::MAX;
    });
    let downed_id = sim.add_player().unwrap();
    let reviver_id = sim.add_player().unwrap();
    let holding = PlayerInputs::from([(
        reviver_id,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    )]);

    step(&mut sim, &holding, 4);
    let snapshot = Snapshot::from_sim(&sim);
    let WireLife::Downed {
        bleed_secs_left, ..
    } = snapshot.players[0].life
    else {
        panic!(
            "the first player is downed, got {:?}",
            snapshot.players[0].life
        );
    };
    assert!(
        bleed_secs_left > 29.0 && bleed_secs_left <= 30.0,
        "bleed time is shown in seconds: {bleed_secs_left}"
    );

    // 8초 가운데 절반쯤 지났을 때의 모습입니다.
    step(&mut sim, &holding, 80);
    let snapshot = Snapshot::from_sim(&sim);
    let WireLife::Downed {
        revive_progress: Some(on_downed),
        ..
    } = snapshot.players[0].life
    else {
        panic!(
            "a revive is in progress, got {:?}",
            snapshot.players[0].life
        );
    };
    let WireLife::Alive {
        interaction: Some(WireInteraction::Reviving { target, progress }),
        ..
    } = snapshot.players[1].life
    else {
        panic!(
            "the second player is reviving, got {:?}",
            snapshot.players[1].life
        );
    };
    assert_eq!(target, downed_id.0);
    assert_eq!(progress, on_downed);
    assert!(
        (0.4..=0.6).contains(&progress),
        "about half of 8 seconds has passed: {progress}"
    );

    step(&mut sim, &holding, 100);
    let snapshot = Snapshot::from_sim(&sim);
    assert_eq!(
        snapshot.players[0].life,
        WireLife::Alive {
            hp: 30,
            max_hp: 100,
            interaction: None
        },
        "revived with 30 percent of the maximum"
    );
}

#[test]
fn an_expedition_that_ends_shows_its_outcome_and_the_seconds_until_the_next() {
    // 몬스터가 마차 바로 옆에서 나타나고, 플레이어를 쫓지 않으므로 마차만 공격합니다. 플레이어가 있어야 실패로 판정됩니다.
    let mut sim = sim_with(|t| {
        t.wagon.max_hp = 1;
        t.monsters.grunt.aggro_range = 0.1;
        t.map.monster_spawns = vec![Vec2::new(12.0, 10.5)];
        t.waves.sizes = vec![1];
        t.waves.spawn_lanes = 1;
        t.waves.first_delay_secs = 0.1;
    });
    sim.add_player().unwrap();
    let none = PlayerInputs::new();
    let mut over = None;
    for _ in 0..100 {
        sim.step(&none);
        let snapshot = Snapshot::from_sim(&sim);
        if matches!(snapshot.phase, WirePhase::Over { .. }) {
            over = Some(snapshot);
            break;
        }
    }
    let over = over.expect("the wagon falls and the expedition ends");
    assert_eq!(
        over.phase,
        WirePhase::Over {
            outcome: WireOutcome::Defeat,
            secs_left: 5.0
        },
        "restart_secs is 5, shown as seconds"
    );
    assert_eq!(over.wagon.hp, 0);

    step(&mut sim, &none, 20);
    assert_eq!(
        Snapshot::from_sim(&sim).phase,
        WirePhase::Over {
            outcome: WireOutcome::Defeat,
            secs_left: 4.0
        }
    );
}
