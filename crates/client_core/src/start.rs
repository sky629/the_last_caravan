//! 시작 화면의 상태입니다. Bevy를 모르므로 키 하나를 넣고 결과를 보는 테스트로 확인합니다.
//! 화면을 그리는 것과 Bevy의 키보드 이벤트를 `FormKey`로 바꾸는 것은 `caravan_client`의 `start.rs`가 합니다.

use caravan_protocol::{PlayerName, RoomCode};

/// 입력 칸입니다. 칸마다 받는 문자와 길이의 상한이 다릅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Room,
}

impl Field {
    fn max_chars(self) -> usize {
        match self {
            Field::Name => PlayerName::MAX_CHARS,
            Field::Room => RoomCode::MAX_LEN,
        }
    }

    /// 입력한 문자를 칸에 넣을 문자로 바꿉니다. 칸이 받지 않는 문자이면 `None`입니다.
    /// 방 코드는 대문자를 소문자로 바꿔서 넣으므로, 칸에 보이는 글자가 서버가 쓰는 코드와 같습니다.
    fn accept(self, c: char) -> Option<char> {
        match self {
            Field::Name => PlayerName::allows(c).then_some(c),
            Field::Room => {
                let c = c.to_ascii_lowercase();
                RoomCode::allows(c).then_some(c)
            }
        }
    }
}

/// 시작 화면이 받는 키입니다. 글자는 Bevy가 알려 준 문자 그대로이고, 칸이 받지 않는 문자는 칸이 거릅니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormKey {
    Char(char),
    Backspace,
    /// 다음 칸으로 갑니다. 마지막 칸이면 첫 칸으로 돌아갑니다.
    Tab,
    Enter,
    /// 클립보드에서 읽은 글자입니다. 한 글자씩 넣은 것과 같습니다.
    Paste(String),
}

/// 글자 입력 칸 하나입니다. 글자는 끝에만 더하고 끝에서만 지웁니다(커서 이동은 없습니다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextField {
    field: Field,
    text: String,
}

impl TextField {
    /// `text`에서 칸이 받는 글자만 상한까지 넣은 칸입니다. 기기에 저장된 값과 주소의 값은 믿을 수 없으므로 거릅니다.
    pub fn new(field: Field, text: &str) -> TextField {
        let mut input = TextField {
            field,
            text: String::new(),
        };
        text.chars().for_each(|c| input.push(c));
        input
    }

    /// 칸이 받지 않는 글자와 상한을 넘는 글자는 버립니다.
    pub fn push(&mut self, c: char) {
        if self.text.chars().count() >= self.field.max_chars() {
            return;
        }
        if let Some(c) = self.field.accept(c) {
            self.text.push(c);
        }
    }

