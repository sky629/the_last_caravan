//! `Snapshot::from_sim`과 `Welcome::from_sim`이 실제 규칙(`Sim`)의 상태를 전송 값으로 옮기는지 확인합니다.
//! 규칙은 공개 함수(`add_player`, `step`)로만 움직이고, 기대하는 값은 규칙의 약속(수치표의 값)에서 구합니다.

use caravan_protocol::{
    Snapshot, WeaponInfo, Welcome, WireAngle, WireAttack, WireBuildRefusal, WireCannotBuild,
    WireDelivery, WireInteraction, WireLife, WireMonsterKind, WireOutcome, WirePhase, WirePos,
    WireRepairTarget, WireTile, WireTowerState, WireWeaponKind,
};
use caravan_rules::{
    AimDir, MonsterKind, MoveDir, NightPlan, PlayerInput, PlayerInputs, Seed, Sim, Tile, Tuning,
    Vec2, WavePlan,
};

fn step(sim: &mut Sim, inputs: &PlayerInputs, ticks: u32) {
    for _ in 0..ticks {
        sim.step(inputs);
    }
}

/// 밤이 하나이고 무리가 하나이며 몬스터가 한 마리인 구성입니다.
fn one_monster() -> Vec<NightPlan> {
    vec![NightPlan {
        waves: vec![WavePlan {
            kind: MonsterKind::Grunt,
            count: 1,
        }],
    }]
}

/// 낮이 곧 끝나고, 밤의 몬스터가 마차의 동쪽 가장자리에서 2타일 떨어진 곳 (36, 32)에 한 마리 나타나는 수치입니다.
/// 건설할 수 있는 거리를 줄여서 몬스터가 나타나는 거리도 줄일 수 있게 합니다.
fn a_monster_right_beside_the_wagon(t: &mut Tuning) {
    t.village.build_range = 0.5;
    t.nights.spawn_distance = 2.0;
    t.nights.spawn_lanes = 1;
    t.nights.plans = one_monster();
    t.cycle.day_secs = 0.05;
    t.nights.first_wave_delay_secs = 0.05;
}

/// 규칙이 `Tuning::validate`를 통과하는 수치만 받으므로, 바꾼 수치도 같은 검사를 거칩니다.
fn sim_with(adjust: impl FnOnce(&mut Tuning)) -> Sim {
    let mut tuning = Tuning::default();
    adjust(&mut tuning);
    tuning.validate().expect("the adjusted numbers are valid");
    Sim::new(tuning, Seed(1))
}

#[test]
fn a_fresh_room_maps_to_the_numbers_in_the_tuning_table() {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    sim.add_player().unwrap();
    let snapshot = Snapshot::from_sim(&sim);

    assert_eq!(snapshot.tick, 0);
    assert_eq!(snapshot.expedition, 1);
    assert_eq!(
        snapshot.phase,
        WirePhase::Day {
            day: 1,
            secs_left: 90.0
        },
        "day_secs is 90, shown as seconds and not as ticks"
    );
    assert_eq!((snapshot.wagon.hp, snapshot.wagon.max_hp), (500, 500));
    assert_eq!(
        snapshot.monsters.len(),
        16,
        "the day's guards are already there: two beside each of the eight nodes"
    );
    assert!(snapshot.monsters.iter().all(|m| m.hp == 60));

    assert_eq!(snapshot.stash, 10, "the starting stash");
    assert!(snapshot.towers.is_empty(), "the village starts empty");

    let [player] = snapshot.players[..] else {
        panic!("one player expected, got {:?}", snapshot.players);
    };
    assert_eq!((player.id, player.team), (1, 0));
    assert_eq!(
        player.pos.to_tiles(),
        Vec2::new(32.0, 29.5),
        "first spawn point"
    );
    assert_eq!(
        player.life,
        WireLife::Alive {
            hp: 100,
            max_hp: 100,
            carried: 0,
            stamina: 100,
            interaction: None,
            build_refusal: None
        }
    );
    assert_eq!((player.last_input_seq, player.last_attack), (0, None));
    assert_eq!(
        player.weapon,
        WireWeaponKind::Sword,
        "a player starts with the sword"
    );
    assert!(snapshot.piles.is_empty(), "nothing lies on the ground");
    let amounts: Vec<(u32, u32)> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id, node.amount))
        .collect();
    assert_eq!(
        amounts,
        (1..=8).map(|id| (id, 15)).collect::<Vec<_>>(),
        "eight full nodes, numbered from 1"
    );
}

