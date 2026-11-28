//! 전용 서버 실행 파일입니다. 게임 서버 라이브러리에 WebSocket 접속 방식을 연결해서 실행하는 일만 합니다.
//!
//! 실행: `cargo run -p caravan_dedicated -- --addr 127.0.0.1:7777`
//!
//! - `--debug-net`: 주고받는 모든 메시지를 읽을 수 있는 형태로 출력합니다. 스냅샷은 요약해서 출력합니다.
//! - `--stats`: 1초에 한 줄씩 방, 플레이어, 몬스터의 수와 Tick 시간의 평균과 최대를 출력합니다. 부하를 시험할 때 씁니다.
//! - `--waves 8,12/20,30,40/60,90,150`: 밤마다 무리의 몬스터 수를 바꿉니다. 밤은 `/`로, 무리는 `,`로 나누고, 밤의 수가 원정의 날 수입니다.
//!   수는 네 명이 있을 때의 크기이고, 방에 있는 사람이 적으면 그에 비례해서 줄어듭니다(`--stress`는 줄이지 않습니다). 부하를 시험할 때 씁니다.
//!   이 옵션은 무리의 몬스터 수만 정하므로 모든 무리가 기본 몬스터 한 묶음이고(돌격병과 파괴병이 섞이지 않습니다), 보스의 졸개는 이 옵션과 관계없이 기본 수치 그대로 돌격병입니다.
//! - `--stress`: 마차, 포탑, 플레이어를 죽지 않게 하고, 감시탑을 상한만큼 미리 세우고, 낮을 한 Tick으로 줄이고,
//!   몬스터를 Tick마다 한 마리씩 등장시킵니다. 기본 수치에서는 몬스터가 수십 마리 쌓이기 전에 원정이 실패로 끝나므로,
//!   감시탑이 가득 찬 마을에 몬스터 수백 마리가 몰리는 부하는 이 옵션으로 만듭니다. 플레이어는 검과 활을 함께 가지고 시작합니다.
//!   무리의 크기는 방에 있는 사람의 수와 관계없이 수치표 그대로입니다. 마지막 무리를 처치하면 원정이 끝나도록 보스는 나오지 않습니다. 사람의 조작 없이 원정이 진행되어야 하므로 야영지를 건너뜁니다. 방은 만들어지자마자 첫날을 시작하고, 결과를 한 Tick만 보여 준 뒤 새 원정을 시작합니다.
//! - `--web-dir DIR`: 같은 포트로 오는 HTTP 요청에 이 폴더(웹 빌드의 출력)의 파일을 내줍니다. 주지 않으면 파일을 내주지 않고, WebSocket 접속만 받습니다.
//! - `--public-url https://HOST`: 이 서버로 들어오는 공개 주소입니다. `--web-dir`이 필요하고, `http(s)://호스트[:포트]`의 글자만 받습니다.
//!   내주는 페이지가 이 주소를 알게 되어, 이 컴퓨터의 주소로 연 페이지에서도 초대 링크는 이 주소로 만들어집니다.
//! - `--seed 42`: 모든 방의 무작위를 이 씨앗 하나로 정합니다. 씨앗을 주지 않으면 방마다, 서버를 실행할 때마다 다른 씨앗을 씁니다.
//!   방을 만들 때 출력하는 씨앗을 이 옵션에 주면 같은 상태에서 다시 시작합니다.
//!
//! 기본 출력은 일어난 일뿐입니다. 접속을 받는 주소, 플레이어의 입장과 퇴장, 방의 생성과 제거, 한도 때문에 닫은 접속과 그 이유, 시작 오류를 한 줄씩 출력합니다.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

mod report;

use caravan_protocol::{LinkId, ServerEvent, ServerLimits, ServerMsg, ServerTransport};
use caravan_rules::{
    Finale, Loadout, MonsterKind, NightPlan, Pack, Seed, Start, Tuning, Wave, WaveScaling,
    WeaponKind,
};
use caravan_server::{GameServer, RandomSource, RoomSeeds};
use caravan_ws_server::{PublicUrl, WebFiles, WsServer};
use report::TickTimer;

const DEFAULT_ADDR: &str = "127.0.0.1:7777";
/// Tick 사이에도 이 간격으로 접속 방식을 확인합니다. 입장 요청에 답하는 지연의 상한입니다.
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const USAGE: &str = "usage: caravan-dedicated [--addr HOST:PORT] [--web-dir DIR [--public-url https://HOST]] [--debug-net] [--stats] [--waves SIZE,SIZE,.../SIZE,...] [--stress] [--seed N]";

