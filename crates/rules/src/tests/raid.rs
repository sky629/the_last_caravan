//! 모의 전투: 보스와 싸우는 단순한 입력으로 규칙을 직접 돌려서 보스를 잡는 데 걸리는 시간을 잽니다.
//!
//! 입력을 만드는 쪽(`Bot`)은 화면이 알려 주는 것만 읽습니다. 보스의 몸과 예고된 영역(원과 띠), 졸개, 쓰러진 동료, 자기 자리입니다.
//! 예고를 읽는 시간이 `REACTION_TICKS`만큼 늦어서 반응이 느린 사람을 흉내 내고, 달리지 않고 걷기만 합니다.
//! 보스를 때리는 플레이어(`Role::Fighter`)는 보스에게서 마차의 반대쪽에 서서 보스를 마차에서 끌어내고, 곁에 온 졸개는 제자리에서 처치하고,
//! 쓰러진 동료에게 가장 가까운 한 명이 부활시킵니다. 방에 둘 이상이 있으면 한 명(`Role::Guard`)이 마차를 지킵니다.
//! 마차를 지키는 플레이어(`guard_the_wagon`)는 졸개가 마차에 가장 가까이 온 쪽의 마차 가장자리에서 `GUARD_POST_GAP`타일 떨어진 자리로 길을 찾아 걸어가고,
//! 곁에 온 졸개를 제자리에서 벱니다. 졸개가 없으면 서 있습니다.
//! 포탑을 짓는 일과 수리하는 일은 넣지 않았고, 예고를 읽는 시간 말고는 실수하지 않으므로 이 시간은 사람이 하는 전투의 대표값이 아니라
//! 「단순한 입력으로도 잡을 수 있는가」와 「수치가 어느 크기인가」를 보는 자로 쓰입니다.
//! 시간은 출력만 하고 단언하지 않습니다. 단언하는 것은 기본 수치에서 검을 든 4명과 1명이 보스를 잡는 것과,
//! 감시탑 15개에서 마차를 지키는 한 명이 있는 쪽이 없는 쪽보다 낫다는 것(둘 다 이기면 마차의 HP가 `GUARD_WORTH` 이상 많거나, 없는 쪽만 지는 것)뿐입니다.
//! 보스가 없는 밤의 측정(`night`)은 같은 입력으로 마차를 지키는 플레이어와 감시탑만 있는 경우를 견주어 출력합니다.
//!
//! 시간을 보려면 다음 명령을 실행합니다.
//!
//! ```text
//! cargo test -p caravan_rules raid -- --nocapture
//! ```

use std::collections::VecDeque;

use super::support::Scenario;
use crate::boss::Act;
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::PlayerId;
use crate::input::{AimDir, MoveDir, PlayerInput};
use crate::kind::{MonsterKind, WeaponKind};
use crate::math::Vec2;
use crate::monster::Monster;
use crate::nav::FlowField;
use crate::player::Player;
use crate::sim::Sim;
use crate::telegraph::Zone;
use crate::tuning::{Delivery, MapSpec, Tuning, WaveScaling};

/// 예고된 영역을 읽는 데 걸리는 Tick입니다. 0.3초이고, 입력이 늦는 정도(0.1초에서 0.15초)보다 느립니다.
const REACTION_TICKS: usize = 6;

/// 예고된 영역에서 벗어나려고 걸을 때 맵 안에 있어야 하는 거리(타일)입니다. 원의 반지름 2.5에 여유를 둔 값입니다.
const ESCAPE_TILES: f32 = 3.2;

/// 보스를 때리는 플레이어가 마차의 반대쪽으로 물러서며 보스를 끌어낼 때 맵의 가장자리에서 남겨 두는 거리(타일)입니다.
const EDGE_MARGIN: f32 = 6.0;

/// 곁에 온 졸개를 제자리에서 베기 시작하는 거리(타일)입니다. 검이 닿는 거리 1.0에 플레이어와 돌격병의 반지름을 더하고 조금 줄인 값입니다.
const SWING_REACH: f32 = 1.55;