    pub fn backspace(&mut self) {
        self.text.pop();
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// 입장을 요청하는 값입니다. 시작 화면이 검사를 마친 값만 담습니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinRequest {
    pub name: PlayerName,
    /// 없으면 서버가 새 방을 만듭니다.
    pub room: Option<RoomCode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartForm {
    pub name: TextField,
    pub room: TextField,
    pub focus: Field,
    /// 칸 아래에 보이는 한 줄입니다. 거절된 이유, 끊긴 이유, 잘못 입력한 값의 이유입니다.
    pub notice: Option<String>,
}

impl StartForm {
    /// 칸을 미리 채운 시작 화면입니다. 이름 칸이 비어 있으면 이름 칸에서, 아니면 방 코드 칸에서 시작합니다.
    pub fn new(name: &str, room: &str, notice: Option<String>) -> StartForm {
        let name = TextField::new(Field::Name, name);
        let focus = if name.text().trim().is_empty() {
            Field::Name
        } else {
            Field::Room
        };
        StartForm {
            name,
            room: TextField::new(Field::Room, room),
            focus,
            notice,
        }
    }

    /// Enter가 하는 일의 안내입니다. 방 코드 칸이 비어 있으면 새 방을 만든다고 알려 줍니다.
    pub fn enter_hint(&self) -> &'static str {
        if self.room.text().trim().is_empty() {
            "ENTER  CREATE A NEW ROOM"
        } else {
            "ENTER  JOIN"
        }
    }

    fn focused(&mut self) -> &mut TextField {
        match self.focus {
            Field::Name => &mut self.name,
            Field::Room => &mut self.room,
        }
    }

    /// 키 하나를 처리합니다. Enter에서 두 칸이 모두 맞으면 입장 요청을 돌려주고, 맞지 않으면 `notice`에 이유를 적고 틀린 칸으로 갑니다.
    pub fn press(&mut self, key: FormKey) -> Option<JoinRequest> {
        match key {
            FormKey::Char(c) => {
                self.notice = None;
                self.focused().push(c);
            }
            FormKey::Paste(text) => {
                self.notice = None;
                text.chars().for_each(|c| self.focused().push(c));
            }
            FormKey::Backspace => self.focused().backspace(),
            FormKey::Tab => {
                self.focus = match self.focus {
                    Field::Name => Field::Room,
                    Field::Room => Field::Name,
                };
            }
            FormKey::Enter => return self.submit(),
        }
        None
    }

    fn submit(&mut self) -> Option<JoinRequest> {
        let name = match PlayerName::parse(self.name.text()) {
            Ok(name) => name,
            Err(err) => {
                self.notice = Some(err.to_string());
                self.focus = Field::Name;
                return None;
            }
        };
        let room = if self.room.text().trim().is_empty() {
            None
        } else {
            match RoomCode::parse(self.room.text()) {
                Ok(room) => Some(room),
                Err(err) => {
                    self.notice = Some(err.to_string());
                    self.focus = Field::Room;
                    return None;
                }
            }
        };
        Some(JoinRequest { name, room })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_text(form: &mut StartForm, text: &str) {
        for c in text.chars() {
            form.press(FormKey::Char(c));
        }
    }

    #[test]
    fn a_field_takes_only_the_letters_its_kind_allows() {
        let mut name = TextField::new(Field::Name, "");
        for c in "Ka ng_1-!한\t\n".chars() {
            name.push(c);
        }
        assert_eq!(name.text(), "Ka ng_1-");

        let mut room = TextField::new(Field::Room, "");
        for c in "K7m 2Q_-!한".chars() {
            room.push(c);
        }
        assert_eq!(
            room.text(),
            "k7m2q_-",
            "a room code is lowercase and has no spaces"
        );
    }

    #[test]
    fn a_field_stops_at_its_limit() {
        let mut name = TextField::new(Field::Name, "");
        "ABCDEFGHIJKLMNOP".chars().for_each(|c| name.push(c));
        assert_eq!(name.text(), "ABCDEFGHIJKL", "12 letters");

        let mut room = TextField::new(Field::Room, "");
        "abcdefghijklmnopqrstuvwxyz"
            .chars()
            .for_each(|c| room.push(c));
        assert_eq!(room.text(), "abcdefghijklmnop", "16 letters");
    }

    #[test]
    fn a_value_from_the_device_is_filtered_like_typing() {
        let name = TextField::new(Field::Name, "KANG\n\t한글!!!");
        assert_eq!(name.text(), "KANG");
        let room = TextField::new(Field::Room, "../etc/passwd");
        assert_eq!(room.text(), "etcpasswd");
    }

    #[test]
    fn backspace_removes_the_last_letter_and_nothing_from_an_empty_field() {
        let mut field = TextField::new(Field::Name, "AB");
        field.backspace();
        assert_eq!(field.text(), "A");
        field.backspace();
        field.backspace();
        assert_eq!(field.text(), "");
    }

    #[test]
    fn the_form_starts_on_the_name_when_it_is_empty_and_on_the_room_otherwise() {
        assert_eq!(StartForm::new("", "", None).focus, Field::Name);
        assert_eq!(StartForm::new("  ", "k7m2q", None).focus, Field::Name);
        assert_eq!(StartForm::new("KANG", "", None).focus, Field::Room);
    }

    #[test]
    fn typing_goes_to_the_focused_field_and_tab_moves_between_fields() {
        let mut form = StartForm::new("", "", None);
        type_text(&mut form, "KANG");
        form.press(FormKey::Tab);
        type_text(&mut form, "K7M2Q");
        assert_eq!((form.name.text(), form.room.text()), ("KANG", "k7m2q"));

        form.press(FormKey::Backspace);
        assert_eq!(
            form.room.text(),
            "k7m2",
            "backspace works on the focused field"
        );
        form.press(FormKey::Tab);
        assert_eq!(form.focus, Field::Name, "tab goes round");
        form.press(FormKey::Backspace);
        assert_eq!(form.name.text(), "KAN");
    }

    #[test]
    fn a_paste_goes_in_letter_by_letter_through_the_same_filter() {
        let mut form = StartForm::new("KANG", "", None);
        assert_eq!(form.focus, Field::Room);
        form.press(FormKey::Paste("  K7M-2Q!\n".to_string()));
        assert_eq!(form.room.text(), "k7m-2q");
        form.press(FormKey::Paste("x".repeat(40)));
        assert_eq!(
            form.room.text().len(),
            16,
            "a long paste stops at the limit"
        );
    }

    #[test]
    fn enter_with_a_name_and_a_room_asks_to_join_that_room() {
        let mut form = StartForm::new("  KANG ", "K7M2Q", None);
        let request = form.press(FormKey::Enter).expect("both fields are fine");
        assert_eq!(request.name.as_str(), "KANG");
        assert_eq!(request.room.as_ref().map(RoomCode::as_str), Some("k7m2q"));
    }

    #[test]
    fn enter_with_a_blank_room_asks_for_a_new_room() {
        let mut form = StartForm::new("KANG", "", None);
        let request = form.press(FormKey::Enter).expect("a blank room is allowed");
        assert_eq!(request.room, None);
    }

    #[test]
    fn enter_without_a_name_says_so_and_moves_to_the_name() {
        let mut form = StartForm::new("   ", "k7m2q", None);
        form.focus = Field::Room;
        assert_eq!(form.press(FormKey::Enter), None);
        assert_eq!(form.notice.as_deref(), Some("Enter a name"));
        assert_eq!(form.focus, Field::Name);
        assert_eq!(form.room.text(), "k7m2q", "what was typed stays");
    }

    #[test]
    fn the_enter_hint_says_whether_a_room_is_made_or_joined() {
        assert_eq!(
            StartForm::new("KANG", "", None).enter_hint(),
            "ENTER  CREATE A NEW ROOM"
        );
        assert_eq!(
            StartForm::new("KANG", "k7m2q", None).enter_hint(),
            "ENTER  JOIN"
        );
    }

    #[test]
    fn typing_clears_the_notice_but_moving_does_not() {
        let mut form = StartForm::new("", "", None);
        form.press(FormKey::Enter);
        assert!(form.notice.is_some());
        form.press(FormKey::Tab);
        assert!(form.notice.is_some());
        form.press(FormKey::Char('a'));
        assert_eq!(form.notice, None);
    }
}
