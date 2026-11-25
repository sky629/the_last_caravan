//! 전용 서버 실행 파일입니다. 게임 서버 라이브러리에 WebSocket 접속 방식을 연결해서 실행하는 일만 합니다.
//!
//! 실행: `cargo run -p caravan_dedicated -- --addr 127.0.0.1:7777`
//!
//! - `--debug-net`: 주고받는 모든 메시지를 읽을 수 있는 형태로 출력합니다. 스냅샷은 요약해서 출력합니다.
//! - `--stats`: 1초에 한 줄씩 방, 플레이어, 몬스터의 수와 Tick 시간의 평균과 최대를 출력합니다. 부하를 시험할 때 씁니다.
//! - `--waves 8,12/20,30,40/60,90,150`: 밤마다 무리의 몬스터 수를 바꿉니다. 밤은 `/`로, 무리는 `,`로 나누고, 밤의 수가 원정의 날 수입니다.
//!   부하를 시험할 때 씁니다.
//! - `--stress`: 마차, 감시탑, 플레이어를 죽지 않게 하고, 감시탑을 상한만큼 미리 세우고, 낮을 한 Tick으로 줄이고,
//!   몬스터를 Tick마다 한 마리씩 등장시킵니다. 기본 수치에서는 몬스터가 수십 마리 쌓이기 전에 원정이 실패로 끝나므로,
//!   감시탑이 가득 찬 마을에 몬스터 수백 마리가 몰리는 부하는 이 옵션으로 만듭니다. 플레이어는 검과 활을 함께 가지고 시작합니다.
//!   사람의 조작 없이 원정이 진행되어야 하므로 야영지를 건너뜁니다. 방은 만들어지자마자 첫날을 시작하고, 결과를 한 Tick만 보여 준 뒤 새 원정을 시작합니다.
//! - `--seed 42`: 모든 방의 무작위를 이 씨앗 하나로 정합니다. 씨앗을 주지 않으면 방마다, 서버를 실행할 때마다 다른 씨앗을 씁니다.
//!   방을 만들 때 출력하는 씨앗을 이 옵션에 주면 같은 상태에서 다시 시작합니다.
//!
//! 기본 출력은 일어난 일뿐입니다. 접속을 받는 주소, 플레이어의 입장과 퇴장, 방의 생성과 제거, 한도 때문에 닫은 접속과 그 이유, 시작 오류를 한 줄씩 출력합니다.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::process::ExitCode;
use std::time::{Duration, Instant};

mod report;

use caravan_protocol::{LinkId, ServerEvent, ServerLimits, ServerMsg, ServerTransport};
use caravan_rules::{Loadout, MonsterKind, NightPlan, Seed, Start, Tuning, WavePlan, WeaponKind};
use caravan_server::{GameServer, RoomSeeds, ServerSecret};
use caravan_ws_server::WsServer;
use report::TickTimer;

const DEFAULT_ADDR: &str = "127.0.0.1:7777";
/// Tick 사이에도 이 간격으로 접속 방식을 확인합니다. 입장 요청에 답하는 지연의 상한입니다.
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const USAGE: &str = "usage: caravan-dedicated [--addr HOST:PORT] [--debug-net] [--stats] [--waves SIZE,SIZE,.../SIZE,...] [--stress] [--seed N]";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    addr: String,
    debug_net: bool,
    /// 1초마다 서버의 규모와 Tick 시간을 한 줄로 출력합니다.
    stats: bool,
    /// 밤마다, 무리마다 몬스터의 수입니다. 없으면 기본 수치를 씁니다.
    waves: Option<Vec<Vec<u32>>>,
    stress: bool,
    /// 모든 방이 쓸 씨앗입니다. 없으면 방마다 다른 씨앗을 씁니다.
    seed: Option<u64>,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        addr: DEFAULT_ADDR.to_string(),
        debug_net: false,
        stats: false,
        waves: None,
        stress: false,
        seed: None,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--addr" => match rest.next() {
                Some(value) => options.addr = value.clone(),
                None => return Err("--addr needs a value such as 127.0.0.1:7777".to_string()),
            },
            "--debug-net" => options.debug_net = true,
            "--stats" => options.stats = true,
            "--stress" => options.stress = true,
            "--seed" => match rest.next() {
                Some(value) => {
                    options.seed = Some(value.parse().map_err(|_| {
                        format!(
                            "--seed: \"{value}\" is not a whole number from 0 to {}",
                            u64::MAX
                        )
                    })?)
                }
                None => return Err("--seed needs a whole number such as 42".to_string()),
            },
            "--waves" => match rest.next() {
                Some(value) => options.waves = Some(parse_waves(value)?),
                None => return Err("--waves needs a list such as 8,12/20,30,40".to_string()),
            },
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