/// 마차를 지키는 플레이어가 졸개가 가장 가까이 온 쪽의 마차 가장자리에서 떨어져 기다리는 거리(타일)입니다.
const GUARD_POST_GAP: f32 = 1.5;

/// 마차를 지키는 플레이어가 부활시키러 가는 쓰러진 동료가 마차의 가장자리에서 떨어진 가장 먼 거리(타일)입니다. 이보다 먼 동료는 보스를 때리는 사람이 부활시킵니다.
const GUARD_REACH: f32 = 6.0;

/// 마차를 지키는 한 명이 있는 쪽과 없는 쪽이 둘 다 보스를 잡았을 때, 있는 쪽이 낫다고 보는 마차의 HP 차이입니다.
const GUARD_WORTH: u32 = 50;

/// 한 번의 모의 전투가 끝났을 때의 모습입니다.
#[derive(Debug)]
struct Raid {
    outcome: Option<Outcome>,
    /// 보스가 나타난 Tick부터 전투가 끝난 Tick까지 걸린 시간(초)입니다.
    secs: f32,
    boss_hp_left: u32,
    wagon_hp: u32,
    players_standing: usize,
    towers_standing: usize,
}

/// 모의 전투의 플레이어가 하는 일입니다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// 보스에게 다가가서 때립니다.
    Fighter,
    /// 마차를 지킵니다.
    Guard,
}

struct Bot {
    id: PlayerId,
    role: Role,
    /// 예고된 영역의 기록입니다. 앞에 있는 것이 `REACTION_TICKS` Tick 전에 보인 영역이고 그것을 보고 움직입니다.
    seen: VecDeque<Option<Zone>>,
}

/// 보스가 지금 예고하거나 달리는 영역입니다. 걷거나 포효하는 동안에는 없습니다.
fn zone_now(sim: &Sim) -> Option<Zone> {
    match &sim.boss()?.act {
        Act::Slamming(telegraph) | Act::Winding { telegraph, .. } => Some(telegraph.zone),
        Act::Charging(charge) => Some(charge.zone()),
        Act::Walking { .. } | Act::Roaring { .. } => None,
    }
}

/// 여유 `margin`타일을 두고 `zone` 안에 있는지입니다.
fn is_inside(zone: &Zone, at: Vec2, margin: f32) -> bool {
    match *zone {
        Zone::Circle { center, radius } => at.distance(center) < radius + margin,
        Zone::Lane {
            from,
            to,
            half_width,
        } => {
            let (along, _) = lane_frame(from, to, at);
            at.distance(along) < half_width + margin
        }
    }
}

/// 띠의 중심선에서 `at`에 가장 가까운 점과, 그 점에서 `at`으로 향하는 방향(중심선에 수직)입니다. 중심선 위에 있으면 한쪽 옆을 고릅니다.
fn lane_frame(from: Vec2, to: Vec2, at: Vec2) -> (Vec2, Vec2) {
    let axis = to - from;
    let along = axis.normalize_or_zero();
    let reach = (at - from).dot(along).clamp(0.0, axis.length());
    let nearest = from + along * reach;
    let side = Vec2::new(-along.y, along.x);
    let away = (at - nearest).normalize_or_zero();
    (nearest, if away == Vec2::ZERO { side } else { away })
}

