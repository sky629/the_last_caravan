//! 포위 측정: 셋째 밤에 파괴병이 감시탑을 얼마나 부수는지, 파괴병을 막는 사람이 한 명 있으면 얼마나 달라지는지를 규칙을 직접 돌려서 잽니다.
//!
//! 모든 플레이어는 검을 들고, 기본 마을에 감시탑이 `towers`개 서 있는 방에서 셋째 밤의 무리를 처음부터 끝까지 겪습니다. 보스는 없습니다.
//! 플레이어가 하는 일은 둘 가운데 하나입니다.
//! - 마차를 지킵니다(`guard_the_wagon`): 곁에 온 몬스터를 제자리에서 베고, 몬스터가 마차에 가장 가까이 온 쪽의 마차 곁으로 길을 따라 걸어갑니다.
//! - 파괴병을 막습니다(`block_the_sappers`): 마을 안의 가장 가까운 파괴병에게 길을 따라 걸어가서 곁에 닿으면 제자리에서 벱니다. 마을 안에 파괴병이 없으면 마차를 지킵니다.
//!
//! 「아무도 막지 않음」은 모두가 마차를 지키는 방이고, 「한 명이 막음」은 그 가운데 한 명이 파괴병을 막는 방입니다. 방의 인원은 같습니다.
//! 몬스터는 파괴병만이 아니라 기본 몬스터도 포탑을 때리므로, 부서진 감시탑의 수에는 기본 몬스터가 부순 것도 들어 있습니다.
//! 그래서 파괴병이 부순 몫을 보려고, 같은 방에서 파괴병을 모두 기본 몬스터로 바꾼 밤(`without_sappers`)의 수를 함께 잽니다.
//! 입력은 `Sim`의 공개 입력(`PlayerInput`)뿐이고, 입력을 만들 때 읽는 것은 화면이 알려 주는 것(몬스터의 종류와 위치, 플레이어 자기 자리)과 길을 찾으려고 읽는 타일 지도뿐입니다.
//! 쓰러진 플레이어를 부활시키지 않고 포탑을 짓거나 수리하지도 않습니다. 감시탑은 모두 가득 찬 HP로 시작하고 밤 동안 부서진 것은 다시 서지 않습니다.
//!
//! 숫자는 출력합니다. 단언하는 것은 파괴병을 막는 입력이 Tick마다 막혀 서 있지 않고 실제로 걷고 베는 것뿐입니다.
//! 「아무도 막지 않으면 감시탑의 절반 이상이 부서지고, 한 명이 막으면 부서지는 감시탑이 더 적다」가 성립하는지는 출력한 표를 읽어서 판단합니다.
//!
//! 출력을 보려면 다음 명령을 실행합니다.
//!
//! ```text
//! cargo test --release -p caravan_rules siege -- --nocapture --test-threads=1
//! ```

use std::collections::BTreeMap;

use super::raid::{SWING_REACH, guard_the_wagon, walk_toward, walking};
use super::support::Scenario;
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::{MonsterId, PlayerId};
use crate::input::PlayerInput;
use crate::kind::MonsterKind;
use crate::math::Vec2;
use crate::player::Player;
use crate::sim::Sim;
use crate::tuning::{Tuning, WaveScaling};

/// 마차의 가장자리에서 이 거리(타일) 안에 들어온 파괴병만 막으러 갑니다. 마을 밖으로 나가면 몬스터 무리 한가운데에서 쓰러지기 때문입니다.
const VILLAGE_REACH: f32 = 14.0;

/// 파괴병까지 이 거리(타일)보다 멀면 달리기 게이지가 남아 있는 동안 달려서 갑니다.
const RUN_BEYOND: f32 = 4.0;

/// 달리기 게이지가 이 비율(%) 이상 남아 있으면 달립니다.
const RUN_FROM: u8 = 5;