/// `8,12/20,30,40` 같은 목록을 밤마다, 무리마다 몬스터의 수로 읽습니다. 밤은 `/`로, 무리는 `,`로 나누고 번호는 1부터 셉니다.
fn parse_waves(list: &str) -> Result<Vec<Vec<u32>>, String> {
    let mut nights = Vec::new();
    for (night_index, night) in list.split('/').enumerate() {
        let mut sizes = Vec::new();
        for (wave_index, item) in night.split(',').enumerate() {
            let place = format!("night {} wave {}", night_index + 1, wave_index + 1);
            let item = item.trim();
            if item.is_empty() {
                return Err(format!(
                    "--waves: {place} is empty; write whole numbers separated by single commas, and nights by single slashes, such as 8,12/20,30,40"
                ));
            }
            let size: u32 = item.parse().map_err(|_| {
                format!("--waves: {place} is \"{item}\", which is not a whole number of monsters")
            })?;
            if size == 0 {
                return Err(format!(
                    "--waves: {place} is 0; every wave needs at least one monster"
                ));
            }
            sizes.push(size);
        }
        nights.push(sizes);
    }
    Ok(nights)
}

fn tuning_from(options: &Options) -> Tuning {
    let mut tuning = Tuning::default();
    if let Some(nights) = &options.waves {
        tuning.nights.plans = nights
            .iter()
            .map(|sizes| NightPlan {
                waves: sizes
                    .iter()
                    .map(|&count| WavePlan {
                        kind: MonsterKind::Grunt,
                        count,
                    })
                    .collect(),
            })
            .collect();
    }
    if options.stress {
        tuning.wagon.max_hp = u32::MAX;
        tuning.player.max_hp = u32::MAX;
        tuning.structures.watchtower.max_hp = u32::MAX;
        tuning.village.start_towers = tuning.spread_tower_sites(tuning.village.tower_limit);
        // 부하 시험용 클라이언트(`--bow`)가 Q로 활을 들 수 있게 검과 활을 함께 가지고 시작합니다.
        tuning.player.start_loadout = Loadout::new(WeaponKind::Sword, Some(WeaponKind::Bow));
        tuning.cycle.day_secs = tuning.dt();
        tuning.nights.spawn_interval_secs = tuning.dt();
        tuning.camp.start = Start::AtOnce;
        tuning.camp.debrief_secs = tuning.dt();
    }
    tuning
}

/// 다른 접속 방식을 감싸서 주고받는 메시지를 `{:?}` 형태로 출력합니다. 전송 형식은 바꾸지 않습니다.
struct DebugTransport {
    inner: Box<dyn ServerTransport>,
}

impl ServerTransport for DebugTransport {
    fn poll(&mut self) -> Option<ServerEvent> {
        let event = self.inner.poll();
        if let Some(event) = &event {
            eprintln!("net <- {event:?}");
        }
        event
    }

    fn send(&mut self, link: LinkId, msg: &ServerMsg) {
        eprintln!("net -> {link:?} {}", describe(msg));
        self.inner.send(link, msg);
    }

    fn close(&mut self, link: LinkId) {
        eprintln!("net -> {link:?} close");
        self.inner.close(link);
    }
}

/// 서버를 실행할 때마다 달라지는 씨앗과 비밀 값의 뿌리입니다. 표준 라이브러리의 `RandomState`는 운영체제의 무작위로 키를 정하므로,
/// 외부 크레이트 없이, 시계를 읽지 않고 이 값을 얻습니다.
fn fresh_entropy() -> u64 {
    RandomState::new().hash_one(())
}