/// 영역을 벗어나려고 걷는 방향입니다. 원은 보스에게서 멀어지는 쪽이고(영역은 보스 곁의 플레이어를 중심으로 하므로 어느 쪽으로 걸어도 벗어나지만 보스 쪽은 보스에 닿습니다),
/// 띠는 중심선에 수직인 쪽으로 가까운 옆입니다. 그쪽으로 `ESCAPE_TILES`타일을 걸으면 맵 밖이 되는 경우에는 맵 안에 머무는 여덟 방향 가운데 가장 가까운 쪽으로 바꿉니다.
fn way_out(zone: &Zone, at: Vec2, boss: Vec2, map: &MapSpec) -> Vec2 {
    let preferred = match *zone {
        Zone::Circle { .. } => (at - boss).normalize_or_zero(),
        Zone::Lane { from, to, .. } => lane_frame(from, to, at).1,
    };
    let stays_inside = |dir: Vec2| {
        let landing = at + dir * ESCAPE_TILES;
        landing.x > 1.0
            && landing.y > 1.0
            && landing.x < map.width - 1.0
            && landing.y < map.height - 1.0
    };
    if stays_inside(preferred) {
        return preferred;
    }
    (0..8)
        .map(|step| {
            let angle = step as f32 * std::f32::consts::FRAC_PI_4;
            Vec2::new(angle.cos(), angle.sin())
        })
        .filter(|dir| stays_inside(*dir))
        .max_by(|a, b| a.dot(preferred).total_cmp(&b.dot(preferred)))
        .unwrap_or(preferred)
}

/// `weapon`으로 보스의 중심에서 몇 타일 떨어져 서면 맞는지입니다. 가까이서 치는 무기는 맞는 거리의 끝에서 조금 안쪽입니다.
fn stand_off(t: &Tuning, weapon: WeaponKind, target_radius: f32) -> f32 {
    match t.weapons.get(weapon).delivery {
        Delivery::Arc(arc) => t.player.radius + arc.reach + target_radius - 0.35,
        Delivery::Projectile(_) | Delivery::Hitscan(_) => 5.0,
    }
}

fn walking(step: Vec2, aim: Vec2, attack: bool) -> PlayerInput {
    PlayerInput {
        move_dir: MoveDir::new(step.x, step.y),
        aim: AimDir::new(aim.x, aim.y),
        attack,
        ..PlayerInput::default()
    }
}

impl Bot {
    fn input(&mut self, sim: &Sim) -> PlayerInput {
        self.seen.push_back(zone_now(sim));
        let Some(player) = sim.player(self.id).filter(|player| player.is_alive()) else {
            return PlayerInput::default();
        };
        let t = sim.tuning();
        let Some(boss) = sim.boss().and_then(|boss| {
            sim.monsters()
                .find(|monster| monster.id == boss.id)
                .map(|body| body.pos)
        }) else {
            return PlayerInput::default();
        };
        let delayed = if self.seen.len() > REACTION_TICKS {
            self.seen.pop_front().flatten()
        } else {
            None
        };
        if let Some(zone) = delayed
            && is_inside(&zone, player.pos, 0.5)
        {
            let out = way_out(&zone, player.pos, boss, &t.map);
            return walking(out, out, false);
        }
        // 영역 밖으로 나온 뒤에는 영역이 사라질 때까지 그 자리에서 기다립니다. 걸어 들어가면 터질 때 맞습니다.
        let waits_for_the_strike = delayed.is_some();

        if self.role == Role::Fighter
            && let Some(swing) = swing_at_minion_beside(sim, player.pos)
        {
            // 졸개가 곁에서 때리면 보스를 때리던 것을 멈추고 그 졸개를 처치합니다.
            return swing;
        }
        let wagon = sim.wagon().rect;
        let downed_ally = sim
            .players()
            .filter(|other| other.id != self.id && other.is_downed())
            .filter(|other| {
                self.role == Role::Fighter || wagon.distance_to(other.pos) < GUARD_REACH
            })
            .filter(|other| nearest_helper(sim, other.pos) == Some(self.id))
            .min_by(|a, b| {
                (a.pos - player.pos)
                    .length()
                    .total_cmp(&(b.pos - player.pos).length())
            });
        if let Some(ally) = downed_ally {
            // 쓰러진 동료에게 가서 F를 누르고 있습니다. 부활시키는 동안에는 공격하지 않습니다.
            let to = ally.pos - player.pos;
            let reach = t.revive.range * 0.6;
            let step = if to.length() > reach {
                to.normalize_or_zero()
            } else {
                Vec2::ZERO
            };
            return PlayerInput {
                move_dir: MoveDir::new(step.x, step.y),
                aim: AimDir::new(to.x, to.y),
                interact: true,
                ..PlayerInput::default()
            };
        }
        match self.role {
            Role::Fighter => strike_the_boss(sim, player, boss, waits_for_the_strike),
            Role::Guard => guard_the_wagon(sim, player.pos, waits_for_the_strike),
        }
    }
}

