//! 이 기기가 기억하는 것입니다: 지난번에 쓴 이름과, 서버마다 마지막으로 입장한 방 코드.
//!
//! 형식은 한 줄에 하나인 탭(`<TAB>`)으로 나눈 글자이고, 웹과 PC가 같은 글자를 씁니다. 저장하는 곳만 다릅니다
//! (웹은 `localStorage`의 `caravan.memory` 하나, PC는 설정 폴더의 `memory.txt`, `caravan_client`의 `storage.rs`).
//!
//! ```text
//! name<TAB>KANG
//! room<TAB>ws://127.0.0.1:7777<TAB>k7m2q
//! ```
//!
//! 저장된 글자는 믿을 수 없으므로, 읽을 수 없는 줄은 버리고 나머지를 씁니다.

use std::collections::BTreeMap;

use caravan_protocol::{PlayerName, RoomCode};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    name: Option<PlayerName>,
    /// 서버 주소마다 마지막으로 입장한 방 코드입니다.
    last_room: BTreeMap<String, RoomCode>,
}

impl Memory {
    /// 저장된 글자를 읽습니다. 알 수 없는 줄, 칸의 수가 맞지 않는 줄, 값이 맞지 않는 줄은 버립니다.
    pub fn parse(text: &str) -> Memory {
        let mut memory = Memory::default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields[..] {
                ["name", name] => {
                    if let Ok(name) = PlayerName::parse(name) {
                        memory.name = Some(name);
                    }
                }
                ["room", server, room] => {
                    if let Ok(room) = RoomCode::parse(room) {
                        memory.last_room.insert(server.to_string(), room);
                    }
                }
                _ => {}
            }
        }
        memory
    }

    /// 저장할 글자입니다. `parse`가 같은 기억으로 되돌립니다.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        if let Some(name) = &self.name {
            text.push_str(&format!("name\t{name}\n"));
        }
        for (server, room) in &self.last_room {
            text.push_str(&format!("room\t{server}\t{room}\n"));
        }
        text
    }

    pub fn name(&self) -> Option<&PlayerName> {
        self.name.as_ref()
    }

    pub fn last_room(&self, server: &str) -> Option<&RoomCode> {
        self.last_room.get(server)
    }

    /// 입장에 성공했습니다. 이름과, 이 서버의 마지막 방을 기억합니다.
    /// 서버 주소에 제어 문자(탭, 줄바꿈 등)가 있으면 저장하는 줄의 모양이 깨지므로 방은 기억하지 않습니다.
    pub fn remember_join(&mut self, server: &str, name: &PlayerName, room: &RoomCode) {
        self.name = Some(name.clone());
        if !server.chars().any(char::is_control) {
            self.last_room.insert(server.to_string(), room.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(raw: &str) -> PlayerName {
        PlayerName::parse(raw).unwrap()
    }

    fn room(raw: &str) -> RoomCode {
        RoomCode::parse(raw).unwrap()
    }

    #[test]
    fn what_is_remembered_comes_back_after_saving_and_loading() {
        let mut memory = Memory::default();
        memory.remember_join("ws://127.0.0.1:7777", &name("KANG"), &room("k7m2q"));
        memory.remember_join("ws://10.0.0.5:9000", &name("KANG"), &room("night"));

        let loaded = Memory::parse(&memory.to_text());

        assert_eq!(loaded, memory);
        assert_eq!(loaded.name().map(PlayerName::as_str), Some("KANG"));
        assert_eq!(
            loaded
                .last_room("ws://127.0.0.1:7777")
                .map(RoomCode::as_str),
            Some("k7m2q")
        );
        assert_eq!(
            loaded.last_room("ws://10.0.0.5:9000").map(RoomCode::as_str),
            Some("night"),
            "each server keeps its own last room"
        );
        assert_eq!(loaded.last_room("ws://elsewhere:1"), None);
    }

    #[test]
    fn the_latest_join_replaces_the_name_and_the_room_of_that_server() {
        let mut memory = Memory::default();
        memory.remember_join("ws://a:1", &name("KANG"), &room("first"));
        memory.remember_join("ws://a:1", &name("MIRO"), &room("second"));

        assert_eq!(memory.name().map(PlayerName::as_str), Some("MIRO"));
        assert_eq!(
            memory.last_room("ws://a:1").map(RoomCode::as_str),
            Some("second")
        );
    }

    #[test]
    fn a_broken_line_is_dropped_and_the_rest_is_kept() {
        let text = "\
name\tKANG
garbage
room\tws://a:1\tk7m2q
room\tws://b:2\tnot a code!
room\tws://c:3
name\t
name\ttoo long a name for this
unknown\tx\ty
room\tws://d:4\tabcde\textra
";
        let memory = Memory::parse(text);

        assert_eq!(memory.name().map(PlayerName::as_str), Some("KANG"));
        assert_eq!(
            memory.last_room("ws://a:1").map(RoomCode::as_str),
            Some("k7m2q")
        );
        for server in ["ws://b:2", "ws://c:3", "ws://d:4"] {
            assert_eq!(memory.last_room(server), None, "{server}");
        }
    }

    #[test]
    fn empty_or_unreadable_text_is_an_empty_memory() {
        assert_eq!(Memory::parse(""), Memory::default());
        assert_eq!(Memory::parse("\u{0}\u{1}binary\u{ff}"), Memory::default());
    }

    #[test]
    fn a_server_address_that_would_break_the_line_is_not_remembered() {
        let mut memory = Memory::default();
        let evil = "ws://evil\nname\tHACKER\n";
        memory.remember_join(evil, &name("KANG"), &room("k7m2q"));

        let loaded = Memory::parse(&memory.to_text());

        assert_eq!(
            loaded.name().map(PlayerName::as_str),
            Some("KANG"),
            "the address cannot add a line of its own"
        );
        assert_eq!(loaded.last_room(evil), None);
        assert_eq!(loaded.to_text(), "name\tKANG\n");
    }
}
