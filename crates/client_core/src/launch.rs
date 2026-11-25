//! 방 코드와 서버 주소를 정하는 곳입니다.
//!
//! PC: `caravan-client --name KANG --room test --server ws://127.0.0.1:7777 --debug-net`
//! 웹: `http://127.0.0.1:8080/?room=test&name=KANG&server=ws://127.0.0.1:7777&debug-net`
//! 두 방식 모두 `from_values`에서 같은 규칙으로 검사합니다.

use std::fmt;

use caravan_protocol::{RoomCode, RoomCodeError};

pub const DEFAULT_ROOM: &str = "test";
pub const DEFAULT_SERVER: &str = "ws://127.0.0.1:7777";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchOptions {
    pub room: RoomCode,
    /// 인자에 있던 이름입니다. 이름은 입장할 때 검사합니다.
    pub name: Option<String>,
    pub server_url: String,
    /// 켜면 주고받는 메시지를 읽을 수 있는 형태로 로그에 출력합니다.
    pub debug_net: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchError {
    UnknownArgument(String),
    MissingValue(&'static str),
    BadRoomCode(RoomCodeError),
    BadServerUrl(String),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchError::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
            LaunchError::MissingValue(flag) => write!(f, "{flag} needs a value"),
            LaunchError::BadRoomCode(err) => write!(f, "{err}"),
            LaunchError::BadServerUrl(url) => {
                write!(
                    f,
                    "server address must start with ws:// or wss://, got {url}"
                )
            }
        }
    }
}

impl LaunchOptions {
    /// PC의 실행 인자를 해석합니다. 프로그램 이름은 빼고 넘깁니다.
    pub fn from_args(args: &[String]) -> Result<LaunchOptions, LaunchError> {
        let mut room = None;
        let mut name = None;
        let mut server = None;
        let mut debug_net = false;
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            match arg.as_str() {
                "--room" => {
                    room = Some(
                        rest.next()
                            .ok_or(LaunchError::MissingValue("--room"))?
                            .clone(),
                    )
                }
                "--name" => {
                    name = Some(
                        rest.next()
                            .ok_or(LaunchError::MissingValue("--name"))?
                            .clone(),
                    )
                }
                "--server" => {
                    server = Some(
                        rest.next()
                            .ok_or(LaunchError::MissingValue("--server"))?
                            .clone(),
                    )
                }
                "--debug-net" => debug_net = true,
                other => return Err(LaunchError::UnknownArgument(other.to_string())),
            }
        }
        LaunchOptions::from_values(room, name, server, debug_net)
    }

    /// 값이 없으면 기본값을 사용하고, 있으면 검사합니다.
    pub fn from_values(
        room: Option<String>,
        name: Option<String>,
        server: Option<String>,
        debug_net: bool,
    ) -> Result<LaunchOptions, LaunchError> {
        let room = RoomCode::parse(room.as_deref().unwrap_or(DEFAULT_ROOM))
            .map_err(LaunchError::BadRoomCode)?;
        let server_url = server.unwrap_or_else(|| DEFAULT_SERVER.to_string());
        if !(server_url.starts_with("ws://") || server_url.starts_with("wss://")) {
            return Err(LaunchError::BadServerUrl(server_url));
        }
        Ok(LaunchOptions {
            room,
            name,
            server_url,
            debug_net,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_means_room_test_on_the_local_server() {
        let options = LaunchOptions::from_args(&[]).unwrap();
        assert_eq!(options.room.as_str(), "test");
        assert_eq!(options.server_url, "ws://127.0.0.1:7777");
        assert!(!options.debug_net);
    }

    #[test]
    fn flags_set_room_server_and_debug_logging() {
        let options = LaunchOptions::from_args(&args(&[
            "--room",
            "Night",
            "--name",
            "KANG",
            "--server",
            "ws://10.0.0.5:9000",
            "--debug-net",
        ]))
        .unwrap();
        assert_eq!(options.room.as_str(), "night");
        assert_eq!(options.name.as_deref(), Some("KANG"));
        assert_eq!(options.server_url, "ws://10.0.0.5:9000");
        assert!(options.debug_net);
    }

    #[test]
    fn bad_input_is_reported_instead_of_guessed() {
        assert_eq!(
            LaunchOptions::from_args(&args(&["--room"])),
            Err(LaunchError::MissingValue("--room"))
        );
        assert_eq!(
            LaunchOptions::from_args(&args(&["--nickname", "A"])),
            Err(LaunchError::UnknownArgument("--nickname".to_string()))
        );
        assert_eq!(
            LaunchOptions::from_values(Some("my room".to_string()), None, None, false),
            Err(LaunchError::BadRoomCode(RoomCodeError::BadChar(' ')))
        );
        assert_eq!(
            LaunchOptions::from_values(
                None,
                None,
                Some("http://127.0.0.1:7777".to_string()),
                false
            ),
            Err(LaunchError::BadServerUrl(
                "http://127.0.0.1:7777".to_string()
            ))
        );
    }
}
