//! 이 기기가 기억하는 것입니다: 지난번에 쓴 이름, 서버마다 마지막으로 입장한 방 코드, 그 방에서 내 자리의 표식.
//!
//! 형식은 한 줄에 하나인 탭(`<TAB>`)으로 나눈 글자이고, 웹과 PC가 같은 글자를 씁니다. 저장하는 곳만 다릅니다
//! (웹은 `localStorage`의 `caravan.memory` 하나, PC는 설정 폴더의 `memory.txt`, `caravan_client`의 `storage.rs`).
//!
//! ```text
//! name<TAB>KANG
//! room<TAB>ws://127.0.0.1:7777<TAB>k7m2q
//! seat<TAB>ws://127.0.0.1:7777<TAB>k7m2q<TAB>00017f80abcdefff1020304050607008
//! ```
//!
//! 자리의 표식은 서버마다 마지막으로 입장한 방의 것 하나만 기억합니다. 접속이 끊긴 자리를 서버가 유지하는 시간은 60초이므로,
//! 그보다 오래된 표식은 쓸모가 없고 다른 방의 표식은 곧바로 바뀌기 때문입니다. 표식은 그 자리를 이어받는 유일한 자격이므로
//! 기기 밖으로 내보내지 않습니다(링크와 로그에 넣지 않고, `Debug`는 값을 보여 주지 않습니다).
//!
//! 저장된 글자는 믿을 수 없으므로, 읽을 수 없는 줄은 버리고 나머지를 씁니다.

use std::collections::BTreeMap;