/// 같은 포트로 웹 클라이언트의 파일을 내주는 설정입니다. 공개 주소는 폴더와 함께일 때만 있습니다.
#[derive(Debug, PartialEq, Eq)]
struct Web {
    dir: PathBuf,
    public_url: Option<PublicUrl>,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    addr: String,
    /// 없으면 파일을 내주지 않습니다.
    web: Option<Web>,
    debug_net: bool,
    /// 1초마다 서버의 규모와 Tick 시간을 한 줄로 출력합니다.
    stats: bool,
    /// 밤마다, 무리마다 몬스터의 수입니다. 없으면 기본 수치를 씁니다. 수만 정하므로 종류는 모두 기본 몬스터입니다(`tuning_from`).
    waves: Option<Vec<Vec<u32>>>,
    stress: bool,
    /// 모든 방이 쓸 씨앗입니다. 없으면 방마다 다른 씨앗을 씁니다.
    seed: Option<u64>,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        addr: DEFAULT_ADDR.to_string(),
        web: None,
        debug_net: false,
        stats: false,
        waves: None,
        stress: false,
        seed: None,
    };
    let mut web_dir = None;
    let mut public_url = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--addr" => match rest.next() {
                Some(value) => options.addr = value.clone(),
                None => return Err("--addr needs a value such as 127.0.0.1:7777".to_string()),
            },
            "--web-dir" => match rest.next() {
                Some(value) => web_dir = Some(PathBuf::from(value)),
                None => {
                    return Err("--web-dir needs a folder such as target/playtest/web".to_string());
                }
            },
            "--public-url" => match rest.next() {
                Some(value) => {
                    public_url = Some(
                        PublicUrl::parse(value).map_err(|err| format!("--public-url: {err}"))?,
                    );
                }
                None => {
                    return Err(
                        "--public-url needs an address such as https://quiet-river-1234.trycloudflare.com"
                            .to_string(),
                    );
                }
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
    options.web = match (web_dir, public_url) {
        (Some(dir), public_url) => Some(Web { dir, public_url }),
        (None, None) => None,
        (None, Some(_)) => return Err("--public-url needs --web-dir".to_string()),
    };
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
        // `--waves`는 무리의 몬스터 수를 정하는 부하 시험용 옵션이므로 무리마다 기본 몬스터 한 묶음을 만들고, 종류를 정하지 않습니다.
        // 보스의 졸개(`SummonTuning::kind`)는 이 옵션이 건드리지 않아서 기본 수치의 종류인 돌격병 그대로입니다.
        tuning.nights.plans = nights
            .iter()
            .map(|sizes| NightPlan {
                waves: sizes
                    .iter()
                    .map(|&count| Wave {
                        packs: vec![Pack::always(MonsterKind::Grunt, count)],
                    })
                    .collect(),
            })
            .collect();
    }
    if options.stress {
        // 부하의 기준은 기본 몬스터가 몰려오는 것이므로, 기본 수치의 무리에 섞인 다른 종류도 어느 인원의 방에서나 나오는 기본 몬스터로 바꿉니다.
        let packs = tuning
            .nights
            .plans
            .iter_mut()
            .flat_map(|night| &mut night.waves)
            .flat_map(|wave| &mut wave.packs);
        for pack in packs {
            *pack = Pack::always(MonsterKind::Grunt, pack.count);
        }
        tuning.wagon.max_hp = u32::MAX;
        tuning.player.max_hp = u32::MAX;
        tuning.structures.watchtower.max_hp = u32::MAX;
        tuning.structures.slow_tower.max_hp = u32::MAX;
        tuning.structures.stun_tower.max_hp = u32::MAX;
        tuning.structures.haste_tower.max_hp = u32::MAX;
        tuning.structures.swift_tower.max_hp = u32::MAX;
        tuning.village.start_towers = tuning.spread_tower_sites(tuning.village.tower_limit);
        // 부하 시험용 클라이언트(`--bow`)가 Q로 활을 들 수 있게 검과 활을 함께 가지고 시작합니다.
        tuning.player.start_loadout = Loadout::new(WeaponKind::Sword, Some(WeaponKind::Bow));
        tuning.cycle.day_secs = tuning.dt();
        tuning.nights.spawn_interval_secs = tuning.dt();
        tuning.camp.start = Start::AtOnce;
        tuning.camp.debrief_secs = tuning.dt();
        // 부하의 크기는 접속한 클라이언트의 수로 정해지지 않고 수치표로 정해집니다.
        tuning.nights.scaling = WaveScaling::Fixed;
        // 마지막 무리를 처치하면 원정이 끝나는 부하의 기준을 바꾸지 않습니다.
        tuning.nights.finale = Finale::LastWave;
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

/// 서버를 실행할 때마다 달라지는 씨앗의 뿌리입니다. 표준 라이브러리의 `RandomState`는 운영체제의 무작위로 키를 정하므로,
/// 시계를 읽지 않고 이 값을 얻습니다. 씨앗은 방을 만들 때 로그에 출력하므로 비밀이 아닙니다.
fn fresh_entropy() -> u64 {
    RandomState::new().hash_one(())
}

/// 운영체제의 암호용 무작위입니다. 자리의 표식이 이 바이트가 그대로 되므로, 다른 사람이 다음 값을 예측할 수 없어야 합니다.
struct OsRandom;

impl RandomSource for OsRandom {
    fn fill(&mut self, bytes: &mut [u8]) {
        // 운영체제가 무작위를 주지 못하면 안전한 표식을 만들 수 없으므로, 서버를 멈춥니다.
        getrandom::fill(bytes).expect("the operating system gives random bytes");
    }
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
    let files = match &options.web {
        Some(web) => match WebFiles::load(&web.dir, web.public_url.as_ref()) {
            Ok(files) => Some(files),
            Err(err) => {
                eprintln!("cannot serve {}: {err}", web.dir.display());
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let bound = match files {
        Some(files) => WsServer::bind_with_files(&options.addr, limits, files),
        None => WsServer::bind(&options.addr, limits),
    };
    let ws = match bound {
        Ok(ws) => ws,
        Err(err) => {
            eprintln!("cannot listen on {}: {err}", options.addr);
            return ExitCode::FAILURE;
        }
    };
    eprintln!("listening on ws://{}", ws.local_addr());
    if let Some(web) = &options.web {
        eprintln!(
            "serving the web client from {} at http://{}/",
            web.dir.display(),
            ws.local_addr()
        );
        if let Some(url) = &web.public_url {
            eprintln!("invite links point to {}/", url.as_str());
        }
    }

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
            .map(|night| night.waves.iter().map(Wave::count).collect())
            .collect();
        match tuning.nights.scaling {
            WaveScaling::ByParty { full_party } => {
                eprintln!("wave sizes by night for {full_party} players: {nights:?}");
            }
            WaveScaling::Fixed => eprintln!("wave sizes by night: {nights:?}"),
        }
    }
    // 자리의 표식은 운영체제의 무작위로 만듭니다. 운영체제가 무작위를 주지 못하면 사람이 입장한 뒤에 멈추지 않고, 시작하지 않습니다.
    if let Err(err) = getrandom::fill(&mut [0u8; 16]) {
        eprintln!("cannot start the server: the operating system gives no random bytes: {err}");
        return ExitCode::FAILURE;
    }
    let started = Instant::now();
    let seeds = match options.seed {
        Some(seed) => RoomSeeds::Fixed(Seed(seed)),
        None => RoomSeeds::Fresh(Seed(fresh_entropy())),
    };
    let mut server = match GameServer::new(
        transport,
        tuning,
        limits,
        seeds,
        Box::new(OsRandom),
        started,
    ) {
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
    use caravan_rules::StructureKind;
    use std::collections::BTreeSet;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_operating_system_random_gives_new_bytes_each_time() {
        let mut random = OsRandom;
        let draws: Vec<[u8; 16]> = (0..20)
            .map(|_| {
                let mut bytes = [0u8; 16];
                random.fill(&mut bytes);
                bytes
            })
            .collect();
        let distinct: std::collections::BTreeSet<_> = draws.iter().collect();
        assert_eq!(distinct.len(), draws.len(), "no draw twice");
        assert!(draws.iter().all(|bytes| bytes.iter().any(|&b| b != 0)));
    }

    #[test]
    fn defaults_and_flags() {
        assert_eq!(
            parse_args(&[]),
            Ok(Options {
                addr: "127.0.0.1:7777".to_string(),
                web: None,
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
                web: None,
                debug_net: true,
                stats: false,
                waves: None,
                stress: false,
                seed: None
            })
        );
    }

    #[test]
    fn a_web_folder_and_a_public_address_are_options_and_the_address_needs_the_folder() {
        let tunnel = "https://quiet-river-1234.trycloudflare.com";
        let options = parse_args(&args(&[
            "--web-dir",
            "target/playtest/web",
            "--public-url",
            tunnel,
        ]))
        .unwrap();
        assert_eq!(
            options.web,
            Some(Web {
                dir: PathBuf::from("target/playtest/web"),
                public_url: Some(PublicUrl::parse(tunnel).unwrap()),
            })
        );
        assert_eq!(
            parse_args(&args(&["--web-dir", "web"])).unwrap().web,
            Some(Web {
                dir: PathBuf::from("web"),
                public_url: None,
            }),
            "a folder alone is served with no public address"
        );
        assert_eq!(
            parse_args(&[]).unwrap().web,
            None,
            "no folder serves no file"
        );

        for refused in [
            &["--public-url", "https://host"][..],
            &["--web-dir"],
            &["--web-dir", "web", "--public-url"],
            &["--web-dir", "web", "--public-url", "https://a\"b"],
            &["--web-dir", "web", "--public-url", "https://host/path"],
        ] {
            assert!(parse_args(&args(refused)).is_err(), "{refused:?}");
        }
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
    fn waves_replace_the_wave_sizes_and_keep_the_pacing() {
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
            .map(|night| night.waves.iter().map(Wave::count).collect())
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
    fn waves_make_every_wave_one_pack_of_grunts_and_leave_the_boss_summons_as_they_are() {
        let summon = |tuning: &Tuning| match &tuning.nights.finale {
            Finale::Boss(boss) => boss.summon,
            Finale::LastWave => panic!("the default tuning has a boss"),
        };
        let tuning = tuning_from(&parse_args(&args(&["--waves", "8,12/20,30,40"])).unwrap());

        for pack_kinds in tuning
            .nights
            .plans
            .iter()
            .flat_map(|night| &night.waves)
            .map(|wave| wave.packs.iter().map(|pack| pack.kind).collect::<Vec<_>>())
        {
            assert_eq!(pack_kinds, [MonsterKind::Grunt]);
        }
        assert_eq!(summon(&tuning), summon(&Tuning::default()));
        assert_eq!(summon(&tuning).kind, MonsterKind::Runner);
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
        for kind in StructureKind::ALL {
            assert_eq!(tuning.structures.get(kind).max_hp, u32::MAX, "{kind:?}");
        }
        assert_eq!(tuning.ticks(tuning.nights.spawn_interval_secs), 1);
        assert_eq!(
            tuning.ticks(tuning.cycle.day_secs),
            1,
            "the first night is at once"
        );
        let sizes = |tuning: &Tuning| -> Vec<Vec<u32>> {
            let night_sizes = |night: &NightPlan| night.waves.iter().map(Wave::count).collect();
            tuning.nights.plans.iter().map(night_sizes).collect()
        };
        assert_eq!(sizes(&tuning), sizes(&Tuning::default()), "wave sizes stay");
    }

    #[test]
    fn stress_sends_only_grunts_and_a_normal_server_keeps_the_kinds_the_default_waves_mix() {
        let kinds = |arguments: &[&str]| -> BTreeSet<MonsterKind> {
            let tuning = tuning_from(&parse_args(&args(arguments)).unwrap());
            tuning
                .nights
                .plans
                .iter()
                .flat_map(|night| &night.waves)
                .flat_map(|wave| &wave.packs)
                .map(|pack| pack.kind)
                .collect()
        };

        assert_eq!(kinds(&["--stress"]), BTreeSet::from([MonsterKind::Grunt]));
        assert!(kinds(&[]).contains(&MonsterKind::Runner));
        assert!(kinds(&[]).contains(&MonsterKind::Sapper));
    }

    #[test]
    fn stress_has_no_boss_and_a_normal_server_has_one() {
        let finale = |arguments: &[&str]| {
            tuning_from(&parse_args(&args(arguments)).unwrap())
                .nights
                .finale
        };

        assert_eq!(finale(&["--stress"]), Finale::LastWave);
        assert!(matches!(finale(&[]), Finale::Boss(_)));
        assert!(
            matches!(finale(&["--waves", "5,5"]), Finale::Boss(_)),
            "custom waves keep the boss"
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

    /// 플레이어 한 명이 있는 방에서 첫째 밤의 첫 무리가 모두 나온 뒤 필드에 있는 몬스터의 수입니다.
    /// 낮의 몬스터를 빼서 무리만 세고, 무리는 마차에 닿기 전이므로 아직 아무도 처치되지 않았습니다.
    fn first_wave_for_one_player(mut tuning: Tuning) -> usize {
        tuning.guards.per_node = 0;
        tuning.guards.per_chest = 0;
        let mut sim = caravan_rules::Sim::new(tuning, Seed(1));
        sim.add_player().expect("the room has space");
        for _ in 0..100 {
            sim.step(&caravan_rules::PlayerInputs::new());
        }
        assert!(
            matches!(sim.phase(), Some(caravan_rules::Phase::Night { .. })),
            "the first night has begun"
        );
        sim.monsters().count()
    }

    #[test]
    fn stress_does_not_scale_the_waves_with_the_party_and_a_normal_server_does() {
        let stress = tuning_from(&parse_args(&args(&["--stress"])).unwrap());
        assert_eq!(
            first_wave_for_one_player(stress),
            8,
            "the whole first wave for one player"
        );

        // 야영지와 낮을 건너뛰고 몬스터가 Tick마다 나오게 하는 것 말고는 일반 서버의 수치입니다.
        let mut normal = tuning_from(&parse_args(&[]).unwrap());
        normal.camp.start = Start::AtOnce;
        normal.cycle.day_secs = normal.dt();
        normal.nights.spawn_interval_secs = normal.dt();
        assert_eq!(
            first_wave_for_one_player(normal),
            2,
            "a quarter of the first wave: 8 x 1 / 4"
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
