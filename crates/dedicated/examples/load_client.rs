//! 부하 시험용 클라이언트입니다. Bevy 없이 한 방에 접속하고, Tick마다 입력을 보내고,
//! 받은 스냅샷의 크기와 몬스터 수, 날아다니는 탄환의 수를 1초마다 기록합니다.
//!
//! 실행: `cargo run --release -p caravan_dedicated --example load_client -- --addr 127.0.0.1:7777 --room load --secs 60`
//!
//! 이 클라이언트는 제자리에서 공격 버튼을 누른 채 조준 방향을 돌립니다. `--passive`를 붙이면 공격하지 않아서 몬스터가 가장 많이 쌓입니다.
//! `--bow`를 붙이면 원정마다 첫 스냅샷을 받은 뒤에 무기 전환 키를 한 번 눌러서 활로 바꾸고, 탄환을 쏩니다.
//! 활로 바꾸려면 활을 가지고 있어야 하므로, 서버는 `--stress`로 실행해야 합니다.
//! 여러 개를 동시에 실행하면 한 방에 여러 접속을 만들 수 있습니다.
//! 기록하는 크기는 WebSocket 프레임의 내용(스냅샷을 이진 형식으로 바꾼 바이트)이며, 프레임 머리(2~4바이트)와 TCP 머리는 포함하지 않습니다.

use std::io;
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, InputMsg, PROTOCOL_VERSION, ServerMsg, Snapshot, WireNightStep, WirePhase, WireVec2,
    decode_server, encode_client,
};
use tokio_tungstenite::tungstenite::stream::MaybeTlsStream;
use tokio_tungstenite::tungstenite::{Error, Message, WebSocket, connect};

const USAGE: &str = "usage: load_client [--addr HOST:PORT] [--room CODE] [--name NAME] [--secs SECONDS] [--passive] [--bow]";