/// 보스를 때리는 플레이어의 입력입니다. 보스에게서 마차의 반대쪽에 서서 보스가 따라오며 마차에서 멀어지게 하고, 제자리에서 보스를 때립니다.
/// 예고된 영역의 밖에서 영역이 사라지기를 기다리는 동안(`waits_for_the_strike`)에는 걷지 않습니다.
fn strike_the_boss(
    sim: &Sim,
    player: &Player,
    boss: Vec2,
    waits_for_the_strike: bool,
) -> PlayerInput {
    let t = sim.tuning();
    let stand = stand_off(t, player.loadout.held(), t.monsters.boss.hit_radius());
    let step = if waits_for_the_strike {
        Vec2::ZERO
    } else {
        let outward = boss - sim.wagon().rect.center();
        let spot = boss + outward.normalize_or_zero() * stand;
        let spot = Vec2::new(
            spot.x.clamp(EDGE_MARGIN, t.map.width - EDGE_MARGIN),
            spot.y.clamp(EDGE_MARGIN, t.map.height - EDGE_MARGIN),
        );
        let to_spot = spot - player.pos;
        if to_spot.length() > 0.5 {
            to_spot.normalize_or_zero()
        } else {
            Vec2::ZERO
        }
    };
    walking(step, boss - player.pos, true)
}

/// 마차를 지키는 플레이어의 입력입니다. 곁에 온 졸개는 제자리에서 벱니다. 곁에 졸개가 없으면 마차에 가장 가까운 졸개가 있는 쪽의 마차 가장자리에서
/// `GUARD_POST_GAP`타일 떨어진 자리로 길을 따라 걸어가고, 닿으면 그 졸개를 보며 기다립니다. 졸개가 없으면 서 있습니다.
/// 예고된 영역의 밖에서 영역이 사라지기를 기다리는 동안(`waits_for_the_strike`)에는 걷지 않습니다.
fn guard_the_wagon(sim: &Sim, at: Vec2, waits_for_the_strike: bool) -> PlayerInput {
    if let Some(swing) = swing_at_minion_beside(sim, at) {
        return swing;
    }
    let wagon = sim.wagon().rect;
    let Some(threat) = sim
        .monsters()
        .filter(|monster| is_minion(monster))
        .map(|monster| monster.pos)
        .min_by(|a, b| wagon.distance_to(*a).total_cmp(&wagon.distance_to(*b)))
    else {
        return walking(Vec2::ZERO, Vec2::new(1.0, 0.0), false);
    };
    let edge = wagon.closest_point(threat);
    let post = edge + (threat - edge).normalize_or_zero() * GUARD_POST_GAP;
    if waits_for_the_strike || at.distance(post) < 0.4 {
        return walking(Vec2::ZERO, threat - at, false);
    }
    let tiles = sim.tiles();
    let goal = tiles
        .nearest_free(post)
        .unwrap_or_else(|| tiles.tile_at(post));
    let step = FlowField::toward(tiles, &[goal])
        .next_waypoint(tiles, at)
        .map_or(post - at, |waypoint| waypoint - at)
        .normalize_or_zero();
    walking(step, threat - at, false)
}

/// 곁(`SWING_REACH`타일 안)에 졸개가 있으면 가장 가까운 졸개를 제자리에서 베는 입력입니다.
fn swing_at_minion_beside(sim: &Sim, at: Vec2) -> Option<PlayerInput> {
    let minion = sim
        .monsters()
        .filter(|monster| is_minion(monster))
        .map(|monster| monster.pos)
        .filter(|pos| pos.distance(at) < SWING_REACH)
        .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))?;
    Some(walking(Vec2::ZERO, minion - at, true))
}