/// 걸으려는 입력이 한 Tick에 걷는 거리(걷는 속도와 한 Tick의 길이의 곱)의 이 비율보다 적게 움직이면, 입력이 막혀 서 있는 Tick으로 셉니다.
const BLOCKED_BELOW: f32 = 0.1;

/// 감시탑을 세운 모양입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    /// 마차에 가까운 자리부터 세웁니다(`Tuning::spread_tower_sites`). 모의 전투와 모의 원정이 짓는 모양이고, 마차를 지키는 사람이 감시탑 곁에도 서 있습니다.
    NearTheWagon,
    /// 마을의 모든 자리 가운데 같은 간격으로 골라서 마을 전체에 세웁니다. 마차를 지키는 사람이 닿지 못하는 감시탑이 생깁니다.
    AcrossTheVillage,
}

impl Layout {
    /// 이 모양으로 감시탑 `towers`개를 세울 자리입니다.
    fn sites(self, t: &Tuning, towers: u32) -> Vec<crate::world::Tile> {
        match self {
            Layout::NearTheWagon => t.spread_tower_sites(towers),
            Layout::AcrossTheVillage => {
                let all = t.spread_tower_sites(u32::MAX);
                let step = (all.len() / towers.max(1) as usize).max(1);
                all.into_iter()
                    .step_by(step)
                    .take(towers as usize)
                    .collect()
            }
        }
    }
}

/// 셋째 밤을 겪는 조건입니다.
#[derive(Clone, Copy)]
struct Siege {
    party: u32,
    /// 마을에 서 있는 감시탑의 수입니다.
    towers: u32,
    layout: Layout,
    /// 파괴병을 막는 사람의 수입니다. 나머지 플레이어는 마차를 지킵니다.
    blockers: u32,
}

/// 셋째 밤 하나가 끝났을 때의 모습입니다.
#[derive(Debug)]
struct Night {
    /// 마차가 부서지거나 모두가 쓰러지지 않고 밤의 무리를 모두 처치했으면 `Victory`입니다. 제한 시간 안에 끝나지 않았으면 `None`입니다.
    outcome: Option<Outcome>,
    secs: f32,
    wagon_hp: u32,
    players_standing: usize,
    towers_at_start: usize,
    towers_standing: usize,
    /// 파괴병이 공격한 횟수입니다. 감시탑을 때린 공격이 대부분이고, 마차를 때린 공격도 섞여 있습니다.
    sapper_hits: u32,
    /// 파괴병을 막는 사람이 걷고 있던 Tick의 수와 베고 있던 Tick의 수입니다.
    blocker_walking_ticks: u32,
    blocker_swinging_ticks: u32,
    /// 파괴병을 막는 사람이 걸으려고 했는데 움직이지 못한 Tick의 수입니다.
    blocker_blocked_ticks: u32,
}

impl Night {
    fn towers_lost(&self) -> usize {
        self.towers_at_start - self.towers_standing
    }
}

/// 밤의 무리의 파괴병을 모두 기본 몬스터로 바꿉니다. 무리의 크기는 그대로입니다.
fn without_sappers(t: &mut Tuning) {
    let packs = t
        .nights
        .plans
        .iter_mut()
        .flat_map(|night| &mut night.waves)
        .flat_map(|wave| &mut wave.packs);
    for pack in packs.filter(|pack| pack.kind == MonsterKind::Sapper) {
        pack.kind = MonsterKind::Grunt;
    }
}

/// 마을 안의 가장 가까운 파괴병에게 길을 따라 걸어가고(멀면 달립니다), 곁에 닿으면 제자리에서 베는 입력입니다. 마을 안에 파괴병이 없으면 `None`입니다.
fn go_for_a_sapper(sim: &Sim, me: &Player) -> Option<PlayerInput> {
    let wagon = sim.wagon().rect;
    let sapper = sim
        .monsters()
        .filter(|monster| monster.kind == MonsterKind::Sapper)
        .filter(|monster| wagon.distance_to(monster.pos) <= VILLAGE_REACH)
        .min_by(|a, b| a.pos.distance(me.pos).total_cmp(&b.pos.distance(me.pos)))?;
    let to = sapper.pos - me.pos;
    if to.length() < SWING_REACH {
        return Some(walking(Vec2::ZERO, to, true));
    }
    let step = walk_toward(sim, me.pos, sapper.pos);
    Some(PlayerInput {
        sprint: to.length() > RUN_BEYOND && me.stamina_percent(sim.tuning()) >= RUN_FROM,
        ..walking(step, to, false)
    })
}

