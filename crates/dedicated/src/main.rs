//! 전용 서버 실행 파일입니다. 게임 서버 라이브러리에 WebSocket 접속 방식을 연결해서 실행하는 일만 합니다.
//!
//! 실행: `cargo run -p caravan_dedicated -- --addr 127.0.0.1:7777`
//!
//! - `--debug-net`: 주고받는 모든 메시지를 읽을 수 있는 형태로 출력합니다. 스냅샷은 요약해서 출력합니다.
//! - `--stats`: 1초에 한 줄씩 방, 플레이어, 몬스터의 수와 Tick 시간의 평균과 최대를 출력합니다. 부하를 시험할 때 씁니다.
//! - `--waves 100,200,300`: 무리마다 몬스터의 수를 바꿉니다. 부하를 시험할 때 씁니다.
//! - `--stress`: 마차, 감시탑, 플레이어를 죽지 않게 하고 몬스터를 Tick마다 한 마리씩 등장시킵니다.
//!   기본 수치에서는 몬스터가 수십 마리 쌓이기 전에 원정이 실패로 끝나므로, 몬스터 수백 마리의 부하는 이 옵션으로 만듭니다.
//!
//! 기본 출력은 일어난 일뿐입니다. 접속을 받는 주소, 플레이어의 입장과 퇴장, 방의 생성과 제거, 한도 때문에 닫은 접속과 그 이유, 시작 오류를 한 줄씩 출력합니다.

use std::process::ExitCode;
use std::time::{Duration, Instant};

mod report;

use caravan_protocol::{LinkId, ServerEvent, ServerLimits, ServerMsg, ServerTransport};
use caravan_rules::Tuning;
use caravan_server::GameServer;
use caravan_ws_server::WsServer;
use report::TickTimer;

const DEFAULT_ADDR: &str = "127.0.0.1:7777";
/// Tick 사이에도 이 간격으로 접속 방식을 확인합니다. 입장 요청에 답하는 지연의 상한입니다.
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const USAGE: &str = "usage: caravan-dedicated [--addr HOST:PORT] [--debug-net] [--stats] [--waves SIZE,SIZE,...] [--stress]";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    addr: String,
    debug_net: bool,
    /// 1초마다 서버의 규모와 Tick 시간을 한 줄로 출력합니다.
    stats: bool,
    /// 무리마다 몬스터의 수입니다. 없으면 기본 수치를 씁니다.
    waves: Option<Vec<u32>>,
    stress: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        addr: DEFAULT_ADDR.to_string(),
        debug_net: false,
        stats: false,
        waves: None,
        stress: false,
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
            "--waves" => match rest.next() {
                Some(value) => options.waves = Some(parse_waves(value)?),
                None => return Err("--waves needs a list such as 100,200,300".to_string()),
            },
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

/// `100,200,300` 같은 목록을 무리마다 몬스터의 수로 읽습니다. 번호는 1부터 셉니다.
fn parse_waves(list: &str) -> Result<Vec<u32>, String> {
    let mut sizes = Vec::new();
    for (index, item) in list.split(',').enumerate() {
        let wave = index + 1;
        let item = item.trim();
        if item.is_empty() {
            return Err(format!(
                "--waves: wave {wave} is empty; write whole numbers separated by single commas, such as 100,200,300"
            ));
        }
        let size: u32 = item.parse().map_err(|_| {
            format!("--waves: wave {wave} is \"{item}\", which is not a whole number of monsters")
        })?;
        if size == 0 {
            return Err(format!(
                "--waves: wave {wave} is 0; every wave needs at least one monster"
            ));
        }
        sizes.push(size);
    }
    Ok(sizes)
}

fn tuning_from(options: &Options) -> Tuning {
    let mut tuning = Tuning::default();
    if let Some(sizes) = &options.waves {
        tuning.waves.sizes = sizes.clone();
    }
    if options.stress {
        tuning.wagon.max_hp = u32::MAX;
        tuning.player.max_hp = u32::MAX;
        tuning.waves.spawn_interval_secs = tuning.dt();
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

/// 읽을 수 있는 한 줄로 바꿉니다. 스냅샷은 몬스터가 수백 마리일 수 있으므로 몬스터 목록을 수로 줄입니다.
fn describe(msg: &ServerMsg) -> String {
    match msg {
        ServerMsg::Snapshot(snapshot) => format!(
            "Snapshot {{ tick: {}, expedition: {}, phase: {:?}, wagon: {:?}, stash: {}, towers: {:?}, players: {:?}, monsters: {} }}",
            snapshot.tick,
            snapshot.expedition,
            snapshot.phase,
            snapshot.wagon,
            snapshot.stash,
            snapshot.towers,
            snapshot.players,
            snapshot.monsters.len()
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
        eprintln!("wave sizes: {:?}", tuning.waves.sizes);
    }
    let started = Instant::now();
    let mut server = match GameServer::new(transport, tuning, limits, started) {
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
                stress: false
            })
        );
        assert_eq!(
            parse_args(&args(&["--debug-net", "--addr", "0.0.0.0:9000"])),
            Ok(Options {
                addr: "0.0.0.0:9000".to_string(),
                debug_net: true,
                stats: false,
                waves: None,
                stress: false
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
        let options = parse_args(&args(&["--waves", "100,200,300"])).unwrap();
        assert_eq!(options.waves, Some(vec![100, 200, 300]));
        let tuning = tuning_from(&options);
        assert_eq!(tuning.waves.sizes, [100, 200, 300]);
        assert_eq!(
            tuning.waves.spawn_interval_secs,
            Tuning::default().waves.spawn_interval_secs
        );
        assert_eq!(tuning_from(&parse_args(&[]).unwrap()), Tuning::default());
        assert_eq!(parse_waves(" 7 , 8 "), Ok(vec![7, 8]), "spaces are allowed");
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
        assert_eq!(tuning.ticks(tuning.waves.spawn_interval_secs), 1);
        assert_eq!(
            tuning.waves.sizes,
            Tuning::default().waves.sizes,
            "wave sizes stay"
        );
    }

    #[test]
    fn a_bad_wave_list_is_refused_with_a_message_that_names_the_wave() {
        let message = |list: &str| parse_waves(list).unwrap_err();
        assert!(message("").contains("wave 1 is empty"));
        assert!(message("100,,300").contains("wave 2 is empty"));
        assert!(message("100,200,").contains("wave 3 is empty"));
        assert!(message("100,abc").contains("wave 2 is \"abc\""));
        assert!(message("-5").contains("wave 1 is \"-5\""));
        assert!(message("1.5").contains("not a whole number"));
        assert!(message("99999999999").contains("not a whole number"));
        assert!(message("10,0").contains("wave 2 is 0"));
    }

    #[test]
    fn a_missing_value_or_an_unknown_flag_is_an_error() {
        assert!(parse_args(&args(&["--addr"])).is_err());
        assert!(parse_args(&args(&["--waves"])).is_err());
        assert!(parse_args(&args(&["--port", "7777"])).is_err());
    }
}
