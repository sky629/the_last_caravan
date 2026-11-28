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
//! 한 칸(인원, 감시탑의 수, 감시탑을 세운 모양)은 `RUNS`번 돌립니다. 실행마다 플레이어가 서는 자리, 움직이기 시작하는 때, 걷는 방향이 조금씩 달라서(`Siege::run`)
//! 한 번의 우연으로 결론이 나지 않습니다. 표의 값은 실행들의 최솟값, 중앙값, 최댓값입니다.
//!
//! 단언하는 것은 둘입니다. 파괴병을 막는 입력이 Tick마다 막혀 서 있지 않고 실제로 걷고 베는 것, 그리고 기본 수치에서 모의 원정이 셋째 밤에 만나는 감시탑의 수
//! (4명은 35개, 3명은 28개, 2명은 22개)를 마차 가까이 세운 방은 아무도 막지 않으면 한 명이 막을 때보다 감시탑이 더 많이 부서지는 것입니다. 나머지 표는 출력만 하고 `#[ignore]`입니다.
//! 한 명뿐인 방에는 파괴병이 나오지 않으므로(`Pack::joins`) 막을 파괴병이 없고, 이 측정은 2명 이상의 방만 돌립니다.
//!
//! ```text
//! cargo test --release -p caravan_rules siege -- --ignored --nocapture --test-threads=1
//! ```

use super::raid::{SWING_REACH, guard_the_wagon, walk_toward, walking};
use super::support::Scenario;
use crate::expedition::{NightStep, Outcome, Phase};
use crate::ids::PlayerId;
use crate::input::{MoveDir, PlayerInput};
use crate::kind::MonsterKind;
use crate::math::Vec2;
use crate::player::Player;
use crate::sim::Sim;
use crate::tuning::{Pack, Tuning, WaveScaling};
use crate::world::Tile;

/// 마차의 가장자리에서 이 거리(타일) 안에 들어온 파괴병만 막으러 갑니다. 마을 밖으로 나가면 몬스터 무리 한가운데에서 쓰러지기 때문입니다.
const VILLAGE_REACH: f32 = 14.0;

/// 파괴병까지 이 거리(타일)보다 멀면 달리기 게이지가 남아 있는 동안 달려서 갑니다.
const RUN_BEYOND: f32 = 4.0;

/// 달리기 게이지가 이 비율(%) 이상 남아 있으면 달립니다.
const RUN_FROM: u8 = 5;

/// 걸으려는 입력이 한 Tick에 걷는 거리(걷는 속도와 한 Tick의 길이의 곱)의 이 비율보다 적게 움직이면, 입력이 막혀 서 있는 Tick으로 셉니다.
const BLOCKED_BELOW: f32 = 0.1;

/// 한 칸을 돌리는 실행의 수입니다.
const RUNS: u32 = 7;

/// 걷는 방향을 돌리는 가장 큰 각도(라디안)입니다. 0.4는 약 23도입니다.
const STEER_NOISE: f32 = 0.4;

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
    fn sites(self, t: &Tuning, towers: u32) -> Vec<Tile> {
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
    /// 같은 조건을 몇 번째로 돌리는지입니다(`RUNS` 미만). 같은 번호이면 언제나 같은 밤이고, 번호마다 사람의 움직임이 조금씩 어긋납니다.
    /// 플레이어가 서는 자리를 사방 4타일 안에서 옮기고, 밤이 시작된 뒤 첫 입력까지 0에서 59 Tick을 기다리게 하고,
    /// 0번째를 뺀 실행에서는 걷는 방향을 8 Tick마다 `STEER_NOISE` 안에서 돌립니다. 모든 조건에 같은 식으로 더하므로 어느 한쪽에 유리하지 않습니다.
    run: u32,
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

    fn defeated(&self) -> bool {
        self.outcome != Some(Outcome::Victory)
    }
}

