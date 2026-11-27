//! 게임 접속과 같은 포트로 웹 클라이언트의 파일을 내줍니다.
//!
//! 서버는 시작할 때 폴더 바로 아래의 파일을 메모리에 읽어 두고, 요청의 경로를 그 파일의 이름과 글자 그대로 비교합니다.
//! 그래서 요청은 읽어 둔 파일 말고는 어떤 파일 시스템의 경로에도 닿지 않습니다. 하위 폴더, 심볼릭 링크, 이름이 `.`으로 시작하는 파일,
//! 표에 없는 확장자의 파일은 읽지 않습니다. 경로의 퍼센트 인코딩은 풀지 않으므로, 인코딩한 경로는 어떤 이름과도 같지 않습니다.
//! 폴더의 파일이 바뀌면 서버를 다시 켜야 합니다.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

/// 페이지의 파일 이름입니다. `/`는 이 파일을 가리킵니다.
const INDEX: &str = "index.html";
/// 내주는 파일의 확장자와 `Content-Type`입니다. trunk가 만드는 파일의 종류뿐입니다.
const TYPES: [(&str, &str); 3] = [
    ("html", "text/html; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("wasm", "application/wasm"),
];
/// 페이지는 매번 서버에 다시 묻습니다. 다른 파일은 한 시간 동안 다시 묻지 않습니다.
/// trunk가 파일 이름에 넣는 해시는 최적화를 마친 내용을 반영하지 않을 수 있으므로, 이름이 같아도 내용이 바뀌는 경우를 위해 길게 간직하지 않게 합니다.
const INDEX_CACHE: &str = "no-cache";
const FILE_CACHE: &str = "public, max-age=3600";

/// 서버가 내주는 파일입니다. 내용은 시작할 때 읽고, 요청마다 복사하지 않고 함께 씁니다.
#[derive(Debug)]
pub struct WebFiles {
    files: HashMap<String, WebFile>,
}

#[derive(Debug)]
struct WebFile {
    body: Arc<[u8]>,
    content_type: &'static str,
    cache_control: &'static str,
}

/// 요청 하나에 대한 답입니다. 머리와 본문을 따로 가지므로 `HEAD`는 머리만 보냅니다.
pub(crate) struct Answer {
    pub(crate) head: Vec<u8>,
    pub(crate) body: Arc<[u8]>,
}

impl WebFiles {
    /// `dir` 바로 아래의 파일을 읽습니다. `index.html`이 없으면 오류입니다.
    pub fn load(dir: &Path) -> io::Result<WebFiles> {
        let mut files = HashMap::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            // `DirEntry::file_type`은 심볼릭 링크를 따라가지 않으므로, 폴더 밖을 가리키는 링크는 여기서 빠집니다.
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Some(content_type) = content_type_of(&name) else {
                continue;
            };
            let cache_control = if name == INDEX {
                INDEX_CACHE
            } else {
                FILE_CACHE
            };
            let body = std::fs::read(entry.path())?.into();
            files.insert(
                name,
                WebFile {
                    body,
                    content_type,
                    cache_control,
                },
            );
        }
        if !files.contains_key(INDEX) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} has no {INDEX}", dir.display()),
            ));
        }
        Ok(WebFiles { files })
    }

    /// 요청 줄의 메서드와 경로로 답을 정합니다. 경로의 쿼리(`?room=...`)는 보지 않습니다.
    pub(crate) fn answer(&self, method: &str, target: &str) -> Answer {
        let head_only = match method {
            "GET" => false,
            "HEAD" => true,
            _ => return Answer::status("405 Method Not Allowed", "Allow: GET, HEAD\r\n", false),
        };
        let path = target.split_once('?').map_or(target, |(path, _)| path);
        let name = match path {
            "/" => INDEX,
            other => other.strip_prefix('/').unwrap_or(""),
        };
        match self.files.get(name) {
            Some(file) => Answer {
                head: head(
                    "200 OK",
                    file.content_type,
                    file.cache_control,
                    "",
                    file.body.len(),
                ),
                body: if head_only {
                    Arc::from([])
                } else {
                    Arc::clone(&file.body)
                },
            },
            None => Answer::status("404 Not Found", "", head_only),
        }
    }
}

impl Answer {
    /// 파일 응답의 자리가 모두 차 있을 때의 답입니다.
    pub(crate) fn busy(method: &str) -> Answer {
        Answer::status("503 Service Unavailable", "", method == "HEAD")
    }

    /// 파일이 아닌 답입니다. 본문은 상태 줄의 글자입니다.
    fn status(status: &str, extra_headers: &str, head_only: bool) -> Answer {
        let body = format!("{status}\n").into_bytes();
        Answer {
            head: head(
                status,
                "text/plain; charset=utf-8",
                "no-store",
                extra_headers,
                body.len(),
            ),
            body: if head_only {
                Arc::from([])
            } else {
                body.into()
            },
        }
    }
}

/// 응답의 머리입니다. 요청 하나마다 접속 하나를 쓰고 닫습니다.
fn head(
    status: &str,
    content_type: &str,
    cache_control: &str,
    extra_headers: &str,
    content_length: usize,
) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nCache-Control: {cache_control}\r\nX-Content-Type-Options: nosniff\r\n{extra_headers}Connection: close\r\n\r\n"
    )
    .into_bytes()
}

/// 이름이 `.`으로 시작하지 않고 확장자가 표에 있으면 그 `Content-Type`입니다.
fn content_type_of(name: &str) -> Option<&'static str> {
    if name.starts_with('.') {
        return None;
    }
    let (_, extension) = name.rsplit_once('.')?;
    TYPES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, content_type)| *content_type)
}
