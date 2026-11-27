//! 게임 접속과 같은 포트로 웹 클라이언트의 파일을 내주는 서버를 127.0.0.1의 실제 소켓으로 확인합니다.
//! 파일 요청은 `std`의 TCP 소켓으로 HTTP/1.1 요청을 직접 써서 보냅니다. 브라우저와 터널 프로그램이 보내는 요청과 같은 모양입니다.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use caravan_protocol::{
    ClientMsg, PROTOCOL_VERSION, ServerEvent, ServerLimits, ServerTransport, encode_client,
};
use caravan_ws_server::{WebFiles, WsServer};
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;

const PAGE: &str = "<!doctype html><html><head><title>t</title></head><body></body></html>";
const SCRIPT: &str = "export default function init() {}";
const WASM: &[u8] = b"\0asm\x01\0\0\0";
/// RFC 6455가 예로 드는 `Sec-WebSocket-Key`입니다.
const WEBSOCKET_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

/// 테스트마다 새로 만드는 빈 폴더입니다. 버려지면 지웁니다.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("make a temporary folder");
        TempDir(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.0.join(name), bytes).expect("write a test file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// trunk가 만드는 것과 같은 세 파일이 있는 폴더입니다.
fn web_dir() -> TempDir {
    let dir = TempDir::new();
    dir.write("index.html", PAGE.as_bytes());
    dir.write("caravan-client-1a2b.js", SCRIPT.as_bytes());
    dir.write("caravan-client-3c4d_bg.wasm", WASM);
    dir
}

fn serving(dir: &Path, limits: ServerLimits) -> WsServer {
    let files = WebFiles::load(dir).expect("the folder has a page");
    WsServer::bind_with_files("127.0.0.1:0", limits, files).expect("bind a free port")
}

/// HTTP 응답 하나입니다.
struct Reply {
    status: String,
    headers: String,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

fn parse(bytes: &[u8]) -> Option<Reply> {
    let end = bytes.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = String::from_utf8(bytes[..end].to_vec()).ok()?;
    let (status, headers) = head.split_once("\r\n").unwrap_or((&head, ""));
    Some(Reply {
        status: status.to_string(),
        headers: headers.to_string(),
        body: bytes[end + 4..].to_vec(),
    })
}

/// 보낸 바이트를 그대로 서버에 쓰고, 서버가 닫을 때까지 읽은 바이트를 돌려줍니다. 서버가 닫지 않으면 실패합니다.
fn exchange(addr: SocketAddr, bytes: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).expect("connect to the server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set a read timeout");
    // 서버가 요청을 다 읽기 전에 닫으면 쓰기가 실패할 수 있습니다. 그때도 서버가 닫은 뒤의 상태를 확인합니다.
    let _ = stream.write_all(bytes);
    read_until_closed(stream)
}

/// 서버가 접속을 닫을 때까지 읽습니다. 서버가 아무것도 보내지 않고 닫으면 빈 목록입니다.
fn read_until_closed(mut stream: TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    // 서버가 읽지 않은 바이트를 두고 닫으면 운영체제에 따라 연결 재설정 오류가 납니다. 그때까지 읽은 것만 봅니다.
    if let Err(err) = stream.read_to_end(&mut bytes) {
        assert!(
            !matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
            "the server kept the socket open"
        );
    }
    bytes
}

fn request(addr: SocketAddr, method: &str, target: &str) -> Reply {
    let bytes = exchange(
        addr,
        format!("{method} {target} HTTP/1.1\r\nHost: {addr}\r\nAccept: */*\r\n\r\n").as_bytes(),
    );
    parse(&bytes).expect("an answer")
}

fn websocket_request(target: &str, upgrade: &str) -> String {
    format!(
        "GET {target} HTTP/1.1\r\nhost: x\r\nconnection: Upgrade\r\n{upgrade}\r\nsec-websocket-key: {WEBSOCKET_KEY}\r\nsec-websocket-version: 13\r\n\r\n"
    )
}

/// 파일을 받기 시작하는 접속입니다. 응답의 첫 줄을 읽은 뒤에는 읽지 않으므로, 서버의 쓰기가 막힙니다.
fn start_download(addr: SocketAddr, name: &str) -> TcpStream {
    let mut stream = TcpStream::connect(addr).expect("connect to the server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set a read timeout");
    write!(stream, "GET /{name} HTTP/1.1\r\nHost: x\r\n\r\n").expect("ask for the file");
    let mut first = [0u8; 15];
    stream.read_exact(&mut first).expect("the first line");
    assert_eq!(&first, b"HTTP/1.1 200 OK");
    stream
}

/// 서버의 이벤트를 비우면서 `Connected`가 올 때까지 기다립니다.
async fn connected(server: &mut WsServer) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match server.poll() {
            Some(ServerEvent::Connected(..)) => return true,
            Some(_) => {}
            None => tokio::time::sleep(Duration::from_millis(5)).await,
        }
    }
    false
}