struct Options {
    addr: String,
    room: String,
    /// 방의 명단에 보이는 이름입니다.
    name: String,
    secs: u64,
    /// 공격 버튼을 누르지 않습니다.
    passive: bool,
    /// 첫 입력에서 활로 바꿉니다.
    bow: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        addr: "127.0.0.1:7777".to_string(),
        room: "load".to_string(),
        name: "load".to_string(),
        secs: 60,
        passive: false,
        bow: false,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--passive" {
            options.passive = true;
            continue;
        }
        if arg == "--bow" {
            options.bow = true;
            continue;
        }
        let mut value = || {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--addr" => options.addr = value()?,
            "--room" => options.room = value()?,
            "--name" => options.name = value()?,
            "--secs" => {
                let text = value()?;
                options.secs = text
                    .parse()
                    .map_err(|_| format!("--secs needs a whole number, got \"{text}\""))?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

/// 1초 동안 받은 것의 합계입니다.
#[derive(Default)]
struct Window {
    snapshots: u32,
    bytes: usize,
    largest: usize,
}

/// 실행 전체에서 몬스터가 가장 많았던 순간입니다. 탄환은 몬스터와 따로 가장 많았던 수를 셉니다.
#[derive(Default)]
struct Peak {
    monsters: usize,
    snapshot_bytes: usize,
    tick: u64,
    projectiles: usize,
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
    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: &Options) -> Result<(), String> {
    let (mut socket, _) = connect(format!("ws://{}", options.addr))
        .map_err(|err| format!("cannot connect to {}: {err}", options.addr))?;
    send(
        &mut socket,
        &ClientMsg::Join {
            version: PROTOCOL_VERSION,
            room: Some(options.room.clone()),
            name: options.name.clone(),
            token: None,
        },
    )?;

    let started = Instant::now();
    let run_for = Duration::from_secs(options.secs);
    let mut tick_period = Duration::from_millis(50);
    let mut next_input = started;
    let mut next_report = started + Duration::from_secs(1);
    let mut seq = 0u32;
    let mut window = Window::default();
    let mut peak = Peak::default();
    let mut total_bytes = 0usize;
    let mut latest: Option<Snapshot> = None;
    // 활로 바꾼 원정의 번호입니다. 새 원정은 무기를 검으로 돌려놓으므로, 원정마다 한 번씩 다시 바꿉니다.
    let mut bow_expedition: Option<u32> = None;
    let mut you: Option<u32> = None;
    let mut ready_pressed_at: Option<u64> = None;

    while started.elapsed() < run_for {
        let wait = next_input.saturating_duration_since(Instant::now());
        set_read_timeout(&mut socket, wait.max(Duration::from_millis(1)))?;
        match socket.read() {
            Ok(Message::Binary(bytes)) => match decode_server(&bytes) {
                Ok(ServerMsg::Welcome(welcome)) => {
                    tick_period = Duration::from_secs(1) / welcome.tick_hz;
                    you = Some(welcome.you);
                    eprintln!(
                        "joined room {} as player {} (tick {} Hz, capacity {})",
                        options.room, welcome.you, welcome.tick_hz, welcome.capacity
                    );
                }
                Ok(ServerMsg::JoinRejected { reason }) => {
                    return Err(format!("the server rejected the join: {reason:?}"));
                }
                Ok(ServerMsg::SeatTakenOver) => {
                    return Err("another connection took this seat".to_string());
                }
                Ok(ServerMsg::Roster(_)) => {}
                Ok(ServerMsg::Snapshot(snapshot)) => {
                    window.snapshots += 1;
                    window.bytes += bytes.len();
                    window.largest = window.largest.max(bytes.len());
                    total_bytes += bytes.len();
                    peak.projectiles = peak.projectiles.max(snapshot.projectiles.len());
                    if snapshot.monsters.len() > peak.monsters {
                        peak = Peak {
                            monsters: snapshot.monsters.len(),
                            snapshot_bytes: bytes.len(),
                            tick: snapshot.tick,
                            projectiles: peak.projectiles,
                        };
                    }
                    latest = Some(*snapshot);
                }
                Err(err) => return Err(format!("bad message from the server: {err}")),
            },
            Ok(Message::Close(_)) => return Err("the server closed the connection".to_string()),
            Ok(_) => {}
            Err(Error::Io(err))
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(err) => return Err(format!("the connection failed: {err}")),
        }

        let now = Instant::now();
        if now >= next_input {
            seq += 1;
            let angle = started.elapsed().as_secs_f32();
            let expedition = latest.as_ref().map(|snapshot| snapshot.expedition);
            let switch_weapon = options.bow && expedition.is_some() && bow_expedition != expedition;
            if switch_weapon {
                bow_expedition = expedition;
            }
            let (toggle_ready, confirm) =
                unattended_keys(latest.as_ref(), you, &mut ready_pressed_at);
            send(
                &mut socket,
                &ClientMsg::Input(InputMsg {
                    seq,
                    move_dir: WireVec2::ZERO,
                    aim: WireVec2::new(angle.cos(), angle.sin()),
                    attack: !options.passive,
                    interact: false,
                    interact_pressed: false,
                    build: false,
                    sprint: false,
                    switch_weapon,
                    toggle_ready,
                    confirm,
                }),
            )?;
            next_input += tick_period;
        }
        if now >= next_report {
            eprintln!(
                "{}",
                report_line(started.elapsed(), &window, latest.as_ref())
            );
            window = Window::default();
            next_report += Duration::from_secs(1);
        }
    }

    let seconds = started.elapsed().as_secs_f64();
    eprintln!(
        "summary: peak_monsters={} at tick {} snapshot_bytes_at_peak={} bytes_per_sec_at_peak_20hz={} average_bytes_per_sec_over_run={:.0} peak_projectiles={}",
        peak.monsters,
        peak.tick,
        peak.snapshot_bytes,
        peak.snapshot_bytes * 20,
        total_bytes as f64 / seconds,
        peak.projectiles
    );
    Ok(())
}

fn report_line(elapsed: Duration, window: &Window, latest: Option<&Snapshot>) -> String {
    let (monsters, projectiles, phase, expedition) = match latest {
        Some(snapshot) => (
            snapshot.monsters.len(),
            snapshot.projectiles.len(),
            phase_name(snapshot),
            snapshot.expedition,
        ),
        None => (0, 0, "waiting".to_string(), 0),
    };
    format!(
        "client t={:>5.1}s snapshots={} bytes={} largest={} monsters={} projectiles={} expedition={} phase={}",
        elapsed.as_secs_f32(),
        window.snapshots,
        window.bytes,
        window.largest,
        monsters,
        projectiles,
        expedition,
        phase
    )
}

/// 준비 키를 누른 결과가 스냅샷에 실려 돌아오기를 기다리는 Tick의 수입니다.
const READY_ANSWER_TICKS: u64 = 20;

/// 사람이 누를 준비 키와 확인 키를 대신 정해서 `(toggle_ready, confirm)`으로 돌려줍니다.
/// 야영지에서 준비하지 않은 상태이면 준비 키를, 결과 화면에서 확인하지 않은 상태이면 확인 키를 누릅니다. 그래서 사람의 조작 없이 원정이 되풀이됩니다.
/// 준비 키는 누를 때마다 준비를 켜고 끄므로, 누른 결과가 스냅샷에 보일 때까지는 다시 누르지 않습니다. 확인은 끌 수 없으므로 되풀이해도 결과가 같습니다.
fn unattended_keys(
    latest: Option<&Snapshot>,
    you: Option<u32>,
    ready_pressed_at: &mut Option<u64>,
) -> (bool, bool) {
    let (Some(snapshot), Some(you)) = (latest, you) else {
        return (false, false);
    };
    match &snapshot.phase {
        WirePhase::Camp { ready, .. } => {
            let answered =
                ready_pressed_at.is_none_or(|tick| snapshot.tick >= tick + READY_ANSWER_TICKS);
            let press = !ready.contains(&you) && answered;
            if press {
                *ready_pressed_at = Some(snapshot.tick);
            }
            (press, false)
        }
        WirePhase::Debrief { confirmed, .. } => (false, !confirmed.contains(&you)),
        WirePhase::Day { .. } | WirePhase::Night { .. } | WirePhase::Dawn { .. } => (false, false),
    }
}

fn phase_name(snapshot: &Snapshot) -> String {
    match snapshot.phase {
        WirePhase::Camp { .. } => "camp".to_string(),
        WirePhase::Day { day, .. } => format!("day({day})"),
        WirePhase::Night {
            day,
            wave_count,
            step,
        } => match step {
            WireNightStep::Waiting { next_wave, .. } => {
                format!("night({day}) waiting(next {next_wave}/{wave_count})")
            }
            WireNightStep::Fighting { wave } => {
                format!("night({day}) fighting({wave}/{wave_count})")
            }
        },
        WirePhase::Dawn { next_day, .. } => format!("dawn(next {next_day})"),
        WirePhase::Debrief { outcome, .. } => format!("debrief({outcome:?})"),
    }
}

fn send(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, msg: &ClientMsg) -> Result<(), String> {
    socket
        .send(Message::binary(encode_client(msg)))
        .map_err(|err| format!("cannot send: {err}"))
}

/// 읽기가 다음 입력을 보낼 시각까지만 기다리게 합니다.
fn set_read_timeout(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    wait: Duration,
) -> Result<(), String> {
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(wait))
            .map_err(|err| format!("cannot set a read timeout: {err}"))?;
    }
    Ok(())
}
