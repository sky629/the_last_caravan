//! `Snapshot::from_sim`과 `Welcome::from_sim`이 실제 규칙(`Sim`)의 상태를 전송 값으로 옮기는지 확인합니다.
//! 규칙은 공개 함수(`add_player`, `step`)로만 움직이고, 기대하는 값은 규칙의 약속(수치표의 값)에서 구합니다.

use caravan_protocol::{
    RoomCode, SeatToken, Snapshot, WeaponInfo, Welcome, WireAim, WireAngle, WireAttack, WireBoss,
    WireBossAct, WireBuffs, WireBuildRefusal, WireCannotBuild, WireDelivery, WireInteraction,
    WireLife, WireLoadout, WireMonsterKind, WireNightStep, WireOutcome, WirePhase, WirePos,
    WirePrompt, WireRepairTarget, WireStruck, WireStructureKind, WireTile, WireTowerState,
    WireWeaponKind,
};
use caravan_rules::{
    AimDir, Finale, Loadout, MonsterKind, MoveDir, NightPlan, Pack, PlayerInput, PlayerInputs,
    Seed, Sim, Start, Tile, Tuning, Vec2, Wave, WeaponKind,
};

fn room() -> RoomCode {
    RoomCode::parse("test").expect("a valid room code")
}

fn step(sim: &mut Sim, inputs: &PlayerInputs, ticks: u32) {
    for _ in 0..ticks {
        sim.step(inputs);
    }
}