/// 보스가 아닌 몬스터입니다. 보스의 밤에는 보스가 부른 졸개이고, 보스가 없는 밤의 측정에서는 밤의 무리입니다.
fn is_minion(monster: &Monster) -> bool {
    monster.kind != MonsterKind::Boss
}

/// 쓰러진 플레이어가 `at`에 있을 때 가장 가까이 있는 서 있는 플레이어입니다. 부활시키러 가는 사람은 이 한 명뿐입니다.
fn nearest_helper(sim: &Sim, at: Vec2) -> Option<PlayerId> {
    sim.players()
        .filter(|player| player.is_alive())
        .min_by(|a, b| (a.pos - at).length().total_cmp(&(b.pos - at).length()))
        .map(|player| player.id)
}

/// 모의 전투의 조건입니다.
#[derive(Clone, Copy)]
struct Setup {
    party: u32,
    weapon: WeaponKind,
    /// 마을에 서 있는 감시탑의 수입니다.
    towers: u32,
    /// 마지막 한 명이 마차를 지키는지입니다. 아니면 모두가 보스를 때립니다.
    guarded: bool,
}

impl Setup {
    /// 기본 마을(감시탑 15개)에서 `party`명이 `weapon`을 듭니다. 방에 둘 이상이 있으면 한 명이 마차를 지킵니다.
    fn village(party: u32, weapon: WeaponKind) -> Setup {
        Setup {
            party,
            weapon,
            towers: 15,
            guarded: party >= 2,
        }
    }
}

/// `setup`의 조건으로 마지막 밤의 보스와 싸웁니다. 5분 안에 끝나지 않으면 거기서 멈춥니다.
fn raid(setup: Setup) -> Raid {
    let Setup {
        party,
        weapon,
        towers,
        guarded,
    } = setup;
    let mut s = Scenario::default_game(|t| {
        t.nights.finale = Tuning::default().nights.finale;
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
        t.loot.chests_per_day = 0;
        t.village.start_towers = t.spread_tower_sites(towers);
    });
    let spawns = s.sim.tuning().map.player_spawns.clone();
    let mut bots: Vec<Bot> = (0..party)
        .map(|index| {
            let spot = spawns[index as usize % spawns.len()];
            let id = s.player_at(spot.x, spot.y);
            s.give_weapon(id, weapon);
            Bot {
                id,
                role: if guarded && index == party - 1 {
                    Role::Guard
                } else {
                    Role::Fighter
                },
                seen: VecDeque::new(),
            }
        })
        .collect();
    s.sim.set_phase(Phase::Night {
        day: 3,
        party,
        step: NightStep::Warning { ticks_left: 1 },
    });

    let mut appeared = None;
    let limit = s.ticks(300.0);
    for tick in 0..limit + s.ticks(10.0) {
        for bot in &mut bots {
            let input = bot.input(&s.sim);
            s.hold(bot.id, input);
        }
        s.run_ticks(1);
        if appeared.is_none() && s.sim.boss().is_some() {
            appeared = Some(tick);
        }
        if s.sim.debrief().is_some() {
            break;
        }
        if appeared.is_some_and(|at| tick - at >= limit) {
            break;
        }
    }
    let appeared = appeared.expect("the boss came");
    let ended = s.sim.tick() as u32;
    Raid {
        outcome: s.sim.debrief().map(|debrief| debrief.outcome()),
        secs: (ended - appeared) as f32 * s.sim.tuning().dt(),
        boss_hp_left: s
            .sim
            .boss()
            .and_then(|boss| s.monster(boss.id))
            .map_or(0, |body| body.hp),
        wagon_hp: s.sim.wagon().hp,
        players_standing: s.sim.players().filter(|player| player.is_alive()).count(),
        towers_standing: s.sim.towers().filter(|tower| tower.is_standing()).count(),
    }
}

