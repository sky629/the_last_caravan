//! 모의 전투: 보스와 싸우는 단순한 입력으로 규칙을 직접 돌려서 보스를 잡는 데 걸리는 시간을 잽니다.
//!
//! 입력을 만드는 쪽(`Bot`)은 화면이 알려 주는 것만 읽습니다. 보스의 몸과 예고된 영역, 쓰러진 동료, 자기 자리입니다.
//! 예고를 읽는 시간이 `REACTION_TICKS`만큼 늦어서 반응이 느린 사람을 흉내 내고, 달리지 않고 걷기만 합니다.
//! 사람의 판단(포탑 짓기, 수리, 보스를 마차에서 끌어내기)은 넣지 않았으므로 이 시간은 사람이 하는 전투의 대표값이 아니라
//! 「단순한 입력으로도 잡을 수 있는가」와 「수치가 어느 크기인가」를 보는 자로 쓰입니다. 시간은 출력만 하고 단언하지 않습니다.
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
use crate::kind::WeaponKind;
use crate::math::Vec2;
use crate::sim::Sim;
use crate::telegraph::Zone;
use crate::tuning::{Delivery, Tuning, WaveScaling};

/// 예고된 영역을 읽는 데 걸리는 Tick입니다. 0.3초이고, 입력이 늦는 정도(0.1초에서 0.15초)보다 느립니다.
const REACTION_TICKS: usize = 6;

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

struct Bot {
    id: PlayerId,
    /// 예고된 영역의 기록입니다. 앞에 있는 것이 `REACTION_TICKS` Tick 전에 보인 영역이고 그것을 보고 움직입니다.
    seen: VecDeque<Option<Zone>>,
}

fn zone_now(sim: &Sim) -> Option<Zone> {
    match &sim.boss()?.act {
        Act::Slamming(telegraph) => Some(telegraph.zone),
        Act::Walking { .. } | Act::Winding { .. } | Act::Charging(_) | Act::Roaring { .. } => None,
    }
}

/// 여유 `margin`타일을 두고 `zone` 안에 있는지입니다.
fn is_inside(zone: &Zone, at: Vec2, margin: f32) -> bool {
    match *zone {
        Zone::Circle { center, radius } => at.distance(center) < radius + margin,
        Zone::Lane { .. } => false,
    }
}

/// `weapon`으로 보스의 중심에서 몇 타일 떨어져 서면 맞는지입니다. 가까이서 치는 무기는 맞는 거리의 끝에서 조금 안쪽입니다.
fn stand_off(t: &Tuning, weapon: WeaponKind) -> f32 {
    match t.weapons.get(weapon).delivery {
        Delivery::Arc(arc) => t.player.radius + arc.reach + t.monsters.boss.hit_radius() - 0.35,
        Delivery::Projectile(_) | Delivery::Hitscan(_) => 5.0,
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
        if delayed.is_some_and(|zone| is_inside(&zone, player.pos, 0.5)) {
            // 보스에게서 멀어지는 쪽으로 벗어납니다. 영역은 보스 곁의 플레이어를 중심으로 하므로 어느 쪽으로 걸어도 벗어나지만, 보스 쪽은 보스에게 닿습니다.
            let away = (player.pos - boss).normalize_or_zero();
            return PlayerInput {
                move_dir: MoveDir::new(away.x, away.y),
                aim: AimDir::new(away.x, away.y),
                ..PlayerInput::default()
            };
        }
        // 영역 밖으로 나온 뒤에는 영역이 사라질 때까지 그 자리에서 기다립니다. 걸어 들어가면 터질 때 맞습니다.
        let waits_for_the_strike = delayed.is_some();

        let downed_ally = sim
            .players()
            .filter(|other| other.id != self.id && other.is_downed())
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
        let stand = stand_off(t, player.loadout.held());
        let to = boss - player.pos;
        let distance = to.length();
        let step = if waits_for_the_strike {
            Vec2::ZERO
        } else if distance > stand + 0.3 {
            to.normalize_or_zero()
        } else if distance < stand - 0.6 && stand > 1.5 {
            to.normalize_or_zero() * -1.0
        } else {
            Vec2::ZERO
        };
        PlayerInput {
            move_dir: MoveDir::new(step.x, step.y),
            aim: AimDir::new(to.x, to.y),
            attack: true,
            ..PlayerInput::default()
        }
    }
}

/// `party`명이 `weapon`을 들고 마을(감시탑 `towers`개)에서 마지막 밤의 보스와 싸웁니다. 5분 안에 끝나지 않으면 거기서 멈춥니다.
fn raid(party: u32, weapon: WeaponKind, towers: u32) -> Raid {
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

fn describe(party: u32, weapon: WeaponKind, towers: u32, result: &Raid) -> String {
    format!(
        "{party} x {weapon:?}, {towers} towers: {:?} after {:.0}s, boss HP left {}, wagon {}, {} players standing, {} towers",
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
            let result = raid(party, weapon, 15);
            println!("{}", describe(party, weapon, 15, &result));
        }
    }
}