/// 파일 요청이 다시 `200`을 받을 때까지 기다립니다.
async fn page_is_served_again(server: &mut WsServer) -> bool {
    let addr = server.local_addr();
    for _ in 0..300 {
        let _ = server.poll();
        if request(addr, "GET", "/").status == "HTTP/1.1 200 OK" {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[test]
fn the_page_the_script_and_the_wasm_come_over_http_on_the_game_port() {
    let dir = web_dir();
    let server = serving(&dir.0, ServerLimits::default());
    let addr = server.local_addr();

    for (target, content_type, body, cache) in [
        ("/", "text/html; charset=utf-8", PAGE.as_bytes(), "no-cache"),
        (
            "/?room=k7m2q",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
            "no-cache",
        ),
        (
            "/index.html",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
            "no-cache",
        ),
        (
            "/caravan-client-1a2b.js",
            "text/javascript; charset=utf-8",
            SCRIPT.as_bytes(),
            "public, max-age=3600",
        ),
        (
            "/caravan-client-3c4d_bg.wasm",
            "application/wasm",
            WASM,
            "public, max-age=3600",
        ),
    ] {
        let reply = request(addr, "GET", target);
        assert_eq!(reply.status, "HTTP/1.1 200 OK", "{target}");
        assert_eq!(reply.header("content-type"), Some(content_type), "{target}");
        assert_eq!(reply.header("cache-control"), Some(cache), "{target}");
        assert_eq!(
            reply.header("content-length"),
            Some(body.len().to_string().as_str()),
            "{target}"
        );
        assert_eq!(reply.body, body, "{target}");
    }
}

#[test]
fn head_answers_with_the_headers_of_get_and_no_body() {
    let dir = web_dir();
    let server = serving(&dir.0, ServerLimits::default());
    let addr = server.local_addr();

    let get = request(addr, "GET", "/caravan-client-3c4d_bg.wasm");
    let head = request(addr, "HEAD", "/caravan-client-3c4d_bg.wasm");
    assert_eq!(head.status, "HTTP/1.1 200 OK");
    assert_eq!(head.headers, get.headers);
    assert_eq!(
        head.header("content-length"),
        Some(WASM.len().to_string().as_str())
    );
    assert!(head.body.is_empty());
}

#[test]
fn a_missing_file_is_not_found_and_another_method_is_not_allowed() {
    let dir = web_dir();
    let server = serving(&dir.0, ServerLimits::default());
    let addr = server.local_addr();

    assert_eq!(
        request(addr, "GET", "/favicon.ico").status,
        "HTTP/1.1 404 Not Found"
    );
    assert_eq!(
        request(addr, "HEAD", "/favicon.ico").status,
        "HTTP/1.1 404 Not Found"
    );
    for method in ["POST", "PUT", "DELETE", "OPTIONS"] {
        let reply = request(addr, method, "/");
        assert_eq!(reply.status, "HTTP/1.1 405 Method Not Allowed", "{method}");
        assert_eq!(reply.header("allow"), Some("GET, HEAD"), "{method}");
    }
}

#[test]
fn a_server_without_files_sends_nothing_to_a_plain_get() {
    let server = WsServer::bind("127.0.0.1:0", ServerLimits::default()).unwrap();
    let bytes = exchange(
        server.local_addr(),
        b"GET / HTTP/1.1\r\nHost: x\r\nAccept: */*\r\n\r\n",
    );
    assert!(
        bytes.is_empty(),
        "the WebSocket handshake fails and the socket closes without an answer"
    );
}

#[test]
fn nothing_outside_the_folder_is_served() {
    let root = TempDir::new();
    let web = root.0.join("web");
    let outside_dir = root.0.join("outside");
    std::fs::create_dir_all(web.join("sub")).unwrap();
    std::fs::create_dir_all(&outside_dir).unwrap();
    std::fs::write(web.join("index.html"), PAGE).unwrap();
    std::fs::write(web.join("sub").join("inner.js"), "inner").unwrap();
    std::fs::write(web.join(".hidden.js"), "hidden").unwrap();
    std::fs::write(web.join("notes.txt"), "text").unwrap();
    std::fs::write(web.join("client.wasm.map"), "map").unwrap();
    std::fs::write(root.0.join("Cargo.toml"), "secret").unwrap();
    std::fs::write(root.0.join("outside.js"), "secret").unwrap();
    std::fs::write(outside_dir.join("secret.js"), "secret").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.0.join("outside.js"), web.join("linked.js")).unwrap();
        std::os::unix::fs::symlink(&outside_dir, web.join("linked_dir")).unwrap();
    }

    let server = serving(&web, ServerLimits::default());
    let addr = server.local_addr();
    assert_eq!(
        request(addr, "GET", "/index.html").status,
        "HTTP/1.1 200 OK",
        "the folder itself is served"
    );
    for target in [
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
        "/%2E%2E%2FCargo.toml",
        "/..%2fCargo.toml",
        "/../outside.js",
        "/linked.js",
        "/linked_dir/secret.js",
        "/sub/inner.js",
        "/.hidden.js",
        "/notes.txt",
        "/client.wasm.map",
        "//etc/passwd",
        "/index.html/",
        "/INDEX.HTML",
        "http://127.0.0.1/index.html",
    ] {
        let reply = request(addr, "GET", target);
        assert_eq!(reply.status, "HTTP/1.1 404 Not Found", "{target}");
        let body = String::from_utf8_lossy(&reply.body);
        assert!(!body.contains("secret"), "{target}");
    }
}

#[test]
fn a_folder_without_a_page_is_refused_at_start() {
    let dir = TempDir::new();
    dir.write("caravan-client-1a2b.js", SCRIPT.as_bytes());
    assert!(WebFiles::load(&dir.0).is_err());
}

#[tokio::test]
async fn a_websocket_on_the_port_that_serves_files_is_a_game_link() {
    let dir = web_dir();
    let mut server = serving(&dir.0, ServerLimits::default());
    // 웹 클라이언트와 PC 클라이언트는 경로 없이 접속하고, 경로를 붙인 주소도 같은 접속입니다.
    // 앞의 접속이 닫히는 이벤트가 끼지 않도록 두 접속을 끝까지 열어 둡니다.
    let mut clients = Vec::new();
    for path in ["", "/play?x=1"] {
        let url = format!("ws://{}{path}", server.local_addr());
        let (mut client, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let join = ClientMsg::Join {
            version: PROTOCOL_VERSION,
            room: Some("test".to_string()),
            name: "TESTER".to_string(),
            token: None,
        };
        client
            .send(Message::binary(encode_client(&join)))
            .await
            .unwrap();
        let mut events = Vec::new();
        while events.len() < 2 {
            match server.poll() {
                Some(event) => events.push(event),
                None => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        }
        assert!(matches!(events[0], ServerEvent::Connected(..)), "{path}");
        assert!(
            matches!(&events[1], ServerEvent::Message(_, msg) if *msg == join),
            "{path}"
        );
        clients.push(client);
    }
}

#[test]
fn a_websocket_request_is_known_by_any_case_of_its_headers_and_by_a_head_in_pieces() {
    let dir = web_dir();
    let server = serving(&dir.0, ServerLimits::default());
    let addr = server.local_addr();

    let upgrade = "UPGRADE: WebSocket";
    let whole = exchange_in_pieces(addr, &websocket_request("/", upgrade), usize::MAX);
    assert!(whole.starts_with(b"HTTP/1.1 101 "), "{whole:?}");
    let in_pieces = exchange_in_pieces(addr, &websocket_request("/", upgrade), 7);
    assert!(in_pieces.starts_with(b"HTTP/1.1 101 "), "{in_pieces:?}");

    let page = exchange_in_pieces(addr, "GET / HTTP/1.1\r\nHost: x\r\n\r\n", 7);
    assert!(page.starts_with(b"HTTP/1.1 200 OK"), "{page:?}");
}

/// 요청을 `piece` 바이트씩 끊어서 보내고, 응답의 앞부분(WebSocket이면 101의 머리, 파일이면 전부)을 읽습니다.
fn exchange_in_pieces(addr: SocketAddr, text: &str, piece: usize) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).expect("connect to the server");
    stream.set_nodelay(true).expect("send small pieces at once");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set a read timeout");
    for part in text.as_bytes().chunks(piece) {
        stream.write_all(part).expect("write a piece");
        stream.flush().expect("flush a piece");
        std::thread::sleep(Duration::from_millis(2));
    }
    if text.contains("Upgrade") {
        // WebSocket 연결은 열려 있으므로 응답의 머리가 끝날 때까지만 읽습니다.
        let mut bytes = Vec::new();
        let mut byte = [0u8; 1];
        while !bytes.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).expect("read the answer");
            bytes.push(byte[0]);
        }
        bytes
    } else {
        read_until_closed(stream)
    }
}

