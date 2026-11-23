//! 전용 서버 실행 파일입니다. 게임 서버 라이브러리에 WebSocket 접속 방식을 연결해서 실행하는 일만 합니다.
//!
//! 실행: `cargo run -p caravan_dedicated -- --addr 127.0.0.1:7777`
//! `--debug-net`을 붙이면 주고받는 모든 메시지를 읽을 수 있는 형태로 출력합니다.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use caravan_protocol::{LinkId, ServerEvent, ServerLimits, ServerMsg, ServerTransport};
use caravan_rules::Tuning;
use caravan_server::GameServer;
use caravan_ws_server::WsServer;

const DEFAULT_ADDR: &str = "127.0.0.1:7777";
/// Tick 사이에도 이 간격으로 접속 방식을 확인합니다. 입장 요청에 답하는 지연의 상한입니다.
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const USAGE: &str = "usage: caravan-dedicated [--addr HOST:PORT] [--debug-net]";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    addr: String,
    debug_net: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        addr: DEFAULT_ADDR.to_string(),
        debug_net: false,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--addr" => match rest.next() {
                Some(value) => options.addr = value.clone(),
                None => return Err("--addr needs a value such as 127.0.0.1:7777".to_string()),
            },
            "--debug-net" => options.debug_net = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
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
        eprintln!("net -> {link:?} {msg:?}");
        self.inner.send(link, msg);
    }

    fn close(&mut self, link: LinkId) {
        eprintln!("net -> {link:?} close");
        self.inner.close(link);
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
    let mut server = match GameServer::new(transport, Tuning::default(), limits, Instant::now()) {
        Ok(server) => server,
        Err(err) => {
            eprintln!("cannot start the server: {err}");
            return ExitCode::FAILURE;
        }
    };
    // 프로세스를 종료하면(Ctrl-C) 반복도 끝납니다.
    loop {
        let now = Instant::now();
        for notice in server.pump(now) {
            eprintln!("{notice}");
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
                debug_net: false
            })
        );
        assert_eq!(
            parse_args(&args(&["--debug-net", "--addr", "0.0.0.0:9000"])),
            Ok(Options {
                addr: "0.0.0.0:9000".to_string(),
                debug_net: true
            })
        );
    }

    #[test]
    fn a_missing_value_or_an_unknown_flag_is_an_error() {
        assert!(parse_args(&args(&["--addr"])).is_err());
        assert!(parse_args(&args(&["--port", "7777"])).is_err());
    }
}