/// 읽을 수 있는 한 줄로 바꿉니다. 스냅샷은 몬스터가 수백 마리, 탄환이 수십 개일 수 있으므로 두 목록을 수로 줄입니다.
fn describe(msg: &ServerMsg) -> String {
    match msg {
        ServerMsg::Snapshot(snapshot) => format!(
            "Snapshot {{ tick: {}, expedition: {}, phase: {:?}, wagon: {:?}, stash: {}, towers: {:?}, nodes: {:?}, piles: {:?}, players: {:?}, monsters: {}, projectiles: {} }}",
            snapshot.tick,
            snapshot.expedition,
            snapshot.phase,
            snapshot.wagon,
            snapshot.stash,
            snapshot.towers,
            snapshot.nodes,
            snapshot.piles,
            snapshot.players,
            snapshot.monsters.len(),
            snapshot.projectiles.len()
        ),
        other => format!("{other:?}"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let limits = ServerLimits::default();
    let ws = match WsServer::bind(&options.addr, limits) {
        Ok(ws) => ws,
        Err(err) => {
            eprintln!("cannot listen on {}: {err}", options.addr);
            return ExitCode::FAILURE;
        }
    };
    eprintln!("listening on ws://{}", ws.local_addr());

    let transport: Box<dyn ServerTransport> = if options.debug_net {
        Box::new(DebugTransport {
            inner: Box::new(ws),
        })
    } else {
        Box::new(ws)
    };
    let tuning = tuning_from(&options);
    if options.waves.is_some() {
        let nights: Vec<Vec<u32>> = tuning
            .nights
            .plans
            .iter()
            .map(|night| night.waves.iter().map(|wave| wave.count).collect())
            .collect();
        eprintln!("wave sizes by night: {nights:?}");
    }
    let started = Instant::now();
    let seeds = match options.seed {
        Some(seed) => RoomSeeds::Fixed(Seed(seed)),
        None => RoomSeeds::Fresh(Seed(fresh_entropy())),
    };
    let secret = ServerSecret {
        k0: fresh_entropy(),
        k1: fresh_entropy(),
    };
    let mut server = match GameServer::new(transport, tuning, limits, seeds, secret, started) {
        Ok(server) => server,
        Err(err) => {
            eprintln!("cannot start the server: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut timer = options.stats.then(|| TickTimer::new(started));
    // 프로세스를 종료하면(Ctrl-C) 반복도 끝납니다.
    loop {
        let now = Instant::now();
        let ticks_before = timer.is_some().then(|| server.stats().ticks);
        for notice in server.pump(now) {
            eprintln!("{notice}");
        }
        if let (Some(timer), Some(ticks_before)) = (&mut timer, ticks_before) {
            let stats = server.stats();
            timer.record(stats.ticks - ticks_before, now.elapsed());
            if let Some(line) = timer.line_if_due(Instant::now(), stats) {
                eprintln!("{line}");
            }
        }
        std::thread::sleep(server.time_until_next_tick(now).min(POLL_INTERVAL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_and_flags() {
        assert_eq!(
            parse_args(&[]),
            Ok(Options {
                addr: "127.0.0.1:7777".to_string(),
                debug_net: false,
                stats: false,
                waves: None,
                stress: false,
                seed: None
            })
        );
        assert_eq!(
            parse_args(&args(&["--debug-net", "--addr", "0.0.0.0:9000"])),
            Ok(Options {
                addr: "0.0.0.0:9000".to_string(),
                debug_net: true,
                stats: false,
                waves: None,
                stress: false,
                seed: None
            })
        );
    }

    #[test]
    fn the_statistics_line_is_off_unless_stats_is_given() {
        assert!(!parse_args(&[]).unwrap().stats);
        assert!(
            !parse_args(&args(&["--stress", "--waves", "5"]))
                .unwrap()
                .stats
        );
        let options = parse_args(&args(&["--stats", "--stress"])).unwrap();
        assert!(options.stats, "--stats turns the line on");
        assert!(options.stress, "and leaves the other flags alone");
    }

    #[test]
    fn waves_replace_the_wave_sizes_and_nothing_else() {
        let options = parse_args(&args(&["--waves", "100,200/300"])).unwrap();
        assert_eq!(options.waves, Some(vec![vec![100, 200], vec![300]]));
        let tuning = tuning_from(&options);
        assert_eq!(
            tuning.day_count(),
            2,
            "one night for each slash-separated list"
        );
        let counts: Vec<Vec<u32>> = tuning
            .nights
            .plans
            .iter()
            .map(|night| night.waves.iter().map(|wave| wave.count).collect())
            .collect();
        assert_eq!(counts, [vec![100, 200], vec![300]]);
        assert_eq!(
            tuning.nights.spawn_interval_secs,
            Tuning::default().nights.spawn_interval_secs
        );
        assert_eq!(tuning.validate(), Ok(()));
        assert_eq!(tuning_from(&parse_args(&[]).unwrap()), Tuning::default());
        assert_eq!(
            parse_waves(" 7 , 8 "),
            Ok(vec![vec![7, 8]]),
            "spaces are allowed, and one night needs no slash"
        );
    }

    #[test]
    fn stress_makes_everything_unkillable_and_spawns_a_monster_every_tick() {
        let tuning = tuning_from(&parse_args(&args(&["--stress"])).unwrap());
        assert_eq!(
            tuning.validate(),
            Ok(()),
            "stress numbers keep the rules' premises"
        );
        assert_eq!(tuning.wagon.max_hp, u32::MAX);
        assert_eq!(tuning.player.max_hp, u32::MAX);
        assert_eq!(tuning.structures.watchtower.max_hp, u32::MAX);
        assert_eq!(tuning.ticks(tuning.nights.spawn_interval_secs), 1);
        assert_eq!(
            tuning.ticks(tuning.cycle.day_secs),
            1,
            "the first night is at once"
        );
        assert_eq!(
            tuning.nights.plans,
            Tuning::default().nights.plans,
            "wave sizes stay"
        );
    }

    #[test]
    fn stress_skips_the_camp_and_starts_a_new_expedition_one_tick_after_each_result() {
        let tuning = tuning_from(&parse_args(&args(&["--stress"])).unwrap());
        assert_eq!(tuning.camp.start, Start::AtOnce);
        assert_eq!(tuning.ticks(tuning.camp.debrief_secs), 1);
        let sim = caravan_rules::Sim::new(tuning, Seed(1));
        assert_eq!(sim.expedition_number(), 1, "no camp and no waiting");
        assert!(sim.phase().is_some());

        let normal = tuning_from(&parse_args(&[]).unwrap());
        assert_eq!(normal.camp.start, Start::WhenReady);
        assert_eq!(
            caravan_rules::Sim::new(normal, Seed(1)).expedition_number(),
            0,
            "a normal server starts every room in the camp"
        );
    }

    #[test]
    fn stress_starts_with_the_tower_cap_already_standing() {
        let tuning = tuning_from(&parse_args(&args(&["--stress"])).unwrap());
        assert_eq!(
            tuning.village.start_towers.len(),
            tuning.village.tower_limit as usize
        );
        let sim = caravan_rules::Sim::new(tuning.clone(), Seed(1));
        assert_eq!(sim.towers().count(), tuning.village.tower_limit as usize);
        assert!(sim.towers().all(|tower| tower.is_standing()));
        assert!(
            tuning_from(&parse_args(&[]).unwrap())
                .village
                .start_towers
                .is_empty(),
            "a normal server starts with no towers"
        );
    }

    #[test]
    fn a_bad_wave_list_is_refused_with_a_message_that_names_the_night_and_the_wave() {
        let message = |list: &str| parse_waves(list).unwrap_err();
        assert!(message("").contains("night 1 wave 1 is empty"));
        assert!(message("100,,300").contains("night 1 wave 2 is empty"));
        assert!(message("100,200,").contains("night 1 wave 3 is empty"));
        assert!(message("100/").contains("night 2 wave 1 is empty"));
        assert!(message("100/200,abc").contains("night 2 wave 2 is \"abc\""));
        assert!(message("-5").contains("night 1 wave 1 is \"-5\""));
        assert!(message("1.5").contains("not a whole number"));
        assert!(message("99999999999").contains("not a whole number"));
        assert!(message("10/20,0").contains("night 2 wave 2 is 0"));
    }

    #[test]
    fn seed_takes_a_whole_number_and_nothing_else_changes() {
        assert_eq!(parse_args(&args(&["--seed", "42"])).unwrap().seed, Some(42));
        assert_eq!(
            parse_args(&[]).unwrap().seed,
            None,
            "no seed means a fresh seed for every room"
        );
        assert!(parse_args(&args(&["--seed", "-1"])).is_err());
        assert!(parse_args(&args(&["--seed", "4.5"])).is_err());
        assert!(parse_args(&args(&["--seed", "many"])).is_err());
        assert!(parse_args(&args(&["--seed"])).is_err());
        assert_eq!(
            tuning_from(&parse_args(&args(&["--seed", "42"])).unwrap()),
            Tuning::default(),
            "the seed is not a tuning number"
        );
    }

    #[test]
    fn stress_players_start_with_the_sword_and_the_bow() {
        let tuning = tuning_from(&parse_args(&args(&["--stress"])).unwrap());
        assert_eq!(tuning.validate(), Ok(()));
        assert_eq!(
            tuning.player.start_loadout,
            Loadout::new(WeaponKind::Sword, Some(WeaponKind::Bow))
        );
        assert_eq!(
            tuning_from(&parse_args(&[]).unwrap()).player.start_loadout,
            Loadout::new(WeaponKind::Sword, None),
            "a normal server starts with the sword alone"
        );
    }

    #[test]
    fn a_missing_value_or_an_unknown_flag_is_an_error() {
        assert!(parse_args(&args(&["--addr"])).is_err());
        assert!(parse_args(&args(&["--waves"])).is_err());
        assert!(parse_args(&args(&["--port", "7777"])).is_err());
    }
}