/// 밤의 무리의 파괴병을 모두 어느 인원의 방에서나 나오는 기본 몬스터로 바꿉니다. 무리의 크기는 그대로입니다.
fn without_sappers(t: &mut Tuning) {
    let packs = t
        .nights
        .plans
        .iter_mut()
        .flat_map(|night| &mut night.waves)
        .flat_map(|wave| &mut wave.packs);
    for pack in packs.filter(|pack| pack.kind == MonsterKind::Sapper) {
        *pack = Pack::always(MonsterKind::Grunt, pack.count);
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

/// 걷는 방향을 `STEER_NOISE` 안에서 돌립니다. 각도는 8 Tick마다 플레이어마다 새로 정하고, 같은 (`run`, 플레이어, 시간)이면 언제나 같습니다.
/// 사람이 정확하게 조준해서 걷지 못하는 것을 흉내 냅니다.
fn steer_noise(input: PlayerInput, run: u32, index: u32, tick: u64) -> PlayerInput {
    let step = input.move_dir.get();
    if step == Vec2::ZERO {
        return input;
    }
    let mut hash = u64::from(run) << 40 ^ u64::from(index) << 32 ^ (tick / 8);
    hash = hash.wrapping_add(0x9E37_79B9_7F4A_7C15);
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    hash ^= hash >> 31;
    let angle = ((hash % 2001) as f32 / 1000.0 - 1.0) * STEER_NOISE;
    let (sin, cos) = angle.sin_cos();
    let turned = Vec2::new(step.x * cos - step.y * sin, step.x * sin + step.y * cos);
    PlayerInput {
        move_dir: MoveDir::new(turned.x, turned.y),
        ..input
    }
}

/// `siege`의 조건으로 기본 마을에서 셋째 밤의 무리를 처음부터 끝까지 겪습니다. `adjust`가 수치를 더 바꿉니다. 5분 안에 끝나지 않으면 거기서 멈춥니다.
fn third_night(siege: Siege, adjust: &dyn Fn(&mut Tuning)) -> Night {
    let Siege {
        party,
        towers,
        layout,
        blockers,
        run,
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
            let dx = ((run * 5 + index * 3) % 9) as f32 - 4.0;
            let dy = ((run * 3 + index * 7) % 9) as f32 - 4.0;
            let wanted = Vec2::new(spot.x + dx, spot.y + dy);
            let at = s
                .sim
                .tiles()
                .nearest_free(wanted)
                .map_or(wanted, |tile| tile.center());
            s.player_at(at.x, at.y)
        })
        .collect();
    let lags: Vec<u64> = (0..party)
        .map(|index| u64::from(((run + 1) * 7 + index * 13) % 60))
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
    let (mut walking_ticks, mut swinging_ticks, mut blocked_ticks) = (0, 0, 0);
    for _ in 0..s.ticks(300.0) {
        let mut walkers = Vec::new();
        for (index, &id) in players.iter().enumerate() {
            let Some(player) = s.sim.player(id).filter(|player| player.is_alive()) else {
                s.hold(id, PlayerInput::default());
                continue;
            };
            if s.sim.tick() < lags[index] {
                s.hold(id, PlayerInput::default());
                continue;
            }
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
            let input = if run > 0 {
                steer_noise(input, run, index as u32, s.sim.tick())
            } else {
                input
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
        blocker_walking_ticks: walking_ticks,
        blocker_swinging_ticks: swinging_ticks,
        blocker_blocked_ticks: blocked_ticks,
    }
}

/// `party`명이 `towers`개의 감시탑을 `layout`으로 세운 방에서 `blockers`명이 파괴병을 막는 셋째 밤을 `RUNS`번 돌립니다. 실행의 번호가 곧 `Siege::run`입니다.
fn runs(
    party: u32,
    towers: u32,
    layout: Layout,
    blockers: u32,
    adjust: &dyn Fn(&mut Tuning),
) -> Vec<Night> {
    (0..RUNS)
        .map(|run| {
            let siege = Siege {
                party,
                towers,
                layout,
                blockers,
                run,
            };
            third_night(siege, adjust)
        })
        .collect()
}

/// 한 칸의 세 가지 밤을 `RUNS`번씩 돌린 결과입니다.
struct Cell {
    party: u32,
    towers: u32,
    layout: Layout,
    without_sappers: Vec<Night>,
    nobody_blocks: Vec<Night>,
    one_blocks: Vec<Night>,
}

impl Cell {
    fn measure(party: u32, towers: u32, layout: Layout, adjust: &dyn Fn(&mut Tuning)) -> Cell {
        Cell {
            party,
            towers,
            layout,
            without_sappers: runs(party, towers, layout, 0, &|t| {
                adjust(t);
                without_sappers(t);
            }),
            nobody_blocks: runs(party, towers, layout, 0, adjust),
            one_blocks: runs(party, towers, layout, 1, adjust),
        }
    }

    /// 칸의 결과를 한 줄로 적습니다. 값은 실행들의 최솟값/중앙값/최댓값입니다.
    fn describe(&self) -> String {
        let (nobody, one) = (&self.nobody_blocks, &self.one_blocks);
        format!(
            "{:?}, {} players, {} towers | towers lost: no sappers {}, nobody blocks {}, one blocks {} | runs in which one loses fewer towers than nobody: {} of {RUNS} | defeats: nobody {}, one {} | wagon at the end, lowest: nobody {}, one {} | players standing at the end, fewest: nobody {}, one {}",
            self.layout,
            self.party,
            self.towers,
            spread_of_lost(&self.without_sappers),
            spread_of_lost(nobody),
            spread_of_lost(one),
            runs_where_blocking_pays(nobody, one),
            defeats(nobody),
            defeats(one),
            lowest_wagon(nobody),
            lowest_wagon(one),
            fewest_standing(nobody),
            fewest_standing(one),
        )
    }
}

/// 같은 번호의 실행에서 한 명이 막으면 아무도 막지 않을 때보다 감시탑이 더 적게 부서진 실행의 수입니다.
fn runs_where_blocking_pays(nobody: &[Night], one: &[Night]) -> usize {
    nobody
        .iter()
        .zip(one)
        .filter(|(nobody, one)| one.towers_lost() < nobody.towers_lost())
        .count()
}

/// 실행마다 부서진 감시탑의 수를 `최솟값/중앙값/최댓값`으로 적습니다.
fn spread_of_lost(nights: &[Night]) -> String {
    let mut lost: Vec<usize> = nights.iter().map(Night::towers_lost).collect();
    lost.sort_unstable();
    format!(
        "{}/{}/{}",
        lost[0],
        lost[lost.len() / 2],
        lost[lost.len() - 1]
    )
}

fn defeats(nights: &[Night]) -> usize {
    nights.iter().filter(|night| night.defeated()).count()
}

fn lowest_wagon(nights: &[Night]) -> u32 {
    nights.iter().map(|night| night.wagon_hp).min().unwrap_or(0)
}

fn fewest_standing(nights: &[Night]) -> usize {
    nights
        .iter()
        .map(|night| night.players_standing)
        .min()
        .unwrap_or(0)
}

#[test]
fn with_the_towers_a_mock_expedition_builds_nobody_blocking_loses_more_towers_than_one_blocking() {
    // 같은 번호의 실행끼리 견줍니다. 두 밤은 플레이어가 서는 자리와 움직이기 시작하는 때가 같고, 한 명이 마차를 지키지 않고 파괴병을 막는다는 점만 다릅니다.
    // 감시탑의 수는 모의 원정이 셋째 밤에 만나는 수(4명은 34개에서 37개, 3명은 26개에서 31개, 2명은 19개에서 24개) 가운데 하나입니다.
    for (party, towers) in [(4, 35), (3, 28), (2, 22)] {
        let nobody = runs(party, towers, Layout::NearTheWagon, 0, &|_| {});
        let one = runs(party, towers, Layout::NearTheWagon, 1, &|_| {});

        let paying = runs_where_blocking_pays(&nobody, &one);
        assert!(
            paying >= RUNS as usize - 1,
            "{party} players, {towers} towers: {paying} of {RUNS} runs; towers lost, nobody blocks {} and one blocks {}; defeats {} and {}",
            spread_of_lost(&nobody),
            spread_of_lost(&one),
            defeats(&nobody),
            defeats(&one)
        );
    }
}

#[test]
#[ignore = "a minute for each tower damage a sapper takes; run with --ignored"]
fn the_third_night_is_printed_for_each_party_and_tower_count_at_each_tower_damage_a_sapper_takes() {
    // 마차 가까이 세운 감시탑을 인원 4명, 3명, 2명과 감시탑의 수마다 세 번(파괴병이 없는 밤, 아무도 막지 않는 밤, 한 명이 막는 밤) 돌립니다.
    // 35개, 28개, 22개는 모의 원정이 셋째 밤에 만나는 감시탑의 수(4명은 34개에서 37개, 3명은 26개에서 31개, 2명은 19개에서 24개)와 가깝습니다.
    for percent in [25, 30, 35, 40, 45, 50] {
        println!("a sapper takes {percent}% of a tower's damage");
        let adjust = |t: &mut Tuning| t.monsters.sapper.resist.tower_damage_percent = percent;
        for (party, towers) in [
            (4, 10),
            (4, 15),
            (4, 25),
            (4, 35),
            (3, 15),
            (3, 28),
            (2, 10),
            (2, 15),
            (2, 22),
            (2, 25),
        ] {
            println!(
                "{}",
                Cell::measure(party, towers, Layout::NearTheWagon, &adjust).describe()
            );
        }
    }
}

#[test]
#[ignore = "about a minute; run with --ignored"]
fn the_third_night_is_printed_with_the_towers_spread_across_the_village() {
    // 감시탑을 마을 전체에 같은 간격으로 세우면, 마차를 지키는 사람이 닿지 못하는 감시탑을 파괴병이 노립니다. 기본 수치로 돌립니다.
    // 35개, 28개, 22개는 모의 원정이 셋째 밤에 만나는 감시탑의 수(4명은 34개에서 37개, 3명은 26개에서 31개, 2명은 19개에서 24개)와 가깝습니다.
    for (party, towers) in [
        (4, 10),
        (4, 15),
        (4, 25),
        (4, 35),
        (3, 15),
        (3, 28),
        (2, 10),
        (2, 15),
        (2, 22),
        (2, 25),
    ] {
        println!(
            "{}",
            Cell::measure(party, towers, Layout::AcrossTheVillage, &|_| {}).describe()
        );
    }
}

fn describe_one(siege: Siege, night: &Night) -> String {
    format!(
        "{:?}, {} players, {} block the sappers: {:?} after {:.0}s, {} of {} towers lost, wagon {}, {} players standing; the blocker walked {} ticks, swung {} ticks and was blocked {} ticks",
        siege.layout,
        siege.party,
        siege.blockers,
        night.outcome,
        night.secs,
        night.towers_lost(),
        night.towers_at_start,
        night.wagon_hp,
        night.players_standing,
        night.blocker_walking_ticks,
        night.blocker_swinging_ticks,
        night.blocker_blocked_ticks
    )
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
                run: 0,
            };
            let night = third_night(siege, &|_| {});
            assert_eq!(
                night.blocker_blocked_ticks,
                0,
                "{}",
                describe_one(siege, &night)
            );
            assert!(
                night.blocker_walking_ticks > 0 && night.blocker_swinging_ticks > 0,
                "{}",
                describe_one(siege, &night)
            );
        }
    }
}