fn describe(setup: Setup, result: &Raid) -> String {
    format!(
        "{} x {:?}, {} towers, {}: {:?} after {:.0}s, boss HP left {}, wagon {}, {} players standing, {} towers",
        setup.party,
        setup.weapon,
        setup.towers,
        if setup.guarded {
            "one guards the wagon"
        } else {
            "all strike the boss"
        },
        result.outcome,
        result.secs,
        result.boss_hp_left,
        result.wagon_hp,
        result.players_standing,
        result.towers_standing
    )
}

#[test]
fn a_mock_raid_of_each_party_size_and_weapon_against_the_default_numbers() {
    // 단순한 입력의 결과를 인원과 무기마다 출력합니다. 이 표는 수치를 맞출 때 읽고, 시간은 단언하지 않습니다.
    for weapon in [WeaponKind::Sword, WeaponKind::Bow, WeaponKind::Spear] {
        for party in 1..=4 {
            let setup = Setup::village(party, weapon);
            println!("{}", describe(setup, &raid(setup)));
        }
    }
}

#[test]
fn a_mock_raid_with_and_without_a_guard_and_with_fewer_towers_is_printed_for_comparison() {
    // 마차를 지키는 한 명이 결과를 바꾸는지를 인원과 무기와 감시탑의 수마다 봅니다. 감시탑이 적을수록 졸개가 마차에 닿기 쉽습니다. 출력만 하고 단언하지 않습니다.
    // 방에 한 명뿐이면 그 사람이 보스를 때려야 하므로 지키는 쪽은 없습니다.
    for weapon in [WeaponKind::Sword, WeaponKind::Bow, WeaponKind::Spear] {
        for party in 1..=4 {
            for towers in [3, 5, 10, 15, 25] {
                for guarded in [true, false] {
                    if party == 1 && guarded {
                        continue;
                    }
                    let setup = Setup {
                        towers,
                        guarded,
                        ..Setup::village(party, weapon)
                    };
                    println!("{}", describe(setup, &raid(setup)));
                }
            }
        }
    }
}

#[test]
fn four_swordsmen_and_one_swordsman_win_the_mock_raid_against_the_default_numbers() {
    for party in [4, 1] {
        let setup = Setup::village(party, WeaponKind::Sword);
        let result = raid(setup);
        assert_eq!(
            result.outcome,
            Some(Outcome::Victory),
            "{}",
            describe(setup, &result)
        );
    }
}

#[test]
fn a_guard_at_the_wagon_pays_off_with_fifteen_towers_and_a_lone_player_still_wins_there() {
    // 감시탑이 돌격병을 모두 잡아 주지 못해야 마차를 지키는 일이 쓸모가 있습니다. 둘 다 이기면 지키는 쪽이 마차의 HP를 `GUARD_WORTH` 이상 더 남기고,
    // 지키지 않는 쪽이 지면 지키는 쪽이 이기는 것도 지키는 일이 쓸모 있다는 뜻입니다.
    let guarded = Setup::village(4, WeaponKind::Sword);
    let bare = Setup {
        guarded: false,
        ..guarded
    };
    let (with_guard, without_guard) = (raid(guarded), raid(bare));
    let pays_off = match (with_guard.outcome, without_guard.outcome) {
        (Some(Outcome::Victory), Some(Outcome::Victory)) => {
            with_guard.wagon_hp >= without_guard.wagon_hp + GUARD_WORTH
        }
        (Some(Outcome::Victory), _) => true,
        _ => false,
    };
    assert!(
        pays_off,
        "{}\n{}",
        describe(guarded, &with_guard),
        describe(bare, &without_guard)
    );

    // 방에 한 명뿐이면 마차를 지킬 사람이 없지만, 감시탑 15개가 돌격병을 충분히 막아 주어서 그 한 명이 보스를 잡고 이깁니다.
    let lone = Setup::village(1, WeaponKind::Sword);
    let result = raid(lone);
    assert_eq!(
        result.outcome,
        Some(Outcome::Victory),
        "{}",
        describe(lone, &result)
    );
}