#[test]
fn welcome_describes_the_fixed_world_and_the_joining_player() {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    sim.add_player().unwrap();
    let second = sim.add_player().unwrap();
    let me = sim.player(second).unwrap();
    let welcome = Welcome::from_sim(&sim, me);

    assert_eq!((welcome.you, welcome.team), (2, 0));
    assert_eq!((welcome.tick_hz, welcome.capacity), (20, 4));
    let world = welcome.world;
    assert_eq!(world.size.to_tiles(), Vec2::new(64.0, 64.0));
    assert_eq!(world.obstacles.len(), Tuning::default().map.obstacles.len());
    assert_eq!(world.wagon.to_rect(), Tuning::default().map.wagon);
    assert_eq!(world.day_count, 3);
    assert_eq!(world.tower_limit, 40);
    assert_eq!((world.carry_capacity, world.node_amount), (10, 15));
    let sites: Vec<(u32, Tile)> = world
        .nodes
        .iter()
        .map(|node| (node.id, node.tile.to_tile()))
        .collect();
    let expected: Vec<(u32, Tile)> = Tuning::default()
        .map
        .nodes
        .iter()
        .enumerate()
        .map(|(index, tile)| (index as u32 + 1, *tile))
        .collect();
    assert_eq!(sites, expected, "the nodes of the map, numbered from 1");
    assert_eq!(world.player_radius, 0.35);
    let [grunt] = world.monster_kinds[..] else {
        panic!("one monster kind expected");
    };
    assert_eq!(grunt.kind, WireMonsterKind::Grunt);
    assert_eq!((grunt.radius, grunt.max_hp), (0.4, 60));
}

