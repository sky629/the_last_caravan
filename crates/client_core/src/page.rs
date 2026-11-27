//! 웹 페이지가 열린 곳에서 정해지는 두 가지입니다. `?server=`가 없을 때 접속할 서버의 주소와, 친구에게 보낼 링크의 앞부분입니다.
//!
//! 페이지는 두 곳에서 열립니다.
//! - 게임 서버가 내준 페이지(`http://127.0.0.1:7777/`, 터널의 `https://....trycloudflare.com/`): 서버가 html의 공개 주소 자리를 채워서 내줍니다.
//!   페이지를 연 주소가 곧 서버이므로 `https://`면 `wss://`로, `http://`면 `ws://`로 같은 주소에 접속합니다.
//!   서버가 공개 주소를 알고 있으면 어느 주소로 열었든 초대 링크는 공개 주소로 만듭니다.
//! - `trunk serve`가 내준 페이지: 공개 주소 자리가 채워지지 않은 채 남아 있습니다. 서버가 따로 있으므로 PC 클라이언트와 같은 기본 서버(`DEFAULT_SERVER`)에 접속합니다.

use crate::launch::DEFAULT_SERVER;

/// 웹 페이지가 열린 곳에서 정해진 값입니다. PC에는 페이지가 없으므로 없습니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// `?server=`가 없을 때 접속할 서버입니다. 초대 링크는 접속한 서버가 이 서버와 다를 때만 `&server=`를 붙입니다.
    pub default_server: String,
    /// 친구에게 보낼 링크의 앞부분입니다(쿼리 없는 페이지 주소).
    pub invite_url: String,
}

impl Page {
    /// `origin`은 페이지 주소의 `https://host:port`, `pathname`은 그 경로(`/`)입니다.
    /// `marker`는 html의 공개 주소 자리에 있는 값입니다. 자리가 없거나 값이 주소도 빈 글자도 아니면(채워지지 않은 채 남은 표지)
    /// 게임 서버가 내준 페이지가 아닙니다.
    pub fn open(origin: &str, pathname: &str, marker: Option<&str>) -> Page {
        let served = marker.filter(|value| {
            value.is_empty() || value.starts_with("https://") || value.starts_with("http://")
        });
        let default_server = served
            .and_then(|_| socket_url_of(origin))
            .unwrap_or_else(|| DEFAULT_SERVER.to_string());
        let base = served.filter(|public| !public.is_empty()).unwrap_or(origin);
        Page {
            default_server,
            invite_url: format!("{base}{pathname}"),
        }
    }
}

/// 같은 주소의 WebSocket 주소입니다. `http(s)`가 아닌 페이지(`file://`의 `null`)는 없습니다.
fn socket_url_of(origin: &str) -> Option<String> {
    if let Some(host) = origin.strip_prefix("https://") {
        Some(format!("wss://{host}"))
    } else {
        origin
            .strip_prefix("http://")
            .map(|host| format!("ws://{host}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUNNEL: &str = "https://quiet-river-1234.trycloudflare.com";

    #[test]
    fn a_page_the_game_server_served_connects_to_its_own_address() {
        let https = Page::open(TUNNEL, "/", Some(TUNNEL));
        assert_eq!(
            https.default_server,
            "wss://quiet-river-1234.trycloudflare.com"
        );
        let http = Page::open("http://192.168.0.5:7777", "/", Some(""));
        assert_eq!(
            http.default_server, "ws://192.168.0.5:7777",
            "the port stays"
        );
        assert_eq!(http.invite_url, "http://192.168.0.5:7777/");
    }

    #[test]
    fn a_page_from_trunk_serve_keeps_the_default_server() {
        for marker in [None, Some("{{public_url}}"), Some("quiet-river")] {
            let page = Page::open("http://127.0.0.1:8080", "/", marker);
            assert_eq!(page.default_server, DEFAULT_SERVER, "{marker:?}");
            assert_eq!(page.invite_url, "http://127.0.0.1:8080/", "{marker:?}");
        }
        assert_eq!(
            Page::open("null", "/index.html", Some("")).default_server,
            DEFAULT_SERVER,
            "a page that has no address to connect back to"
        );
    }

    #[test]
    fn the_invite_of_a_page_opened_on_this_computer_points_at_the_public_address() {
        let page = Page::open("http://127.0.0.1:7777", "/", Some(TUNNEL));
        assert_eq!(
            page.default_server, "ws://127.0.0.1:7777",
            "this page plays on this computer"
        );
        assert_eq!(page.invite_url, format!("{TUNNEL}/"));

        let without = Page::open("http://127.0.0.1:7777", "/", Some(""));
        assert_eq!(
            without.invite_url, "http://127.0.0.1:7777/",
            "no public address: the page itself"
        );
    }
}