/// 파괴병을 막는 사람의 입력입니다. 마을 안에 파괴병이 있으면 그 파괴병에게 가고(`go_for_a_sapper`), 없으면 마차를 지키는 사람과 같이 곁에 온 몬스터를 베고 마차를 지킵니다.
fn block_the_sappers(sim: &Sim, me: &Player) -> PlayerInput {
    go_for_a_sapper(sim, me).unwrap_or_else(|| guard_the_wagon(sim, me.pos, false))
}

/// `siege`의 조건으로 기본 마을에서 셋째 밤의 무리를 처음부터 끝까지 겪습니다. `adjust`가 수치를 더 바꿉니다. 5분 안에 끝나지 않으면 거기서 멈춥니다.
fn third_night(siege: Siege, adjust: impl FnOnce(&mut Tuning)) -> Night {
    let Siege {
        party,
        towers,
        layout,
        blockers,
    } = siege;
    let mut s = Scenario::default_game(|t| {
        t.nights.scaling = WaveScaling::ByParty { full_party: 4 };
        t.guards.per_node = 0;
        t.guards.per_chest = 0;
        t.loot.chests_per_day = 0;
        t.village.start_towers = layout.sites(t, towers);
        adjust(t);
    });
    let spawns = s.sim.tuning().map.player_spawns.clone();
    let players: Vec<PlayerId> = (0..party)
        .map(|index| {
            let spot = spawns[index as usize % spawns.len()];
            s.player_at(spot.x, spot.y)
        })
        .collect();
    let blocking = &players[..blockers as usize];
    let towers_at_start = s.sim.towers().count();
    s.sim.set_phase(Phase::Night {
        day: 3,
        party,
        step: NightStep::Waiting {
            next_wave: 1,
            ticks_left: 1,
        },
    });

    let in_the_third_night = |sim: &Sim| matches!(sim.phase(), Some(Phase::Night { day: 3, .. }));
    let full_step = s.sim.tuning().player.speed * s.sim.tuning().dt();
    let mut last_attacks: BTreeMap<MonsterId, Option<u64>> = BTreeMap::new();
    let (mut sapper_hits, mut walking_ticks, mut swinging_ticks, mut blocked_ticks) = (0, 0, 0, 0);
    for _ in 0..s.ticks(300.0) {
        let mut walkers = Vec::new();
        for &id in &players {
            let Some(player) = s.sim.player(id).filter(|player| player.is_alive()) else {
                s.hold(id, PlayerInput::default());
                continue;
            };
            let input = if blocking.contains(&id) {
                let input = block_the_sappers(&s.sim, player);
                if input.attack {
                    swinging_ticks += 1;
                } else if input.move_dir.get() != Vec2::ZERO {
                    walking_ticks += 1;
                    walkers.push((id, player.pos));
                }
                input
            } else {
                guard_the_wagon(&s.sim, player.pos, false)
            };
            s.hold(id, input);
        }
        s.run_ticks(1);
        for (id, from) in walkers {
            let moved = s
                .sim
                .player(id)
                .map_or(f32::INFINITY, |player| player.pos.distance(from));
            if moved < full_step * BLOCKED_BELOW {
                blocked_ticks += 1;
            }
        }
        // 파괴병의 `last_attack_tick`이 바뀐 Tick이 공격 한 번입니다.
        for sapper in s
            .sim
            .monsters()
            .filter(|monster| monster.kind == MonsterKind::Sapper)
        {
            let seen = last_attacks.entry(sapper.id).or_insert(None);
            if *seen != sapper.last_attack_tick {
                *seen = sapper.last_attack_tick;
                sapper_hits += 1;
            }
        }
        if s.sim.debrief().is_some() || !in_the_third_night(&s.sim) {
            break;
        }
    }
    let outcome = match s.sim.debrief() {
        Some(debrief) => Some(debrief.outcome()),
        None if in_the_third_night(&s.sim) => None,
        None => Some(Outcome::Victory),
    };
    Night {
        outcome,
        secs: s.sim.tick() as f32 * s.sim.tuning().dt(),
        wagon_hp: s.sim.wagon().hp,
        players_standing: s.sim.players().filter(|player| player.is_alive()).count(),
        towers_at_start,
        towers_standing: s.sim.towers().filter(|tower| tower.is_standing()).count(),
        sapper_hits,
        blocker_walking_ticks: walking_ticks,
        blocker_swinging_ticks: swinging_ticks,
        blocker_blocked_ticks: blocked_ticks,
    }
}