#[test]
fn monsters_and_the_tower_shots_at_them_appear_once_the_first_wave_comes() {
    // 낮이 곧 끝나므로, 건설을 하는 동안 밤이 시작되어 첫 무리가 마을에 다가옵니다.
    let mut sim = sim_with(|t| t.cycle.day_secs = 0.1);
    let builder = sim.add_player().unwrap();
    let none = PlayerInputs::new();
    let building = PlayerInputs::from([(
        builder,
        PlayerInput {
            build: true,
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &building, 60);
    assert!(
        Snapshot::from_sim(&sim)
            .towers
            .iter()
            .all(|tower| matches!(tower.state, WireTowerState::Standing { .. })),
        "the tower stands after the build time"
    );
    let snapshot = Snapshot::from_sim(&sim);
    assert!(
        matches!(
            snapshot.phase,
            WirePhase::Night {
                day: 1,
                wave_count: 2,
                ..
            }
        ),
        "got {:?}",
        snapshot.phase
    );
    assert_eq!(snapshot.tick, 60);

    // 규칙의 몬스터와 전송된 몬스터는 같은 순서, 같은 번호, 같은 HP이고 위치는 오차 안에서 같습니다.
    let mut shot = None;
    for _ in 0..600 {
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
        if let Some(WireTowerState::Standing {
            last_shot: Some(last),
            ..
        }) = snapshot.towers.first().map(|tower| tower.state)
        {
            shot = Some((last, snapshot.tick));
            break;
        }
    }
    let (shot, seen_at) = shot.expect("the tower shoots a monster that comes into range");
    assert!(shot.tick <= seen_at, "a shot is never from the future");
}

#[test]
fn a_site_under_construction_and_a_destroyed_tower_show_on_the_wire() {
    // 마차 옆에서 건설을 시작하고 3초 가운데 절반쯤 지났을 때의 모습입니다.
    let mut sim = sim_with(|_| {});
    let builder = sim.add_player().unwrap();
    let building = PlayerInputs::from([(
        builder,
        PlayerInput {
            build: true,
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &building, 30);
    let snapshot = Snapshot::from_sim(&sim);

    let [site] = snapshot.towers[..] else {
        panic!("one site expected, got {:?}", snapshot.towers);
    };
    assert_eq!(site.tile, WireTile::from_tile(Tile::new(33, 29)));
    let WireTowerState::Building { progress } = site.state else {
        panic!("the site is under construction, got {:?}", site.state);
    };
    assert!((0.45..=0.55).contains(&progress), "about half: {progress}");
    assert_eq!(snapshot.stash, 0, "the cost was paid");
    let WireLife::Alive {
        interaction: Some(WireInteraction::Building { tower }),
        ..
    } = snapshot.players[0].life
    else {
        panic!(
            "the builder is building, got {:?}",
            snapshot.players[0].life
        );
    };
    assert_eq!(tower, site.id);
}

#[test]
fn a_refused_build_shows_the_reason_and_the_tile_on_the_wire() {
    let mut sim = sim_with(|_| {});
    let builder = sim.add_player().unwrap();
    let building = PlayerInputs::from([(
        builder,
        PlayerInput {
            build: true,
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &building, 200);
    // 보관함이 비었으므로 같은 자리에서 한 번 더 시도하면 거절됩니다.
    let builder_two = sim.add_player().unwrap();
    let again = PlayerInputs::from([(
        builder_two,
        PlayerInput {
            build: true,
            aim: AimDir::new(0.0, 1.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &again, 3);

    let snapshot = Snapshot::from_sim(&sim);
    let WireLife::Alive { build_refusal, .. } = snapshot.players[1].life else {
        panic!("alive");
    };
    assert_eq!(
        build_refusal,
        Some(WireBuildRefusal {
            tile: WireTile { x: 32, y: 35 },
            why: WireCannotBuild::NotEnoughStash
        })
    );
}

#[test]
fn a_destroyed_tower_is_gone_from_the_wire() {
    // 감시탑은 마차의 남쪽 가장자리에 붙어 서고, 몬스터는 동쪽에서 나타나 감시탑 하나만 노리고 한 번에 부숩니다.
    // 플레이어가 있으면 몬스터가 플레이어를 먼저 쫓으므로, 건설을 마치면 플레이어를 내보냅니다.
    let mut sim = sim_with(|t| {
        a_monster_right_beside_the_wagon(t);
        t.structures.watchtower.max_hp = 10;
        t.structures.watchtower.build_secs = 0.1;
        t.monsters.grunt.max_hp = 100_000;
        t.monsters.grunt.attack_damage = 10;
        t.monsters.grunt.aggro_range = 10.0;
        t.nights.first_wave_delay_secs = 3.0;
    });
    let builder = sim.add_player().unwrap();
    let building = PlayerInputs::from([(
        builder,
        PlayerInput {
            build: true,
            aim: AimDir::new(0.0, 1.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &building, 5);
    assert_eq!(Snapshot::from_sim(&sim).towers.len(), 1);
    sim.remove_player(builder);

    step(&mut sim, &PlayerInputs::new(), 200);

    assert!(Snapshot::from_sim(&sim).towers.is_empty());
}

#[test]
fn a_downed_player_and_the_reviver_show_the_same_progress() {
    // 몬스터가 첫 플레이어 옆에서 나타나서 한 번에 쓰러뜨리고, 쿨다운이 길어서 다시 공격하지 않습니다.
    // 두 번째 플레이어는 처음부터 상호작용 키를 누르고 있습니다.
    let mut sim = sim_with(|t| {
        t.max_players = 2;
        a_monster_right_beside_the_wagon(t);
        t.map.player_spawns = vec![Vec2::new(37.2, 32.0), Vec2::new(38.2, 32.0)];
        t.monsters.grunt.attack_damage = 100;
        t.monsters.grunt.attack_cooldown_secs = 100.0;
        t.wagon.max_hp = u32::MAX;
        // 부활한 직후의 HP를 확인하므로, 마차 곁의 회복이 HP를 더하지 않게 합니다.
        t.wagon.recovery.hp_per_pulse = 0;
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

    // 부활에 걸리는 시간의 절반이 지났을 때의 모습입니다.
    let revive_ticks = sim.tuning().ticks(sim.tuning().revive.revive_secs);
    step(&mut sim, &holding, revive_ticks / 2);
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
        "about half of the revive time has passed: {progress}"
    );

    step(&mut sim, &holding, revive_ticks);
    let snapshot = Snapshot::from_sim(&sim);
    assert_eq!(
        snapshot.players[0].life,
        WireLife::Alive {
            hp: 30,
            max_hp: 100,
            carried: 0,
            stamina: 100,
            interaction: None,
            build_refusal: None
        },
        "revived with 30 percent of the maximum HP and a full gauge"
    );
}

#[test]
fn an_expedition_that_ends_shows_its_outcome_and_the_seconds_until_the_next() {
    // 몬스터가 마차 바로 옆에서 나타나고, 플레이어를 쫓지 않으므로 마차만 공격합니다. 플레이어가 있어야 실패로 판정됩니다.
    let mut sim = sim_with(|t| {
        t.wagon.max_hp = 1;
        t.monsters.grunt.aggro_range = 0.1;
        a_monster_right_beside_the_wagon(t);
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

#[test]
fn a_gatherer_the_load_it_carries_and_a_dropped_pile_show_on_the_wire() {
    // 플레이어가 첫 채집 지점 (54, 41)의 서쪽 곁에서 시작합니다.
    let mut sim = sim_with(|t| {
        t.map.player_spawns = vec![Vec2::new(53.5, 41.5)];
        t.guards.per_node = 0;
    });
    let gatherer = sim.add_player().unwrap();
    let holding = PlayerInputs::from([(
        gatherer,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    )]);
    let pulse = sim.tuning().ticks(sim.tuning().resources.gather_pulse_secs);

    step(&mut sim, &holding, pulse);
    let snapshot = Snapshot::from_sim(&sim);

    assert_eq!(snapshot.nodes[0].amount, 14, "one unit left the first node");
    assert_eq!(
        snapshot.players[0].life,
        WireLife::Alive {
            hp: 100,
            max_hp: 100,
            carried: 1,
            stamina: 100,
            interaction: Some(WireInteraction::Gathering { node: 1 }),
            build_refusal: None
        }
    );

    let at = sim.player(gatherer).unwrap().pos;
    sim.remove_player(gatherer);
    let snapshot = Snapshot::from_sim(&sim);
    let [pile] = snapshot.piles[..] else {
        panic!("one pile expected, got {:?}", snapshot.piles);
    };
    assert_eq!((pile.id, pile.amount), (1, 1));
    assert_eq!(pile.pos, WirePos::from_tiles(at));
}

#[test]
fn a_repairer_is_marked_while_the_stash_cannot_pay_the_next_pulse() {
    // 몬스터가 마차를 한 번 때려서 HP가 400이 되고(쿨다운이 길어서 다시 때리지 않습니다), 플레이어는 마차의 북쪽 곁에서 F를 누르고 있습니다.
    // 보관함에는 수리의 한 번을 낼 자원 1뿐입니다.
    let mut sim = sim_with(|t| {
        a_monster_right_beside_the_wagon(t);
        t.map.player_spawns = vec![Vec2::new(32.0, 30.5)];
        t.monsters.grunt.attack_damage = 100;
        t.monsters.grunt.attack_cooldown_secs = 100.0;
        t.monsters.grunt.aggro_range = 0.1;
        t.wagon.start_stash = 1;
    });
    let repairer = sim.add_player().unwrap();
    let holding = PlayerInputs::from([(
        repairer,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    )]);
    let repairing = |sim: &Sim| match Snapshot::from_sim(sim).players[0].life {
        WireLife::Alive {
            interaction:
                Some(WireInteraction::Repairing {
                    target,
                    stash_short,
                }),
            ..
        } => Some((target, stash_short)),
        _ => None,
    };

    let mut first = None;
    for _ in 0..200 {
        sim.step(&holding);
        first = repairing(&sim);
        if first.is_some() {
            break;
        }
    }
    assert_eq!(
        first,
        Some((WireRepairTarget::Wagon, false)),
        "one resource pays the first pulse"
    );

    let pulse = sim
        .tuning()
        .ticks(sim.tuning().wagon.repair.heal.pulse_secs);
    step(&mut sim, &holding, pulse);
    let snapshot = Snapshot::from_sim(&sim);
    assert_eq!(snapshot.stash, 0, "the pulse paid with it");
    assert_eq!(snapshot.wagon.hp, 430, "and healed 30 HP");
    assert_eq!(
        repairing(&sim),
        Some((WireRepairTarget::Wagon, true)),
        "the next pulse cannot be paid"
    );
}

#[test]
fn a_sprinting_player_runs_farther_and_the_stamina_on_the_wire_drains() {
    // 20 Tick(1초) 동안 달리면 가득 찬 게이지(4초)의 4분의 1이 줄어서 75퍼센트이고, 걷는 속도 4의 1.6배로 6.4타일을 갑니다.
    let mut sim = sim_with(|_| {});
    let runner = sim.add_player().unwrap();
    let sprinting = PlayerInputs::from([(
        runner,
        PlayerInput {
            move_dir: MoveDir::new(1.0, 0.0),
            sprint: true,
            ..PlayerInput::default()
        },
    )]);
    let stamina = |sim: &Sim| match Snapshot::from_sim(sim).players[0].life {
        WireLife::Alive { stamina, .. } => stamina,
        other => panic!("alive expected, got {other:?}"),
    };
    assert_eq!(stamina(&sim), 100, "full when the player joins");

    step(&mut sim, &sprinting, 20);

    assert_eq!(stamina(&sim), 75);
    let x = Snapshot::from_sim(&sim).players[0].pos.to_tiles().x;
    assert!(
        (x - (32.0 + 6.4)).abs() <= WirePos::MAX_ERROR_TILES,
        "ran 6.4 tiles from x = 32, got {x}"
    );
}

#[test]
fn the_weapon_a_player_holds_is_on_the_wire() {
    let mut sim = sim_with(|_| {});
    let alive = sim.add_player().unwrap();
    let other = sim.add_player().unwrap();
    let press_q = PlayerInputs::from([(
        alive,
        PlayerInput {
            switch_weapon: true,
            ..PlayerInput::default()
        },
    )]);

    step(&mut sim, &press_q, 1);

    let players = Snapshot::from_sim(&sim).players;
    let weapon_of = |id: u32| players.iter().find(|p| p.id == id).map(|p| p.weapon);
    assert_eq!(weapon_of(alive.0), Some(WireWeaponKind::Bow));
    assert_eq!(
        weapon_of(other.0),
        Some(WireWeaponKind::Sword),
        "only the player who pressed Q switched"
    );
}

#[test]
fn the_last_attack_on_the_wire_names_the_weapon_that_attacked_not_the_weapon_held_now() {
    let mut sim = sim_with(|_| {});
    let fighter = sim.add_player().unwrap();
    let swing = PlayerInputs::from([(
        fighter,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    )]);
    let press_q = PlayerInputs::from([(
        fighter,
        PlayerInput {
            switch_weapon: true,
            ..PlayerInput::default()
        },
    )]);

    step(&mut sim, &swing, 1);
    let swung_at = Snapshot::from_sim(&sim).tick;
    step(&mut sim, &press_q, 1);

    let snapshot = Snapshot::from_sim(&sim);
    let player = snapshot.players.iter().find(|p| p.id == fighter.0).unwrap();
    assert_eq!(player.weapon, WireWeaponKind::Bow);
    assert_eq!(
        player.last_attack,
        Some(WireAttack {
            tick: swung_at,
            weapon: WireWeaponKind::Sword
        }),
        "the effect to draw is the sword swing"
    );
}

#[test]
fn a_shot_arrow_is_on_the_wire_with_its_position_and_direction_and_is_gone_when_it_ends() {
    let mut sim = sim_with(|_| {});
    let archer = sim.add_player().unwrap();
    let press_q = PlayerInputs::from([(
        archer,
        PlayerInput {
            switch_weapon: true,
            ..PlayerInput::default()
        },
    )]);
    let shoot = PlayerInputs::from([(
        archer,
        PlayerInput {
            aim: AimDir::new(1.0, 0.0),
            attack: true,
            ..PlayerInput::default()
        },
    )]);
    assert!(Snapshot::from_sim(&sim).projectiles.is_empty());

    step(&mut sim, &press_q, 1);
    step(&mut sim, &shoot, 1);

    let snapshot = Snapshot::from_sim(&sim);
    let [arrow] = snapshot.projectiles[..] else {
        panic!("one arrow expected, got {:?}", snapshot.projectiles);
    };
    assert_eq!(arrow.id, 1);
    let at = arrow.pos.to_tiles();
    // 플레이어는 (32, 29.5)에서 동쪽을 향해 쏘고, 활의 탄환은 한 Tick에 14 / 20 = 0.7타일 날아갑니다.
    assert!(
        (at.x - 32.7).abs() <= WirePos::MAX_ERROR_TILES
            && (at.y - 29.5).abs() <= WirePos::MAX_ERROR_TILES,
        "arrow at {at:?}"
    );
    assert_eq!(arrow.facing, WireAngle::from_direction(Vec2::new(1.0, 0.0)));

    let ended = PlayerInputs::new();
    step(&mut sim, &ended, 40);
    assert!(
        Snapshot::from_sim(&sim).projectiles.is_empty(),
        "an arrow that ended is removed from the snapshot"
    );
}

#[test]
fn the_welcome_lists_every_weapon_with_the_way_its_attack_is_delivered() {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    let first = sim.add_player().unwrap();

    let welcome = Welcome::from_sim(&sim, sim.player(first).unwrap());

    assert_eq!(
        welcome.world.weapons,
        vec![
            WeaponInfo {
                kind: WireWeaponKind::Sword,
                delivery: WireDelivery::Arc {
                    reach: 1.0,
                    half_angle_deg: 60.0
                },
            },
            WeaponInfo {
                kind: WireWeaponKind::Bow,
                delivery: WireDelivery::Projectile,
            },
        ]
    );
}