/// 밤이 하나이고 무리가 하나이며 몬스터가 한 마리인 구성입니다.
fn one_monster() -> Vec<NightPlan> {
    vec![NightPlan {
        waves: vec![Wave {
            packs: vec![Pack {
                kind: MonsterKind::Grunt,
                count: 1,
            }],
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
    tuning.camp.start = Start::AtOnce;
    adjust(&mut tuning);
    tuning.validate().expect("the adjusted numbers are valid");
    Sim::new(tuning, Seed(1))
}

#[test]
fn a_fresh_room_is_the_camp_with_the_scarecrows_and_nobody_ready() {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    sim.add_player().unwrap();
    let snapshot = Snapshot::from_sim(&sim);

    assert_eq!(snapshot.expedition, 0, "no expedition has started");
    assert_eq!(
        snapshot.phase,
        WirePhase::Camp {
            ready: vec![],
            starting_in: None,
            struck: vec![]
        }
    );
    let kinds: Vec<WireMonsterKind> = snapshot.monsters.iter().map(|m| m.status.kind()).collect();
    assert_eq!(
        kinds,
        vec![WireMonsterKind::Scarecrow; 3],
        "three scarecrows"
    );
    assert!(snapshot.monsters.iter().all(|m| m.hp == 60));
    assert!(snapshot.chests.is_empty() && snapshot.nodes.is_empty());
    assert_eq!(snapshot.stash, 0);
}

#[test]
fn a_fresh_expedition_maps_to_the_numbers_in_the_tuning_table() {
    let mut sim = sim_with(|_| {});
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
        25,
        "the day's guards are already there: two beside each of the eight nodes and three beside each of the three chests"
    );
    assert!(snapshot.monsters.iter().all(|m| m.hp == 60));

    assert_eq!(snapshot.stash, 10, "the starting stash");
    assert!(snapshot.towers.is_empty(), "the village starts empty");
    assert_eq!(
        snapshot
            .chests
            .iter()
            .map(|chest| chest.id)
            .collect::<Vec<_>>(),
        [1, 2, 3],
        "three chests, numbered from 1"
    );
    let sites = &sim.tuning().map.chest_sites;
    assert!(
        snapshot
            .chests
            .iter()
            .all(|chest| sites.contains(&chest.tile.to_tile())),
        "each chest stands on a candidate site"
    );

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
            build_refusal: None,
            buffs: WireBuffs::NONE,
        }
    );
    assert_eq!((player.last_input_seq, player.last_attack), (0, None));
    assert_eq!(
        player.loadout,
        WireLoadout {
            held: WireWeaponKind::Sword,
            spare: None
        },
        "a player starts with the sword and an empty second slot"
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
fn a_chest_its_prompt_and_the_opening_progress_reach_the_snapshot_and_the_weapon_comes_out() {
    let mut sim = sim_with(|t| {
        t.map.chest_sites = vec![Tile::new(47, 32)];
        t.loot.chests_per_day = 1;
        t.guards.per_chest = 0;
        // 상자의 남쪽 타일에 섭니다. 플레이어의 가장자리에서 상자의 가장자리까지 0.25타일입니다.
        t.map.player_spawns = vec![Vec2::new(47.5, 33.6)];
    });
    let id = sim.add_player().unwrap();

    let snapshot = Snapshot::from_sim(&sim);
    assert_eq!(snapshot.chests.len(), 1);
    assert_eq!(
        snapshot.chests[0].tile,
        WireTile { x: 47, y: 32 },
        "the chest says where it stands and nothing about what is inside"
    );
    assert_eq!(snapshot.players[0].prompt, Some(WirePrompt::OpenChest));

    let holding = PlayerInputs::from([(
        id,
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        },
    )]);
    let open_ticks = sim.tuning().ticks(sim.tuning().loot.open_secs);
    step(&mut sim, &holding, open_ticks / 2);
    let snapshot = Snapshot::from_sim(&sim);
    let WireLife::Alive {
        interaction: Some(WireInteraction::Opening { chest, progress }),
        ..
    } = snapshot.players[0].life
    else {
        panic!("the player is opening, got {:?}", snapshot.players[0].life);
    };
    assert_eq!(chest, snapshot.chests[0].id);
    assert!((progress - 0.5).abs() < 0.05, "about half way: {progress}");
    assert_eq!(snapshot.players[0].prompt, None, "the bar shows instead");

    step(&mut sim, &holding, open_ticks - open_ticks / 2);
    let snapshot = Snapshot::from_sim(&sim);
    assert!(snapshot.chests.is_empty(), "the chest is open");
    let [weapon] = snapshot.ground_weapons[..] else {
        panic!("one weapon lies there, got {:?}", snapshot.ground_weapons);
    };
    assert_eq!(weapon.pos.to_tiles(), Vec2::new(47.5, 32.5));
    assert_ne!(
        weapon.kind,
        WireWeaponKind::Sword,
        "a chest never holds the sword"
    );
    assert_eq!(
        snapshot.players[0].prompt,
        Some(WirePrompt::TakeWeapon(weapon.kind)),
        "F now takes the weapon"
    );
}

#[test]
fn welcome_describes_the_fixed_world_and_the_joining_player() {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    sim.add_player().unwrap();
    let second = sim.add_player().unwrap();
    let me = sim.player(second).unwrap();
    let welcome = Welcome::from_sim(&sim, me, &room(), SeatToken([7; 16]));

    assert_eq!((welcome.you, welcome.team), (2, 0));
    assert_eq!((welcome.tick_hz, welcome.capacity), (20, 4));
    assert_eq!(
        welcome.room, "test",
        "the code of the room the player joined"
    );
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
    let [grunt, scarecrow, boss, runner] = world.monster_kinds[..] else {
        panic!("the grunt, the scarecrow, the boss and the runner expected");
    };
    assert_eq!(grunt.kind, WireMonsterKind::Grunt);
    assert_eq!((grunt.radius, grunt.max_hp), (0.4, 60));
    assert_eq!(scarecrow.kind, WireMonsterKind::Scarecrow);
    assert_eq!((scarecrow.radius, scarecrow.max_hp), (0.4, 60));
    // 보스의 반지름은 그리는 크기이므로 맞는 몸의 반지름이고, 최대 HP는 기준 인원일 때의 값입니다.
    assert_eq!(boss.kind, WireMonsterKind::Boss);
    assert_eq!(
        (boss.radius, boss.max_hp),
        (1.0, Tuning::default().monsters.boss.max_hp)
    );
    assert_eq!(runner.kind, WireMonsterKind::Runner);
    assert_eq!((runner.radius, runner.max_hp), (0.3, 30));
    let info = world.boss.expect("the default tuning has a boss");
    assert_eq!(info.slam_radius, 2.5);
}

#[test]
fn welcome_has_no_boss_for_a_tuning_without_one() {
    let mut sim = sim_with(|t| t.nights.finale = Finale::LastWave);
    let me = sim.add_player().unwrap();
    let welcome = Welcome::from_sim(&sim, sim.player(me).unwrap(), &room(), SeatToken([7; 16]));

    assert_eq!(welcome.world.boss, None);
}

/// 야영지에서 시작 무기가 `weapon`인 플레이어가 바로 앞(한 걸음 떨어진 곳)의 허수아비를 동쪽으로 한 번 쏘거나 벤 뒤의 허수아비입니다.
fn scarecrow_after_one_blow_with(weapon: WeaponKind) -> caravan_protocol::WireMonster {
    let mut tuning = Tuning::default();
    tuning.player.start_loadout = Loadout::new(weapon, None);
    // 플레이어는 등장 지점 (32, 29.5)에 서므로 허수아비를 그 동쪽 한 타일 곁에 둡니다.
    tuning.map.camp.scarecrows = vec![Vec2::new(33.0, 29.5)];
    tuning.validate().expect("the adjusted numbers are valid");
    let mut sim = Sim::new(tuning, Seed(1));
    let player = sim.add_player().expect("the room has space");
    let blow = PlayerInputs::from([(
        player,
        PlayerInput {
            attack: true,
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &blow, 1);
    *Snapshot::from_sim(&sim)
        .monsters
        .first()
        .expect("the scarecrow")
}

#[test]
fn a_scarecrow_hit_by_a_weapon_shows_the_effect_of_that_weapon_in_the_snapshot() {
    for (weapon, expected) in [
        (WeaponKind::Sword, (false, false, true)),
        (WeaponKind::Spear, (false, false, true)),
        (WeaponKind::Greatsword, (true, false, false)),
        (WeaponKind::Rifle, (true, false, false)),
        (WeaponKind::Bow, (false, true, false)),
        (WeaponKind::Crossbow, (false, true, false)),
    ] {
        let scarecrow = scarecrow_after_one_blow_with(weapon);
        let effects = scarecrow.status.effects();
        assert_eq!(
            (effects.stunned, effects.slowed, effects.bleeding),
            expected,
            "{weapon:?}: (stunned, slowed, bleeding)"
        );
        assert_eq!(scarecrow.status.kind(), WireMonsterKind::Scarecrow);
        assert_eq!(scarecrow.hp, 60, "nothing is hurt in the camp");
    }
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
            assert_eq!(wire.status.kind(), WireMonsterKind::Grunt);
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

/// 야영지에서 시작하는 방입니다. 허수아비 하나가 플레이어의 등장 지점 (32, 29.5)의 동쪽 1.5타일 (33.5, 29.5)에 섭니다.
fn camp_with_a_scarecrow_beside_the_start() -> Sim {
    sim_with(|t| {
        t.camp.start = Start::WhenReady;
        t.map.camp.scarecrows = vec![Vec2::new(33.5, 29.5)];
    })
}

fn press(input: PlayerInput, id: caravan_rules::PlayerId) -> PlayerInputs {
    PlayerInputs::from([(id, input)])
}

#[test]
fn a_camp_maps_who_is_ready_and_the_seconds_until_the_expedition_starts() {
    let mut sim = camp_with_a_scarecrow_beside_the_start();
    let a = sim.add_player().unwrap();
    let b = sim.add_player().unwrap();
    let ready = PlayerInput {
        toggle_ready: true,
        ..PlayerInput::default()
    };
    assert_eq!(Snapshot::from_sim(&sim).expedition, 0);

    step(&mut sim, &press(ready, a), 1);
    let snapshot = Snapshot::from_sim(&sim);
    assert_eq!(
        snapshot.phase,
        WirePhase::Camp {
            ready: vec![a.0],
            starting_in: None,
            struck: vec![]
        },
        "one ready player does not start the countdown"
    );

    step(&mut sim, &press(ready, b), 1);
    assert_eq!(
        Snapshot::from_sim(&sim).phase,
        WirePhase::Camp {
            ready: vec![a.0, b.0],
            starting_in: Some(3.0),
            struck: vec![]
        },
        "countdown_secs is 3, shown as seconds"
    );

    step(&mut sim, &PlayerInputs::new(), 20);
    assert_eq!(
        Snapshot::from_sim(&sim).phase,
        WirePhase::Camp {
            ready: vec![a.0, b.0],
            starting_in: Some(2.0),
            struck: vec![]
        }
    );
}

#[test]
fn a_scarecrow_that_was_hit_is_on_the_wire_with_the_tick_and_the_damage() {
    let mut sim = camp_with_a_scarecrow_beside_the_start();
    let fighter = sim.add_player().unwrap();
    let scarecrow = sim.monsters().next().expect("the camp has a scarecrow").id;
    let swing = PlayerInput {
        aim: AimDir::new(1.0, 0.0),
        attack: true,
        ..PlayerInput::default()
    };

    step(&mut sim, &press(swing, fighter), 1);

    let snapshot = Snapshot::from_sim(&sim);
    let WirePhase::Camp { struck, .. } = snapshot.phase else {
        panic!("the room is in the camp");
    };
    assert_eq!(
        struck,
        vec![WireStruck {
            monster: scarecrow.0,
            tick: snapshot.tick,
            amount: sim.tuning().weapons.sword.damage
        }]
    );
    assert_eq!(
        snapshot.monsters[0].hp,
        sim.tuning().monsters.scarecrow.max_hp
    );
}

#[test]
fn building_in_the_camp_shows_the_camp_as_the_reason_on_the_wire() {
    let mut sim = camp_with_a_scarecrow_beside_the_start();
    let builder = sim.add_player().unwrap();
    let building = PlayerInput {
        build: true,
        aim: AimDir::new(0.0, -1.0),
        ..PlayerInput::default()
    };

    step(&mut sim, &press(building, builder), 3);

    let snapshot = Snapshot::from_sim(&sim);
    let WireLife::Alive { build_refusal, .. } = snapshot.players[0].life else {
        panic!("alive");
    };
    assert_eq!(
        build_refusal.map(|refusal| refusal.why),
        Some(WireCannotBuild::InCamp)
    );
}

#[test]
fn a_tower_carries_its_kind_and_the_welcome_carries_its_max_hp_range_and_cost() {
    // 보관함에 스턴 포탑(20)을 지을 자원이 있고, 플레이어가 E를 두 번 누른 뒤 B로 짓기 시작합니다.
    let mut sim = sim_with(|t| t.wagon.start_stash = 30);
    let builder = sim.add_player().unwrap();
    let press_e = PlayerInputs::from([(
        builder,
        PlayerInput {
            cycle_tower: true,
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &press_e, 2);
    assert_eq!(
        Snapshot::from_sim(&sim).players[0].build_kind,
        WireStructureKind::StunTower,
        "the choice is sent with the player"
    );
    let building = PlayerInputs::from([(
        builder,
        PlayerInput {
            build: true,
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &building, 1);

    let snapshot = Snapshot::from_sim(&sim);
    let [site] = snapshot.towers[..] else {
        panic!("one site expected, got {:?}", snapshot.towers);
    };
    assert_eq!(site.kind, WireStructureKind::StunTower);

    let player = sim.player(builder).unwrap();
    let welcome = Welcome::from_sim(&sim, player, &room(), SeatToken([7; 16]));
    let kinds: Vec<_> = welcome
        .world
        .structure_kinds
        .iter()
        .map(|info| {
            (
                info.kind,
                info.max_hp,
                info.build_cost,
                info.range,
                info.covers_area,
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            (WireStructureKind::Watchtower, 300, 10, 6.0, false),
            (WireStructureKind::SlowTower, 300, 15, 4.0, true),
            (WireStructureKind::StunTower, 300, 20, 5.0, false),
            (WireStructureKind::HasteTower, 300, 15, 3.0, true),
            (WireStructureKind::SwiftTower, 300, 15, 3.0, true),
        ],
        "one row for each kind in the order of choosing"
    );
    let stun = welcome
        .world
        .structure(site.kind)
        .expect("the kind is listed");
    assert_eq!((stun.build_cost, stun.range), (20, 5.0));
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
        // 낮의 몬스터가 밤에 마차로 와서 플레이어를 공격하지 않게 합니다. 상자를 지키는 몬스터가 가장 가깝습니다.
        t.loot.chests_per_day = 0;
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
            build_refusal: None,
            buffs: WireBuffs::NONE,
        },
        "revived with 30 percent of the maximum HP and a full gauge"
    );
}

#[test]
fn an_expedition_that_ends_shows_its_outcome_and_the_seconds_until_the_camp() {
    // 몬스터가 마차 바로 옆에서 나타나고, 플레이어를 쫓지 않으므로 마차만 공격합니다. 플레이어가 있어야 실패로 판정됩니다.
    let mut sim = sim_with(|t| {
        t.camp.debrief_secs = 5.0;
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
        if matches!(snapshot.phase, WirePhase::Debrief { .. }) {
            over = Some(snapshot);
            break;
        }
    }
    let over = over.expect("the wagon falls and the expedition ends");
    assert_eq!(
        over.phase,
        WirePhase::Debrief {
            outcome: WireOutcome::Defeat,
            secs_left: 5.0,
            confirmed: vec![],
        },
        "debrief_secs is 5, shown as seconds"
    );
    assert_eq!(over.wagon.hp, 0);

    step(&mut sim, &none, 20);
    assert_eq!(
        Snapshot::from_sim(&sim).phase,
        WirePhase::Debrief {
            outcome: WireOutcome::Defeat,
            secs_left: 4.0,
            confirmed: vec![],
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
            build_refusal: None,
            buffs: WireBuffs::NONE,
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

/// 검과 활을 두 칸에 가지고 시작하는 방입니다.
fn sim_with_sword_and_bow(adjust: impl FnOnce(&mut Tuning)) -> Sim {
    sim_with(|t| {
        t.player.start_loadout = Loadout::new(WeaponKind::Sword, Some(WeaponKind::Bow));
        adjust(t);
    })
}

#[test]
fn both_slots_and_the_held_one_are_on_the_wire() {
    let mut sim = sim_with_sword_and_bow(|_| {});
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
    let loadout_of = |id: u32| players.iter().find(|p| p.id == id).map(|p| p.loadout);
    assert_eq!(
        loadout_of(alive.0),
        Some(WireLoadout {
            held: WireWeaponKind::Bow,
            spare: Some(WireWeaponKind::Sword)
        })
    );
    assert_eq!(
        loadout_of(other.0),
        Some(WireLoadout {
            held: WireWeaponKind::Sword,
            spare: Some(WireWeaponKind::Bow)
        }),
        "only the player who pressed Q switched"
    );
}

#[test]
fn the_last_attack_on_the_wire_names_the_weapon_that_attacked_not_the_weapon_held_now() {
    let mut sim = sim_with_sword_and_bow(|_| {});
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
    assert_eq!(player.loadout.held, WireWeaponKind::Bow);
    assert_eq!(
        player.last_attack,
        Some(WireAttack {
            tick: swung_at,
            weapon: WireWeaponKind::Sword,
            trace_end: None
        }),
        "the effect to draw is the sword swing"
    );
}

#[test]
fn a_rifle_shot_puts_the_end_of_its_line_in_the_last_attack_and_a_swing_puts_none() {
    let mut sim = sim_with(|t| {
        t.player.start_loadout = Loadout::new(WeaponKind::Rifle, Some(WeaponKind::Sword));
    });
    let shooter = sim.add_player().unwrap();
    let aim_east = |attack: bool, switch_weapon: bool| {
        PlayerInputs::from([(
            shooter,
            PlayerInput {
                aim: AimDir::new(1.0, 0.0),
                attack,
                switch_weapon,
                ..PlayerInput::default()
            },
        )])
    };
    // 플레이어는 (32, 29.5)에 서 있고, 소총의 사거리는 12타일이며, 동쪽의 12타일 안에는 막힌 곳이 없습니다.
    let start = Snapshot::from_sim(&sim).players[0].pos.to_tiles();

    step(&mut sim, &aim_east(true, false), 1);

    let snapshot = Snapshot::from_sim(&sim);
    let shot = snapshot.players[0].last_attack.expect("the rifle fired");
    assert_eq!(shot.weapon, WireWeaponKind::Rifle);
    assert_eq!(shot.tick, snapshot.tick);
    assert_eq!(
        shot.trace_end,
        Some(WirePos::from_tiles(Vec2::new(start.x + 12.0, start.y))),
        "the line ends at the range, east of the shooter"
    );

    // 쿨다운은 무기를 바꿔도 이어지므로, 소총의 쿨다운(1.5초)이 끝난 뒤에 검이 휘둘러집니다.
    step(&mut sim, &aim_east(false, true), 1);
    step(&mut sim, &aim_east(true, false), 40);
    let swing = Snapshot::from_sim(&sim).players[0]
        .last_attack
        .expect("the sword swung");
    assert_eq!(swing.weapon, WireWeaponKind::Sword);
    assert_eq!(swing.trace_end, None, "a swing has no line");
}

#[test]
fn a_shot_arrow_is_on_the_wire_with_its_position_and_direction_and_is_gone_when_it_ends() {
    let mut sim = sim_with_sword_and_bow(|_| {});
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
fn the_welcome_lists_one_rack_for_each_weapon_in_whichever_stage_the_room_is() {
    let expected: Vec<(WireWeaponKind, Vec2)> = Tuning::default()
        .map
        .camp
        .racks()
        .map(|(kind, pos)| (WireWeaponKind::of(kind), pos))
        .collect();
    assert_eq!(expected.len(), WeaponKind::ALL.len());

    for start in [Start::WhenReady, Start::AtOnce] {
        let mut sim = sim_with(|t| t.camp.start = start);
        let first = sim.add_player().unwrap();

        let welcome = Welcome::from_sim(
            &sim,
            sim.player(first).unwrap(),
            &room(),
            SeatToken([7; 16]),
        );

        let racks: Vec<(WireWeaponKind, Vec2)> = welcome
            .world
            .racks
            .iter()
            .map(|rack| (rack.kind, rack.pos.to_tiles()))
            .collect();
        assert_eq!(racks.len(), expected.len(), "{start:?}");
        for ((kind, pos), (want_kind, want_pos)) in racks.iter().zip(&expected) {
            assert_eq!(kind, want_kind, "{start:?}");
            assert!(
                (pos.x - want_pos.x).abs() <= WirePos::MAX_ERROR_TILES
                    && (pos.y - want_pos.y).abs() <= WirePos::MAX_ERROR_TILES,
                "{start:?}: {pos:?} for {want_pos:?}"
            );
        }
    }
}

#[test]
fn the_welcome_lists_the_nodes_of_the_expedition_even_while_the_room_is_in_the_camp() {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    assert_eq!(sim.nodes().count(), 0, "the camp has no gather nodes");
    let first = sim.add_player().unwrap();

    let welcome = Welcome::from_sim(
        &sim,
        sim.player(first).unwrap(),
        &room(),
        SeatToken([7; 16]),
    );

    assert_eq!(welcome.world.nodes.len(), Tuning::default().map.nodes.len());
}

#[test]
fn the_welcome_lists_every_weapon_with_the_way_its_attack_is_delivered() {
    let mut sim = Sim::new(Tuning::default(), Seed(1));
    let first = sim.add_player().unwrap();

    let welcome = Welcome::from_sim(
        &sim,
        sim.player(first).unwrap(),
        &room(),
        SeatToken([7; 16]),
    );

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
            WeaponInfo {
                kind: WireWeaponKind::Greatsword,
                delivery: WireDelivery::Arc {
                    reach: 1.3,
                    half_angle_deg: 90.0
                },
            },
            WeaponInfo {
                kind: WireWeaponKind::Spear,
                delivery: WireDelivery::Arc {
                    reach: 2.2,
                    half_angle_deg: 15.0
                },
            },
            WeaponInfo {
                kind: WireWeaponKind::Crossbow,
                delivery: WireDelivery::Projectile,
            },
            WeaponInfo {
                kind: WireWeaponKind::Rifle,
                delivery: WireDelivery::Hitscan,
            },
        ]
    );
}

#[test]
fn the_warning_the_boss_its_max_hp_its_aim_and_its_slam_reach_the_wire_in_order() {
    // 마지막 밤의 무리가 한 마리이고 감시탑 하나가 그 한 마리를 처치합니다. 그 뒤에 경고가 5초 이어지고 보스가 나타납니다.
    let mut sim = sim_with(|t| {
        t.cycle.day_secs = 0.1;
        t.nights.plans = one_monster();
        t.nights.first_wave_delay_secs = 0.05;
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
    });
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

    let mut warned_with = None;
    for _ in 0..3000 {
        sim.step(&none);
        if let WirePhase::Night {
            step: WireNightStep::Warning { secs_left },
            ..
        } = Snapshot::from_sim(&sim).phase
        {
            warned_with = Some(secs_left);
            break;
        }
    }
    let secs_left = warned_with.expect("the last wave is cleared and the warning starts");
    assert!((4.9..=5.0).contains(&secs_left), "{secs_left}");
    assert!(
        Snapshot::from_sim(&sim).monsters.is_empty(),
        "nobody is on the field during the warning"
    );

    let mut boss = None;
    for _ in 0..200 {
        sim.step(&none);
        if let WirePhase::Night {
            step: WireNightStep::Boss(state),
            ..
        } = Snapshot::from_sim(&sim).phase
        {
            boss = Some(state);
            break;
        }
    }
    let state = boss.expect("the boss comes after five seconds");
    let snapshot = Snapshot::from_sim(&sim);
    let one = Tuning::default().monsters.boss.max_hp.div_ceil(4);
    assert_eq!(state.max_hp, one, "a room of one has a quarter of the HP");
    assert_eq!(state.aim, WireAim::Wagon);
    assert_eq!(state.act, WireBossAct::Walking);
    let [body] = &snapshot.monsters[..] else {
        panic!("the boss alone: {:?}", snapshot.monsters);
    };
    assert_eq!(body.status.kind(), WireMonsterKind::Boss);
    assert_eq!(body.hp, one);

    let mut slam = None;
    for _ in 0..4000 {
        sim.step(&none);
        if let WirePhase::Night {
            step:
                WireNightStep::Boss(WireBoss {
                    act: WireBossAct::Slam { center, progress },
                    ..
                }),
            ..
        } = Snapshot::from_sim(&sim).phase
        {
            slam = Some((center, progress));
            break;
        }
    }
    let (center, progress) = slam.expect("the boss slams the wagon once it is within eight tiles");
    let wagon = Tuning::default().map.wagon.center();
    assert_eq!(
        center,
        WirePos::from_tiles(wagon),
        "on the middle of the wagon"
    );
    assert!((0.0..=1.0).contains(&progress), "{progress}");
}

#[test]
fn a_charge_its_dash_the_push_it_causes_and_the_roar_reach_the_wire_in_order() {
    // 마지막 밤의 무리가 한 마리씩 둘이고 감시탑 하나가 처치합니다. 보스는 두 무리 다음 방향(마차의 북쪽)에서 나타나 마차를 향해 걷다가
    // 내려찍기, 돌진, 내려찍기, 포효의 차례로 행동합니다. 플레이어는 마차 북쪽의 등장 지점에서 돌진의 띠에 서서 맞고, 쓰러지지 않게 HP를 크게 둡니다.
    let mut sim = sim_with(|t| {
        t.cycle.day_secs = 0.1;
        t.nights.plans = vec![NightPlan {
            waves: vec![
                Wave {
                    packs: vec![Pack {
                        kind: MonsterKind::Grunt,
                        count: 1,
                    }],
                };
                2
            ],
        }];
        t.nights.first_wave_delay_secs = 0.05;
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
        t.player.max_hp = 5000;
    });
    let me = sim.add_player().unwrap();
    let building = PlayerInputs::from([(
        me,
        PlayerInput {
            build: true,
            aim: AimDir::new(1.0, 0.0),
            ..PlayerInput::default()
        },
    )]);
    step(&mut sim, &building, 60);

    let mut lane = None;
    let mut dash = None;
    let mut roar_seen = false;
    let mut pushed: Vec<Vec2> = Vec::new();
    let mut inputs = PlayerInputs::new();
    for _ in 0..4000 {
        sim.step(&inputs);
        let snapshot = Snapshot::from_sim(&sim);
        let WirePhase::Night {
            step: WireNightStep::Boss(boss),
            ..
        } = &snapshot.phase
        else {
            continue;
        };
        let player = snapshot.players.iter().find(|player| player.id == me.0);
        match boss.act {
            WireBossAct::Charge { from, to, progress } => {
                assert!((0.0..=1.0).contains(&progress), "{progress}");
                lane = Some((from, to));
                let (from, to) = (from.to_tiles(), to.to_tiles());
                let middle = (from + to) * 0.5;
                let me_at = player.map_or(from, |player| player.pos.to_tiles());
                let toward = (middle - me_at).normalize_or_zero();
                let arrived = me_at.distance(middle) < 0.15;
                inputs = PlayerInputs::from([(
                    me,
                    PlayerInput {
                        move_dir: if arrived {
                            MoveDir::new(0.0, 0.0)
                        } else {
                            MoveDir::new(toward.x, toward.y)
                        },
                        ..PlayerInput::default()
                    },
                )]);
            }
            WireBossAct::Dash { from, to } => {
                dash = Some((from, to));
                inputs = PlayerInputs::new();
            }
            WireBossAct::Roar { progress } => {
                assert!((0.0..=1.0).contains(&progress), "{progress}");
                roar_seen = true;
            }
            WireBossAct::Walking | WireBossAct::Slam { .. } => {}
        }
        if let Some(player) = player
            && matches!(
                player.life,
                WireLife::Alive {
                    interaction: Some(WireInteraction::Knocked),
                    ..
                }
            )
        {
            pushed.push(player.pos.to_tiles());
        }
        if roar_seen {
            break;
        }
    }

    assert_eq!(
        dash, lane,
        "the lane is shown and then run exactly as shown"
    );
    assert!(roar_seen, "the fourth act is the roar");
    assert!(
        pushed.len() >= 6,
        "the player was pushed for 0.3 seconds: {pushed:?}"
    );
    let steps: Vec<f32> = pushed
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .collect();
    assert!(
        steps.iter().all(|step| *step < 0.7),
        "the push is a slide in small steps, not one jump: {steps:?}"
    );
}