fn describe(siege: Siege, night: &Night) -> String {
    format!(
        "{:?}, {} players, {} block the sappers: {:?} after {:.0}s, {} of {} towers lost, {} sapper hits, wagon {}, {} players standing; the blocker walked {} ticks, swung {} ticks and was blocked {} ticks",
        siege.layout,
        siege.party,
        siege.blockers,
        night.outcome,
        night.secs,
        night.towers_lost(),
        night.towers_at_start,
        night.sapper_hits,
        night.wagon_hp,
        night.players_standing,
        night.blocker_walking_ticks,
        night.blocker_swinging_ticks,
        night.blocker_blocked_ticks
    )
}

#[test]
fn the_third_night_is_printed_for_each_layout_party_and_tower_count_with_nobody_and_one_player_blocking_the_sappers()
 {
    // 감시탑을 세운 모양, 인원 4명과 2명, 감시탑 10개와 15개와 25개마다 세 번 겪습니다. 파괴병이 없는 밤, 파괴병이 있고 아무도 막지 않는 밤, 한 명이 막는 밤입니다.
    for layout in [Layout::NearTheWagon, Layout::AcrossTheVillage] {
        for party in [4, 2] {
            for towers in [10, 15, 25] {
                let siege = |blockers| Siege {
                    party,
                    towers,
                    layout,
                    blockers,
                };
                let control = third_night(siege(0), without_sappers);
                let nobody = third_night(siege(0), |_| {});
                let one = third_night(siege(1), |_| {});
                println!(
                    "{layout:?}, {party} players, {towers} towers: towers lost {} without sappers, {} with sappers and nobody blocking them, {} with one blocking them; sapper hits {} and {}; {:?} and {:?}",
                    control.towers_lost(),
                    nobody.towers_lost(),
                    one.towers_lost(),
                    nobody.sapper_hits,
                    one.sapper_hits,
                    nobody.outcome,
                    one.outcome
                );
            }
        }
    }
}

#[test]
fn the_player_who_blocks_the_sappers_is_never_stuck_and_does_walk_and_swing() {
    // 입력이 막혀 서 있으면 파괴병을 막는 효과가 보이지 않으므로, 막는 사람이 Tick마다 걸으려는 만큼 움직이는지 확인합니다.
    for layout in [Layout::NearTheWagon, Layout::AcrossTheVillage] {
        for party in [4, 2] {
            let siege = Siege {
                party,
                towers: 15,
                layout,
                blockers: 1,
            };
            let night = third_night(siege, |_| {});
            assert_eq!(
                night.blocker_blocked_ticks,
                0,
                "{}",
                describe(siege, &night)
            );
            assert!(
                night.blocker_walking_ticks > 0 && night.blocker_swinging_ticks > 0,
                "{}",
                describe(siege, &night)
            );
        }
    }
}
