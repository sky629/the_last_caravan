//! 실행 파일의 `--web-dir`와 `--public-url`을 확인합니다. 실행 파일을 직접 켜고, 시작할 때 출력하는 주소로 접속합니다.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::time::{Duration, Instant};

use caravan_protocol::{ClientMsg, PROTOCOL_VERSION, ServerMsg, decode_server, encode_client};
use tokio_tungstenite::tungstenite::stream::MaybeTlsStream;
use tokio_tungstenite::tungstenite::{Message, connect};

const PUBLIC_URL: &str = "https://quiet-river-1234.trycloudflare.com";

/// 켜 둔 서버 프로세스입니다. 버려지면 프로세스를 끝냅니다.
struct Server {
    child: Child,
    addr: SocketAddr,
    /// 서버가 출력을 계속 쓸 수 있게 읽는 끝을 열어 둡니다.
    _stderr: BufReader<ChildStderr>,
}

impl Server {
    fn start(args: &[&str]) -> Server {
        let mut child = dedicated(args).spawn().expect("start the server");
        let mut stderr = BufReader::new(child.stderr.take().expect("stderr is piped"));
        let mut first = String::new();
        stderr.read_line(&mut first).expect("read the first line");
        let addr = first
            .trim()
            .strip_prefix("listening on ws://")
            .unwrap_or_else(|| panic!("unexpected first line: {first:?}"))
            .parse()
            .expect("the address of the server");
        Server {
            child,
            addr,
            _stderr: stderr,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn dedicated(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_caravan-dedicated"));
    command
        .args(["--addr", "127.0.0.1:0"])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

/// 주어진 인자로 서버를 켜고, 5초 안에 끝나야 합니다. 종료 코드와 오류 출력을 돌려줍니다.
fn exit_of(args: &[&str]) -> (Option<i32>, String) {
    let mut child = dedicated(args).spawn().expect("start the server");
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().expect("ask for the status") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the server kept running with {args:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr is piped")
        .read_to_string(&mut stderr)
        .expect("read the output");
    (status.code(), stderr)
}

/// 폴더에 `index.html`이 있고, 그 안에 공개 주소의 자리가 있습니다.
fn web_folder(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).expect("make the web folder");
    std::fs::write(
        dir.join("index.html"),
        "<meta name=\"caravan-public-url\" content=\"{{public_url}}\">",
    )
    .expect("write the page");
    dir
}

/// `GET /`에 대한 응답 전체입니다. 서버가 아무것도 보내지 않고 닫으면 빈 글자입니다.
fn get_root(addr: SocketAddr) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect to the server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    let _ = stream.write_all(b"GET / HTTP/1.1\r\nHost: test\r\n\r\n");
    let mut answer = String::new();
    // 서버가 답하지 않고 닫으면 운영체제에 따라 연결 재설정 오류가 납니다. 그때까지 읽은 것만 봅니다.
    let _ = stream.read_to_string(&mut answer);
    answer
}

#[test]
fn the_page_comes_from_the_web_folder_with_the_public_address_in_it() {
    let dir = web_folder("web_files_with_a_public_address");
    let server = Server::start(&[
        "--web-dir",
        dir.to_str().expect("the path is UTF-8"),
        "--public-url",
        PUBLIC_URL,
    ]);

    let answer = get_root(server.addr);
    assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    assert!(
        answer.ends_with(&format!(
            "<meta name=\"caravan-public-url\" content=\"{PUBLIC_URL}\">"
        )),
        "{answer}"
    );
}

#[test]
fn a_page_without_a_public_address_leaves_the_place_empty() {
    let dir = web_folder("web_files_without_a_public_address");
    let server = Server::start(&["--web-dir", dir.to_str().expect("the path is UTF-8")]);

    let answer = get_root(server.addr);
    assert!(
        answer.ends_with("<meta name=\"caravan-public-url\" content=\"\">"),
        "{answer}"
    );
}

#[test]
fn a_player_joins_over_a_websocket_on_the_port_that_serves_the_web_folder() {
    let dir = web_folder("web_files_with_a_player");
    let server = Server::start(&["--web-dir", dir.to_str().expect("the path is UTF-8")]);

    let (mut socket, _) = connect(format!("ws://{}", server.addr)).expect("connect to the server");
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set a read timeout");
    }
    let join = ClientMsg::Join {
        version: PROTOCOL_VERSION,
        room: None,
        name: "KANG".to_string(),
        token: None,
    };
    socket
        .send(Message::binary(encode_client(&join)))
        .expect("send the join");
    loop {
        let message = socket.read().expect("an answer within five seconds");
        if let Message::Binary(bytes) = message {
            match decode_server(&bytes).expect("a valid message") {
                ServerMsg::Welcome(welcome) => {
                    assert_eq!(welcome.room.len(), 5, "the server made a new room");
                    return;
                }
                other => panic!("expected Welcome, got {other:?}"),
            }
        }
    }
}

#[test]
fn a_server_without_a_web_folder_sends_no_file() {
    let server = Server::start(&[]);
    assert_eq!(get_root(server.addr), "");
}

#[test]
fn a_folder_that_cannot_be_served_or_a_public_address_without_a_folder_stops_the_start() {
    let (code, stderr) = exit_of(&["--web-dir", "/no/such/folder"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("/no/such/folder"), "{stderr}");

    let (code, stderr) = exit_of(&["--public-url", PUBLIC_URL]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("--web-dir"), "{stderr}");

    let dir = web_folder("web_files_with_a_bad_address");
    let (code, stderr) = exit_of(&[
        "--web-dir",
        dir.to_str().expect("the path is UTF-8"),
        "--public-url",
        "https://a\"b",
    ]);
    assert_eq!(code, Some(2), "an address that does not fit in html");
    assert!(stderr.contains("--public-url"), "{stderr}");
}