#[test]
fn a_request_head_that_is_unfinished_too_big_or_not_http_is_closed_without_an_answer() {
    let dir = web_dir();
    let limits = ServerLimits {
        handshake_timeout: Duration::from_millis(300),
        ..ServerLimits::default()
    };
    let server = serving(&dir.0, limits);
    let addr = server.local_addr();

    // 머리가 끝나지 않으면 연결 절차의 시간 제한에서 닫습니다.
    let started = Instant::now();
    let bytes = exchange(addr, b"GET / HTTP/1.1\r\nHost: x\r\n");
    assert!(bytes.is_empty());
    assert!(started.elapsed() < Duration::from_secs(5));

    let mut too_long = b"GET / HTTP/1.1\r\nHost: x\r\nX-Pad: ".to_vec();
    too_long.extend(std::iter::repeat_n(b'a', 20_000));
    too_long.extend_from_slice(b"\r\n\r\n");
    assert!(exchange(addr, &too_long).is_empty(), "a head over 16 KiB");

    let mut many = String::from("GET / HTTP/1.1\r\nHost: x\r\n");
    for n in 0..70 {
        many.push_str(&format!("X-{n}: v\r\n"));
    }
    many.push_str("\r\n");
    assert!(
        exchange(addr, many.as_bytes()).is_empty(),
        "70 header lines"
    );

    assert!(exchange(addr, &[0x16, 0x03, 0x01, 0x02, 0x00, 0x01]).is_empty());

    assert_eq!(
        request(addr, "GET", "/").status,
        "HTTP/1.1 200 OK",
        "the server still answers"
    );
}