/// 보스가 없는 밤 하나가 끝났을 때의 모습입니다.
#[derive(Debug)]
struct Night {
    /// 마차가 부서지거나 모두가 쓰러지지 않고 밤의 무리를 모두 처치했으면 `Victory`입니다. 제한 시간 안에 끝나지 않았으면 `None`입니다.
    outcome: Option<Outcome>,
    secs: f32,
    wagon_hp: u32,
    players_standing: usize,
    towers_standing: usize,
}

/// 기본 마을에 감시탑 `towers`개가 서 있고 `party`명이 있는 방에서 `day`번째 밤의 무리를 처음부터 끝까지 겪습니다. 보스는 없습니다.
/// `defend`이면 모두가 검을 들고 마차를 지키고(`guard_the_wagon`), 아니면 모두가 맵 구석에 서 있어서 감시탑만 싸웁니다.
/// 쓰러진 플레이어를 부활시키지 않고 포탑을 짓거나 수리하지도 않습니다. 돌격병이 받는 포탑의 피해는 `runner_tower_percent`입니다.
fn night(day: u32, party: u32, towers: u32, defend: bool, runner_tower_percent: u32) -> Night {
    let mut s = Scenario::default_game(|t| {
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
        t.loot.chests_per_day = 0;
        t.village.start_towers = t.spread_tower_sites(towers);
        t.monsters.runner.resist.tower_damage_percent = runner_tower_percent;
    });
    let spawns = s.sim.tuning().map.player_spawns.clone();
    let players: Vec<PlayerId> = (0..party)
        .map(|index| {
            let spot = if defend {
                spawns[index as usize % spawns.len()]
            } else {
                Vec2::new(2.0 + index as f32, 2.0)
            };
            s.player_at(spot.x, spot.y)
        })
        .collect();
    s.sim.set_phase(Phase::Night {
        day,
        party,
        step: NightStep::Waiting {
            next_wave: 1,
            ticks_left: 1,
        },
    });

    let in_this_night =
        |sim: &Sim| matches!(sim.phase(), Some(Phase::Night { day: now, .. }) if now == day);
    for _ in 0..s.ticks(300.0) {
        for &id in &players {
            let input = match s.sim.player(id).filter(|player| player.is_alive()) {
                Some(player) if defend => guard_the_wagon(&s.sim, player.pos, false),
                _ => PlayerInput::default(),
            };
            s.hold(id, input);
        }
        s.run_ticks(1);
        if s.sim.debrief().is_some() || !in_this_night(&s.sim) {
            break;
        }
    }
    let outcome = match s.sim.debrief() {
        Some(debrief) => Some(debrief.outcome()),
        None if in_this_night(&s.sim) => None,
        None => Some(Outcome::Victory),
    };
    Night {
        outcome,
        secs: s.sim.tick() as f32 * s.sim.tuning().dt(),
        wagon_hp: s.sim.wagon().hp,
        players_standing: s.sim.players().filter(|player| player.is_alive()).count(),
        towers_standing: s.sim.towers().filter(|tower| tower.is_standing()).count(),
    }
}

#[test]
fn the_second_and_third_night_with_towers_alone_and_with_players_at_the_wagon_are_printed_for_comparison()
 {
    // 돌격병이 포탑의 피해를 모두 받을 때와 기본 수치일 때를 견주어서, 돌격병이 밤을 얼마나 어렵게 하는지를 봅니다. 출력만 하고 단언하지 않습니다.
    let by_default = Tuning::default()
        .monsters
        .runner
        .resist
        .tower_damage_percent;
    for day in [2, 3] {
        for party in [1, 4] {
            for defend in [false, true] {
                for percent in [100, by_default] {
                    let result = night(day, party, 15, defend, percent);
                    println!(
                        "night {day}, {party} players, 15 towers, {}, runners take {percent}% of a tower hit: {:?} after {:.0}s, wagon {}, {} players standing, {} towers",
                        if defend {
                            "players at the wagon"
                        } else {
                            "towers alone"
                        },
                        result.outcome,
                        result.secs,
                        result.wagon_hp,
                        result.players_standing,
                        result.towers_standing
                    );
                }
            }
        }
    }
}