use caravan_protocol::{PlayerName, RoomCode, SeatToken};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    name: Option<PlayerName>,
    /// 서버 주소마다 마지막으로 입장한 방 코드입니다.
    last_room: BTreeMap<String, RoomCode>,
    /// 서버 주소마다 마지막으로 입장한 방과 그 방의 내 자리의 표식입니다. `last_room`에 있는 방과 같은 방이거나, 스스로 나가서 없습니다.
    seats: BTreeMap<String, (RoomCode, SeatToken)>,
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
                ["seat", server, room, token] => {
                    if let (Ok(room), Some(token)) =
                        (RoomCode::parse(room), SeatToken::from_hex(token))
                    {
                        memory.seats.insert(server.to_string(), (room, token));
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
        for (server, (room, token)) in &self.seats {
            text.push_str(&format!("seat\t{server}\t{room}\t{}\n", token.to_hex()));
        }
        text
    }

    pub fn name(&self) -> Option<&PlayerName> {
        self.name.as_ref()
    }

    pub fn last_room(&self, server: &str) -> Option<&RoomCode> {
        self.last_room.get(server)
    }

    /// 이 서버의 `room`에서 내 자리의 표식입니다. 기억한 표식이 다른 방의 것이면 없습니다.
    pub fn seat(&self, server: &str, room: &RoomCode) -> Option<SeatToken> {
        self.seats
            .get(server)
            .filter(|(seat_room, _)| seat_room == room)
            .map(|&(_, token)| token)
    }

    /// 입장에 성공했습니다. 이름과, 이 서버의 마지막 방과, 그 방의 내 자리의 표식을 기억합니다. 이 서버의 이전 표식은 버립니다.
    /// 서버 주소에 제어 문자(탭, 줄바꿈 등)가 있으면 저장하는 줄의 모양이 깨지므로 방과 표식은 기억하지 않습니다.
    pub fn remember_join(
        &mut self,
        server: &str,
        name: &PlayerName,
        room: &RoomCode,
        token: SeatToken,
    ) {
        self.name = Some(name.clone());
        if !server.chars().any(char::is_control) {
            self.last_room.insert(server.to_string(), room.clone());
            self.seats.insert(server.to_string(), (room.clone(), token));
        }
    }

    /// 스스로 방을 나갔으므로 이 서버의 표식을 지웁니다. 서버가 자리를 곧바로 내놓으므로 그 표식은 더 이상 쓸 수 없습니다.
    /// 마지막 방 코드는 남겨서, 같은 방에 다시 들어갈 수 있게 합니다.
    pub fn forget_seat(&mut self, server: &str) {
        self.seats.remove(server);
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

    const TOKEN: SeatToken = SeatToken([7; 16]);
    const OTHER: SeatToken = SeatToken([9; 16]);

    #[test]
    fn what_is_remembered_comes_back_after_saving_and_loading() {
        let mut memory = Memory::default();
        memory.remember_join("ws://127.0.0.1:7777", &name("KANG"), &room("k7m2q"), TOKEN);
        memory.remember_join("ws://10.0.0.5:9000", &name("KANG"), &room("night"), OTHER);

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
        assert_eq!(
            loaded.seat("ws://127.0.0.1:7777", &room("k7m2q")),
            Some(TOKEN)
        );
        assert_eq!(
            loaded.seat("ws://10.0.0.5:9000", &room("night")),
            Some(OTHER),
            "each server keeps its own seat"
        );
    }

    #[test]
    fn a_seat_is_offered_only_for_the_room_it_belongs_to() {
        let mut memory = Memory::default();
        memory.remember_join("ws://a:1", &name("KANG"), &room("first"), TOKEN);

        assert_eq!(memory.seat("ws://a:1", &room("first")), Some(TOKEN));
        assert_eq!(
            memory.seat("ws://a:1", &room("second")),
            None,
            "the token opens nothing in another room"
        );
        assert_eq!(memory.seat("ws://b:2", &room("first")), None);

        memory.remember_join("ws://a:1", &name("KANG"), &room("second"), OTHER);
        assert_eq!(
            memory.seat("ws://a:1", &room("first")),
            None,
            "the seat of the last room replaces the one before"
        );
        assert_eq!(memory.seat("ws://a:1", &room("second")), Some(OTHER));
    }

    #[test]
    fn forgetting_the_seat_keeps_the_name_and_the_last_room() {
        let mut memory = Memory::default();
        memory.remember_join("ws://a:1", &name("KANG"), &room("first"), TOKEN);
        memory.remember_join("ws://b:2", &name("KANG"), &room("night"), OTHER);

        memory.forget_seat("ws://a:1");

        assert_eq!(memory.seat("ws://a:1", &room("first")), None);
        assert_eq!(
            memory.last_room("ws://a:1").map(RoomCode::as_str),
            Some("first")
        );
        assert_eq!(memory.name().map(PlayerName::as_str), Some("KANG"));
        assert_eq!(
            memory.seat("ws://b:2", &room("night")),
            Some(OTHER),
            "another server's seat stays"
        );
        let loaded = Memory::parse(&memory.to_text());
        assert_eq!(loaded, memory, "what was forgotten stays forgotten");
    }

    #[test]
    fn the_token_does_not_show_when_the_memory_is_printed() {
        let mut memory = Memory::default();
        memory.remember_join("ws://a:1", &name("KANG"), &room("first"), TOKEN);

        let shown = format!("{memory:?} {memory:#?}");

        assert!(!shown.contains(&TOKEN.to_hex()), "{shown}");
        assert!(shown.contains("SeatToken(..)"), "{shown}");
    }

    #[test]
    fn the_latest_join_replaces_the_name_and_the_room_of_that_server() {
        let mut memory = Memory::default();
        memory.remember_join("ws://a:1", &name("KANG"), &room("first"), TOKEN);
        memory.remember_join("ws://a:1", &name("MIRO"), &room("second"), OTHER);

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
seat\tws://a:1\tk7m2q\t07070707070707070707070707070707
seat\tws://b:2\tk7m2q\tnot a token
seat\tws://c:3\tnot a code!\t07070707070707070707070707070707
seat\tws://d:4\tk7m2q
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
        assert_eq!(memory.seat("ws://a:1", &room("k7m2q")), Some(TOKEN));
        for server in ["ws://b:2", "ws://c:3", "ws://d:4"] {
            assert_eq!(memory.seat(server, &room("k7m2q")), None, "{server}");
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
        memory.remember_join(evil, &name("KANG"), &room("k7m2q"), TOKEN);

        let loaded = Memory::parse(&memory.to_text());

        assert_eq!(
            loaded.name().map(PlayerName::as_str),
            Some("KANG"),
            "the address cannot add a line of its own"
        );
        assert_eq!(loaded.last_room(evil), None);
        assert_eq!(loaded.seat(evil, &room("k7m2q")), None);
        assert_eq!(loaded.to_text(), "name\tKANG\n");
    }
}