#[tokio::test]
async fn downloads_in_progress_hold_file_slots_but_no_game_seat() {
    let dir = web_dir();
    // 소켓의 버퍼에 다 들어가지 않는 크기이므로, 받는 쪽이 읽지 않으면 서버의 쓰기가 멈춥니다.
    dir.write("big_bg.wasm", &vec![7u8; 32 * 1024 * 1024]);
    let limits = ServerLimits {
        write_timeout: Duration::from_secs(60),
        ..ServerLimits::default()
    };
    let slots = limits.max_file_responses;
    assert_eq!(
        slots, limits.max_links_per_ip,
        "the seats would run out first"
    );
    let mut server = serving(&dir.0, limits);
    let addr = server.local_addr();

    let downloads: Vec<TcpStream> = (0..slots)
        .map(|_| start_download(addr, "big_bg.wasm"))
        .collect();

    let busy = request(addr, "GET", "/");
    assert_eq!(
        busy.status, "HTTP/1.1 503 Service Unavailable",
        "every file slot is taken by a download"
    );

    let (_client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}"))
        .await
        .expect("the downloads keep no seat of this address");
    assert!(
        connected(&mut server).await,
        "a player joins while every file slot is busy"
    );

    drop(downloads);
    assert!(
        page_is_served_again(&mut server).await,
        "closing the downloads frees their slots"
    );
}

#[tokio::test]
async fn a_download_that_stops_reading_is_cut_at_the_write_timeout() {
    let dir = web_dir();
    dir.write("big_bg.wasm", &vec![7u8; 32 * 1024 * 1024]);
    let limits = ServerLimits {
        max_file_responses: 1,
        write_timeout: Duration::from_secs(1),
        ..ServerLimits::default()
    };
    let mut server = serving(&dir.0, limits);
    let addr = server.local_addr();

    let _stalled = start_download(addr, "big_bg.wasm");
    assert_eq!(
        request(addr, "GET", "/").status,
        "HTTP/1.1 503 Service Unavailable",
        "the stalled download holds the only slot"
    );
    assert!(
        page_is_served_again(&mut server).await,
        "the write that did not finish in time frees the slot"
    );
}
