//! 클라이언트의 화면 흐름입니다. 시작 화면과 방 안의 접속 상태 가운데 하나입니다.
//!
//! ```text
//!   Start --Enter(맞는 값)--> Playing(Connecting -> Joining -> InRoom)
//!   Playing --거절--> Start(입력한 값과 거절된 이유)
//!   Playing --끊김, 접속 실패, 자리를 다른 창에 내줌--> Start(들어가 있던 방 코드와 이유)
//!   Playing(방에 들어감) --Esc--> 나가기 확인 창 --Enter--> Start(들어가 있던 방 코드)
//!   Playing(접속하는 중, 입장하는 중) --Esc--> Start(들어가 있던 방 코드)
//!   (시작할 때 이름과 방 코드가 모두 주어졌으면 Start 없이 Playing)
//! ```
//!
//! 입장하면 서버가 알려 준 자리의 표식을 기기에 저장합니다. 끊겨서 시작 화면으로 돌아온 뒤 같은 방 코드로 입장하면(방 코드를 비우고 방을
//! 만들었어도 칸에 서버가 정한 코드가 채워집니다) 그 표식을 `Join`에 실어서, 서버가 60초 동안 유지하는 같은 캐릭터로 돌아갑니다.
//! 확인 창에서 Enter로 나가는 것은 스스로 나가는 것이므로 서버에 `Leave`를 보내서 자리를 곧바로 내놓고 표식을 지웁니다.
//!
//! 이 파일은 접속 방식, 저장소, 클립보드를 직접 다루지 않고 해야 할 일을 `Command`로 돌려줍니다.
//! 그래서 Bevy 없이 키와 이벤트를 넣고 돌아온 `Command`를 보는 테스트로 확인합니다.

use caravan_protocol::{ClientEvent, ClientMsg, PlayerName, RejectReason};

use crate::hud::{InviteKind, reject_text};
use crate::input::Controls;
use crate::launch::{DEFAULT_SERVER, LaunchError, LaunchOptions};
use crate::leave::{PromptStep, RoomKey};
use crate::memory::Memory;
use crate::page::Page;
use crate::session::Session;
use crate::start::{Field, FormKey, JoinRequest, StartForm};

/// 클라이언트가 실행할 일입니다. `caravan_client`의 `net.rs`가 순서대로 실행합니다.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// 서버에 새 접속을 엽니다. 열리면 `Session`이 `Join`을 보냅니다.
    Connect {
        server: String,
    },
    Send(ClientMsg),
    /// 지금의 접속을 버립니다.
    Disconnect,
    /// 기억할 것이 바뀌었습니다. `Memory::to_text`의 글자를 저장합니다.
    Save(String),
    /// 클립보드에 씁니다. 웹은 키를 누른 같은 프레임에 써야 브라우저가 허락합니다.
    Copy(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    Start(StartForm),
    Playing(Session),
}

pub struct Flow {
    screen: Screen,
    /// 접속할 서버입니다. 실행하는 동안 바뀌지 않습니다.
    server: String,
    /// 실행 인자가 틀렸으면 그 이유입니다. 틀린 인자로 다른 서버에 접속하지 않으므로, 이유가 있으면 입장하지 않습니다.
    launch_error: Option<String>,
    /// 웹 페이지가 열린 곳입니다. 친구에게 보낼 링크의 앞부분과, 그 링크를 여는 사람이 기본으로 접속할 서버가 여기서 나옵니다.
    /// PC에서는 없고, 방 코드만 복사합니다.
    page: Option<Page>,
    memory: Memory,
    /// 마지막으로 입장을 요청한 값입니다. 입장하면 방 코드가 서버가 정한 코드로 바뀝니다. 시작 화면으로 돌아올 때 칸을 다시 채웁니다.
    attempt: Option<JoinRequest>,
}

impl Flow {
    /// 실행 인자(또는 웹 주소)와 기기에 기억한 값으로 시작합니다.
    /// 방 코드 칸: 인자의 방 코드, 없으면 이 서버에서 마지막으로 입장한 방 코드, 없으면 빈 칸.
    /// 이름 칸: 인자의 이름, 없으면 지난번에 쓴 이름, 없으면 빈 칸.
    /// 인자에 이름과 방 코드가 모두 있고 이름이 맞으면 시작 화면 없이 곧바로 입장합니다(`Connect`를 돌려줍니다).
    pub fn new(
        launch: Result<LaunchOptions, LaunchError>,
        memory: Memory,
        page: Option<Page>,
    ) -> (Flow, Vec<Command>) {
        let (launch, launch_error) = match launch {
            Ok(launch) => (launch, None),
            Err(err) => (
                LaunchOptions {
                    room: None,
                    name: None,
                    server_url: DEFAULT_SERVER.to_string(),
                    debug_net: false,
                    profile: None,
                },
                Some(err.to_string()),
            ),
        };
        let room = launch
            .room
            .as_ref()
            .or_else(|| memory.last_room(&launch.server_url))
            .map_or("", |room| room.as_str())
            .to_string();
        let name = launch
            .name
            .as_deref()
            .or_else(|| memory.name().map(|name| name.as_str()))
            .unwrap_or("")
            .to_string();
        // 인자의 이름은 칸에 넣을 때 틀린 글자가 조용히 사라지므로, 칸을 채우기 전에 따로 검사해서 이유를 보여 줍니다.
        let name_error = launch
            .name
            .as_deref()
            .and_then(|raw| PlayerName::parse(raw).err())
            .map(|err| err.to_string());
        let direct = launch.name.is_some() && launch.room.is_some() && name_error.is_none();
        let mut form = StartForm::new(&name, &room, launch_error.clone().or(name_error.clone()));
        if name_error.is_some() {
            // 고칠 것은 이름이므로 입력 위치를 이름 칸에 둡니다. 이름이 남아 있으면 방 코드 칸에 놓이기 때문입니다.
            form.focus = Field::Name;
        }
        let mut flow = Flow {
            screen: Screen::Start(form),
            server: launch.server_url,
            launch_error,
            page,
            memory,
            attempt: None,
        };
        // 칸을 채운 시작 화면에서 Enter를 누른 것과 같습니다.
        let commands = if direct {
            flow.on_key(FormKey::Enter)
        } else {
            Vec::new()
        };
        (flow, commands)
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn session(&self) -> Option<&Session> {
        match &self.screen {
            Screen::Playing(session) => Some(session),
            Screen::Start(_) => None,
        }
    }

    pub fn session_mut(&mut self) -> Option<&mut Session> {
        match &mut self.screen {
            Screen::Playing(session) => Some(session),
            Screen::Start(_) => None,
        }
    }

    pub fn form(&self) -> Option<&StartForm> {
        match &self.screen {
            Screen::Start(form) => Some(form),
            Screen::Playing(_) => None,
        }
    }

    /// 시작 화면의 키입니다. 방 안에서는 아무 일도 하지 않습니다.
    pub fn on_key(&mut self, key: FormKey) -> Vec<Command> {
        let Screen::Start(form) = &mut self.screen else {
            return Vec::new();
        };
        let request = form.press(key);
        match request {
            Some(request) => self.connect(request),
            None => Vec::new(),
        }
    }

    /// 클립보드를 읽지 못해서 붙여 넣지 못했습니다. 시작 화면에 알립니다.
    pub fn on_paste_failed(&mut self) {
        if let Screen::Start(form) = &mut self.screen {
            form.notice = Some("Cannot read the clipboard. Type it instead".to_string());
        }
    }

    /// 접속 방식의 이벤트입니다.
    /// - `Session::on_event`가 돌려준 답을 `Send`로 바꿉니다.
    /// - 입장하면(`Welcome`) 이름과 방 코드를 기억하고 `Save`를 돌려줍니다.
    /// - 세션이 끝났으면(거절, 끊김, 접속 실패) `Disconnect`와 함께 이유가 적힌 시작 화면으로 돌아갑니다.
    pub fn on_event(&mut self, event: ClientEvent) -> Vec<Command> {
        let Screen::Playing(session) = &mut self.screen else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        let was_in_room = session.joined().is_some();
        if let Some(reply) = session.on_event(event) {
            commands.push(Command::Send(reply));
        }
        if !was_in_room && let Some(joined) = session.joined() {
            self.memory.remember_join(
                &self.server,
                &joined.name,
                &joined.room,
                joined.welcome.token,
            );
            if let Some(attempt) = &mut self.attempt {
                attempt.room = Some(joined.room.clone());
            }
            commands.push(Command::Save(self.memory.to_text()));
        }
        if session.is_over() {
            let notice = match &*session {
                Session::Rejected { reason } => reject_text(*reason),
                other => other.status_text(),
            };
            let rejected_name = matches!(
                &*session,
                Session::Rejected {
                    reason: RejectReason::BadName
                }
            );
            self.back_to_start(Some(notice));
            if rejected_name && let Screen::Start(form) = &mut self.screen {
                form.focus = Field::Name;
            }
            commands.push(Command::Disconnect);
        }
        commands
    }

    /// 프레임마다 방 안의 키를 처리합니다. `key`는 이 프레임에 누른 Esc나 Enter이고(둘을 함께 눌렀으면 Esc), `controls`는 이 프레임에 읽은 조작입니다.
    /// 나가기 확인 창이 이 프레임에 떠 있었다면, 곧 프레임이 시작될 때나 `key`를 처리한 뒤에 떠 있다면, 이 프레임의 Enter는 창의 것입니다.
    /// 그래서 `controls`의 확인을 지워서 결과 화면의 확인으로 서버에 가지 않게 합니다. Esc가 창을 닫은 프레임의 Enter도 같습니다.
    pub fn on_room_frame(&mut self, key: Option<RoomKey>, controls: &mut Controls) -> Vec<Command> {
        let prompt_was_open = self.leave_prompt_open();
        let commands = key.map_or_else(Vec::new, |key| self.on_room_key(key));
        if prompt_was_open || self.leave_prompt_open() {
            controls.confirm = false;
        }
        commands
    }

    /// 나가기 확인 창이 떠 있는지 봅니다. 방에 들어가기 전에는 창이 없습니다.
    fn leave_prompt_open(&self) -> bool {
        self.session()
            .and_then(Session::joined)
            .is_some_and(|joined| joined.leave_prompt.is_open())
    }

    /// 방 안에서 Esc나 Enter를 눌렀습니다. 방에 들어간 뒤에는 나가기 확인 창(`LeavePrompt`)이 키를 읽습니다.
    /// Esc가 창을 열고 닫고, 창이 떠 있을 때의 Enter가 방을 나갑니다. 창이 닫혀 있을 때의 Enter는 아무 일도 하지 않습니다.
    /// 방에 들어가기 전(접속하는 중, 입장하는 중)에는 잃을 것이 없으므로 Esc가 확인 없이 나가고, Enter는 아무 일도 하지 않습니다.
    fn on_room_key(&mut self, key: RoomKey) -> Vec<Command> {
        let Screen::Playing(session) = &mut self.screen else {
            return Vec::new();
        };
        match session {
            Session::InRoom(joined) => match joined.leave_prompt.press(key) {
                PromptStep::Stay(prompt) => {
                    joined.leave_prompt = prompt;
                    Vec::new()
                }
                PromptStep::Leave => self.leave(),
            },
            _ if key == RoomKey::Escape => self.leave(),
            _ => Vec::new(),
        }
    }

    /// 확인 없이 방을 나갑니다. 사람이 키로 나가는 길은 확인 창을 거치는 `on_room_frame`입니다.
    /// 접속을 버리고, 들어가 있던 방 코드가 채워진 시작 화면으로 돌아갑니다.
    /// 입장 요청(`Join`)을 보낸 뒤였으면 서버에 `Leave`를 먼저 보내서 자리를 곧바로 내놓고, 이 서버의 표식을 지웁니다.
    /// 서버는 `Join` 뒤의 `Leave`를 순서대로 받으므로, 입장 안내가 오기 전에 나가도 자리가 남지 않습니다.
    /// 접속이 열리기 전에는 서버가 아무것도 모르므로 접속만 버립니다.
    pub fn leave(&mut self) -> Vec<Command> {
        let Screen::Playing(session) = &self.screen else {
            return Vec::new();
        };
        let join_sent = matches!(session, Session::Joining { .. } | Session::InRoom(_));
        self.back_to_start(None);
        let mut commands = Vec::new();
        if join_sent {
            commands.push(Command::Send(ClientMsg::Leave));
        }
        commands.push(Command::Disconnect);
        if join_sent {
            self.memory.forget_seat(&self.server);
            commands.push(Command::Save(self.memory.to_text()));
        }
        commands
    }

    /// C로 복사하는 것의 종류입니다. 페이지 주소가 있는 웹은 링크이고, PC는 방 코드입니다.
    pub fn invite_kind(&self) -> InviteKind {
        match self.page {
            Some(_) => InviteKind::Link,
            None => InviteKind::Code,
        }
    }

    /// 야영지에서 C입니다. 웹은 `<페이지 주소>?room=<코드>`를, PC는 방 코드를 복사합니다. 접속한 서버가 그 페이지의 기본 서버와 다르면 `&server=`를 붙입니다.
    /// 방에 들어가 있지 않으면 없습니다.
    pub fn copy_invite(&self) -> Option<Command> {
        let joined = self.session()?.joined()?;
        let text = match &self.page {
            Some(page) => {
                let mut link = format!("{}?room={}", page.invite_url, joined.room);
                if self.server != page.default_server {
                    link.push_str("&server=");
                    link.push_str(&encode_query_value(&self.server));
                }
                link
            }
            None => joined.room.to_string(),
        };
        Some(Command::Copy(text))
    }

    fn connect(&mut self, request: JoinRequest) -> Vec<Command> {
        if let Some(reason) = &self.launch_error {
            if let Screen::Start(form) = &mut self.screen {
                form.notice = Some(reason.clone());
            }
            return Vec::new();
        }
        self.attempt = Some(request.clone());
        // 이 기기가 이 서버의 그 방에서 기억하는 자리가 있으면 그 자리로 돌아갑니다. 방 코드가 비어 있으면 새 방이므로 돌아갈 자리가 없습니다.
        let token = request
            .room
            .as_ref()
            .and_then(|room| self.memory.seat(&self.server, room));
        self.screen = Screen::Playing(Session::new(request, self.server.clone(), token));
        vec![Command::Connect {
            server: self.server.clone(),
        }]
    }

    /// 시작 화면으로 돌아갑니다. 마지막으로 입장을 요청한 이름과 방 코드를 칸에 채웁니다.
    fn back_to_start(&mut self, notice: Option<String>) {
        let (name, room) = match &self.attempt {
            Some(attempt) => (
                attempt.name.as_str(),
                attempt.room.as_ref().map_or("", |room| room.as_str()),
            ),
            None => ("", ""),
        };
        self.screen = Screen::Start(StartForm::new(name, room, notice));
    }
}

/// 주소의 쿼리 값으로 쓸 수 있게 바꿉니다. 읽기 쉽도록 서버 주소에 흔한 `:`와 `/`는 그대로 둡니다.
fn encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' | b'/' => {
                encoded.push(char::from(byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use caravan_protocol::{PROTOCOL_VERSION, RoomCode, SeatToken, ServerMsg};
    use caravan_rules::{Seed, Sim, Tuning};

    use super::*;

    const SERVER: &str = "ws://127.0.0.1:7777";
    /// `remembered`가 기억해 둔 자리의 표식입니다.
    const MEMORISED_TOKEN: SeatToken = SeatToken([3; 16]);
    /// `welcome`이 알려 주는 자리의 표식입니다.
    const WELCOMED_TOKEN: SeatToken = SeatToken([7; 16]);

    fn launch(room: Option<&str>, name: Option<&str>) -> Result<LaunchOptions, LaunchError> {
        LaunchOptions::from_values(
            room.map(str::to_string),
            name.map(str::to_string),
            None,
            false,
        )
    }

    fn remembered(name: &str, server: &str, room: &str) -> Memory {
        let mut memory = Memory::default();
        memory.remember_join(
            server,
            &PlayerName::parse(name).unwrap(),
            &RoomCode::parse(room).unwrap(),
            MEMORISED_TOKEN,
        );
        memory
    }

    fn start(launch: Result<LaunchOptions, LaunchError>, memory: Memory) -> Flow {
        Flow::new(launch, memory, None).0
    }

    fn form(flow: &Flow) -> &StartForm {
        flow.form().expect("the start screen")
    }

    fn type_text(flow: &mut Flow, text: &str) {
        for c in text.chars() {
            flow.on_key(FormKey::Char(c));
        }
    }

    fn welcome(room: &str) -> ServerMsg {
        let mut sim = Sim::new(Tuning::default(), Seed(1));
        let first = sim.add_player().unwrap();
        let room = RoomCode::parse(room).unwrap();
        let welcome = caravan_protocol::Welcome::from_sim(
            &sim,
            sim.player(first).unwrap(),
            &room,
            WELCOMED_TOKEN,
        );
        ServerMsg::Welcome(Box::new(welcome))
    }

    fn rejected(reason: RejectReason) -> ClientEvent {
        ClientEvent::Message(ServerMsg::JoinRejected { reason })
    }

    /// 이름을 넣고 Enter를 눌러서 접속이 열릴 때까지 갑니다.
    fn connected(flow: &mut Flow, name: &str, room: &str) -> Vec<Command> {
        type_text(flow, name);
        flow.on_key(FormKey::Tab);
        type_text(flow, room);
        flow.on_key(FormKey::Enter);
        flow.on_event(ClientEvent::Connected)
    }

    fn joined(room: &str) -> Flow {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", room);
        flow.on_event(ClientEvent::Message(welcome(if room.is_empty() {
            "k7m2q"
        } else {
            room
        })));
        flow
    }

    #[test]
    fn typing_a_name_entering_and_a_rejection_return_to_the_form_with_the_reason() {
        let mut flow = start(launch(None, None), Memory::default());
        assert_eq!(form(&flow).focus, Field::Name, "no name yet");

        type_text(&mut flow, "KANG");
        flow.on_key(FormKey::Tab);
        type_text(&mut flow, "K7M2Q");
        let commands = flow.on_key(FormKey::Enter);
        assert_eq!(
            commands,
            vec![Command::Connect {
                server: SERVER.to_string()
            }]
        );
        assert!(matches!(flow.screen(), Screen::Playing(_)));

        let commands = flow.on_event(ClientEvent::Connected);
        assert_eq!(
            commands,
            vec![Command::Send(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: Some("k7m2q".to_string()),
                name: "KANG".to_string(),
                token: None,
            })]
        );

        let commands = flow.on_event(rejected(RejectReason::RoomFull { capacity: 4 }));
        assert_eq!(commands, vec![Command::Disconnect]);
        let form = form(&flow);
        assert_eq!(form.name.text(), "KANG", "what was typed stays");
        assert_eq!(form.room.text(), "k7m2q");
        assert_eq!(form.notice.as_deref(), Some("Room is full (4 / 4)"));
    }

    #[test]
    fn a_blank_room_sends_no_room_code_and_a_rejection_keeps_it_blank() {
        let mut flow = start(launch(None, None), Memory::default());
        let commands = connected(&mut flow, "KANG", "");
        assert_eq!(
            commands,
            vec![Command::Send(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: None,
                name: "KANG".to_string(),
                token: None,
            })]
        );

        flow.on_event(rejected(RejectReason::VersionMismatch { server: 9 }));

        let form = form(&flow);
        assert_eq!(form.room.text(), "");
        assert!(form.notice.as_deref().unwrap().contains("Update or reload"));
    }

    #[test]
    fn a_name_the_server_refuses_puts_the_cursor_back_on_the_name() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "k7m2q");
        flow.on_event(rejected(RejectReason::BadName));

        assert_eq!(form(&flow).focus, Field::Name);
        assert!(
            form(&flow)
                .notice
                .as_deref()
                .unwrap()
                .contains("Name must be")
        );
    }

    #[test]
    fn a_name_that_does_not_fit_stays_on_the_form_and_asks_nothing_of_the_server() {
        let mut flow = start(launch(None, None), Memory::default());
        flow.on_key(FormKey::Paste("   ".to_string()));
        assert_eq!(flow.on_key(FormKey::Enter), vec![]);
        assert_eq!(form(&flow).notice.as_deref(), Some("Enter a name"));
        assert!(matches!(flow.screen(), Screen::Start(_)));
    }

    #[test]
    fn the_form_is_filled_with_the_room_of_the_link_before_the_last_room_and_the_last_name() {
        let memory = remembered("KANG", SERVER, "oldroom");

        let flow = start(launch(Some("k7m2q"), None), memory.clone());
        assert_eq!(form(&flow).room.text(), "k7m2q", "the link comes first");
        assert_eq!(form(&flow).name.text(), "KANG", "the name of last time");
        assert_eq!(
            form(&flow).focus,
            Field::Room,
            "only the room is left to check"
        );

        let flow = start(launch(None, None), memory);
        assert_eq!(form(&flow).room.text(), "oldroom", "the room of last time");
    }

    #[test]
    fn a_name_in_the_arguments_comes_before_the_name_of_last_time() {
        let memory = remembered("KANG", SERVER, "oldroom");
        let flow = start(launch(None, Some("MIRO")), memory);
        assert_eq!(form(&flow).name.text(), "MIRO");
    }

    #[test]
    fn the_last_room_is_the_last_room_of_that_server() {
        let memory = remembered("KANG", "ws://10.0.0.5:9000", "elsewhere");
        let flow = start(launch(None, None), memory);
        assert_eq!(
            form(&flow).room.text(),
            "",
            "another server's room is not offered"
        );
        assert_eq!(
            form(&flow).name.text(),
            "KANG",
            "the name is for every server"
        );
    }

    #[test]
    fn nothing_remembered_and_nothing_given_means_empty_fields() {
        let flow = start(launch(None, None), Memory::default());
        assert_eq!(form(&flow).name.text(), "");
        assert_eq!(form(&flow).room.text(), "");
        assert_eq!(form(&flow).notice, None);
    }

    #[test]
    fn a_name_and_a_room_in_the_arguments_join_without_the_start_screen() {
        let (flow, commands) =
            Flow::new(launch(Some("k7m2q"), Some("KANG")), Memory::default(), None);

        assert_eq!(
            commands,
            vec![Command::Connect {
                server: SERVER.to_string()
            }]
        );
        assert!(matches!(
            flow.screen(),
            Screen::Playing(Session::Connecting { .. })
        ));
    }

    #[test]
    fn a_name_and_a_room_from_the_memory_alone_do_not_skip_the_start_screen() {
        let memory = remembered("KANG", SERVER, "oldroom");
        let (flow, commands) = Flow::new(launch(None, None), memory, None);
        assert_eq!(commands, vec![]);
        assert!(matches!(flow.screen(), Screen::Start(_)));

        let memory = remembered("KANG", SERVER, "oldroom");
        let (flow, commands) = Flow::new(launch(Some("k7m2q"), None), memory, None);
        assert_eq!(commands, vec![], "a link has the room but not the name");
        assert!(matches!(flow.screen(), Screen::Start(_)));
    }

    #[test]
    fn a_wrong_name_in_the_arguments_shows_the_start_screen_with_the_reason() {
        let (flow, commands) = Flow::new(
            launch(Some("k7m2q"), Some("bad name!")),
            Memory::default(),
            None,
        );

        assert_eq!(commands, vec![]);
        assert_eq!(form(&flow).room.text(), "k7m2q");
        assert!(form(&flow).notice.as_deref().unwrap().contains("'!'"));
        assert_eq!(
            form(&flow).focus,
            Field::Name,
            "the name is what has to be fixed, though some of it was kept"
        );
    }

    #[test]
    fn a_wrong_argument_is_shown_on_the_start_screen_and_nothing_connects() {
        let bad = LaunchOptions::from_values(None, None, Some("http://x".to_string()), false);
        let (mut flow, commands) = Flow::new(bad, Memory::default(), None);
        assert_eq!(commands, vec![]);
        assert!(form(&flow).notice.as_deref().unwrap().contains("ws://"));

        type_text(&mut flow, "KANG");
        assert_eq!(
            flow.on_key(FormKey::Enter),
            vec![],
            "no guessing another server for a wrong address"
        );
        assert!(form(&flow).notice.as_deref().unwrap().contains("ws://"));
    }

    #[test]
    fn a_welcome_remembers_the_name_and_the_room_the_server_chose() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "");

        let commands = flow.on_event(ClientEvent::Message(welcome("k7m2q")));

        let [Command::Save(text)] = &commands[..] else {
            panic!("expected one Save, got {commands:?}");
        };
        let saved = Memory::parse(text);
        assert_eq!(saved.name().map(PlayerName::as_str), Some("KANG"));
        assert_eq!(saved.last_room(SERVER).map(RoomCode::as_str), Some("k7m2q"));
        assert_eq!(
            flow.session()
                .and_then(Session::joined)
                .map(|j| j.room.as_str()),
            Some("k7m2q")
        );
    }

    #[test]
    fn a_rejected_join_remembers_nothing() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "k7m2q");
        let commands = flow.on_event(rejected(RejectReason::RoomFull { capacity: 4 }));
        assert!(
            !commands.iter().any(|c| matches!(c, Command::Save(_))),
            "{commands:?}"
        );
    }

    #[test]
    fn the_room_the_server_made_is_in_the_form_after_a_disconnection() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));

        let commands = flow.on_event(ClientEvent::Disconnected {
            reason: "the server closed the connection".to_string(),
        });

        assert_eq!(commands, vec![Command::Disconnect]);
        let form = form(&flow);
        assert_eq!(
            form.room.text(),
            "k7m2q",
            "a blank room became the room of the server"
        );
        assert_eq!(form.name.text(), "KANG");
        assert_eq!(
            form.notice.as_deref(),
            Some("Disconnected: the server closed the connection")
        );
    }

    #[test]
    fn a_server_that_cannot_be_reached_returns_to_the_form_with_the_address() {
        let mut flow = start(launch(None, None), Memory::default());
        type_text(&mut flow, "KANG");
        flow.on_key(FormKey::Enter);

        let commands = flow.on_event(ClientEvent::Disconnected {
            reason: "Handshake not finished".to_string(),
        });

        assert_eq!(commands, vec![Command::Disconnect]);
        let notice = form(&flow).notice.clone().unwrap();
        assert!(notice.contains("ws://127.0.0.1:7777"), "{notice}");
        assert_eq!(form(&flow).name.text(), "KANG");
    }

    #[test]
    fn escape_in_the_room_returns_to_the_form_with_the_room_and_no_notice() {
        let mut flow = joined("");

        let commands = flow.leave();

        let [
            Command::Send(ClientMsg::Leave),
            Command::Disconnect,
            Command::Save(_),
        ] = &commands[..]
        else {
            panic!("expected Leave, Disconnect and Save, got {commands:?}");
        };
        let form = form(&flow);
        assert_eq!(form.room.text(), "k7m2q");
        assert_eq!(form.name.text(), "KANG");
        assert_eq!(form.notice, None);
    }

    fn prompt_is_open(flow: &Flow) -> bool {
        flow.session()
            .and_then(Session::joined)
            .is_some_and(|joined| joined.leave_prompt.is_open())
    }

    #[test]
    fn one_escape_in_the_room_asks_instead_of_leaving() {
        let mut flow = joined("k7m2q");

        let commands = flow.on_room_key(RoomKey::Escape);

        assert_eq!(commands, vec![], "nothing is sent and the connection stays");
        assert!(matches!(flow.screen(), Screen::Playing(Session::InRoom(_))));
        assert!(prompt_is_open(&flow));
    }

    #[test]
    fn a_second_escape_closes_the_prompt_and_the_room_goes_on() {
        let mut flow = joined("k7m2q");
        flow.on_room_key(RoomKey::Escape);

        let commands = flow.on_room_key(RoomKey::Escape);

        assert_eq!(commands, vec![]);
        assert!(matches!(flow.screen(), Screen::Playing(Session::InRoom(_))));
        assert!(!prompt_is_open(&flow));
    }

    #[test]
    fn enter_in_the_room_leaves_only_while_the_prompt_is_open() {
        let mut flow = joined("k7m2q");
        assert_eq!(
            flow.on_room_key(RoomKey::Enter),
            vec![],
            "without the prompt Enter is not a way out"
        );
        assert!(matches!(flow.screen(), Screen::Playing(_)));

        flow.on_room_key(RoomKey::Escape);
        let commands = flow.on_room_key(RoomKey::Enter);

        let [
            Command::Send(ClientMsg::Leave),
            Command::Disconnect,
            Command::Save(_),
        ] = &commands[..]
        else {
            panic!("expected Leave, Disconnect and Save, got {commands:?}");
        };
        let form = form(&flow);
        assert_eq!(form.room.text(), "k7m2q");
        assert_eq!(form.notice, None);
    }

    /// 서버로 가는 다음 입력이 결과 화면의 확인을 싣고 있는지 봅니다. `controls`는 그 프레임에 읽은 조작입니다.
    fn confirm_in_next_input(flow: &mut Flow, controls: &Controls) -> bool {
        let session = flow.session_mut().expect("in a room");
        match session.input_due(1.0, controls, None) {
            Some(ClientMsg::Input(input)) => input.confirm,
            other => panic!("an input is due: {other:?}"),
        }
    }

    fn enter() -> Controls {
        Controls {
            confirm: true,
            ..Controls::default()
        }
    }

    #[test]
    fn an_enter_pressed_in_the_frame_where_escape_closes_the_prompt_is_not_a_confirmation() {
        let mut flow = joined("k7m2q");
        flow.on_room_frame(Some(RoomKey::Escape), &mut Controls::default());
        assert!(prompt_is_open(&flow));

        let mut controls = enter();
        let commands = flow.on_room_frame(Some(RoomKey::Escape), &mut controls);

        assert_eq!(commands, vec![]);
        assert!(!prompt_is_open(&flow), "Escape closed the prompt");
        assert!(
            !confirm_in_next_input(&mut flow, &controls),
            "the prompt was up when Enter was pressed, so it was the prompt's Enter"
        );
    }

    #[test]
    fn an_enter_pressed_in_the_frame_where_escape_opens_the_prompt_is_not_a_confirmation() {
        let mut flow = joined("k7m2q");

        let mut controls = enter();
        flow.on_room_frame(Some(RoomKey::Escape), &mut controls);

        assert!(prompt_is_open(&flow));
        assert!(!confirm_in_next_input(&mut flow, &controls));
    }

    #[test]
    fn an_enter_with_no_prompt_in_its_frame_confirms_the_result_and_one_after_the_prompt_closed_does_too()
     {
        let mut flow = joined("k7m2q");
        let mut controls = enter();
        let commands = flow.on_room_frame(Some(RoomKey::Enter), &mut controls);
        assert_eq!(
            commands,
            vec![],
            "Enter without the prompt is not a way out"
        );
        assert!(confirm_in_next_input(&mut flow, &controls));

        let mut flow = joined("k7m2q");
        flow.on_room_frame(Some(RoomKey::Escape), &mut Controls::default());
        flow.on_room_frame(Some(RoomKey::Escape), &mut Controls::default());
        let mut controls = enter();
        flow.on_room_frame(Some(RoomKey::Enter), &mut controls);
        assert!(
            confirm_in_next_input(&mut flow, &controls),
            "closing the prompt gives Enter back"
        );
    }

    #[test]
    fn an_enter_that_leaves_through_the_prompt_confirms_nothing() {
        let mut flow = joined("k7m2q");
        flow.on_room_frame(Some(RoomKey::Escape), &mut Controls::default());

        let mut controls = enter();
        let commands = flow.on_room_frame(Some(RoomKey::Enter), &mut controls);

        assert!(matches!(
            &commands[..],
            [
                Command::Send(ClientMsg::Leave),
                Command::Disconnect,
                Command::Save(_)
            ]
        ));
        assert!(!controls.confirm);
    }

    #[test]
    fn before_the_room_is_entered_escape_leaves_at_once_and_enter_does_nothing() {
        let mut flow = start(launch(None, None), Memory::default());
        type_text(&mut flow, "KANG");
        flow.on_key(FormKey::Enter);
        assert_eq!(flow.on_room_key(RoomKey::Enter), vec![]);
        assert!(matches!(flow.screen(), Screen::Playing(_)));
        assert_eq!(
            flow.on_room_key(RoomKey::Escape),
            vec![Command::Disconnect],
            "the connection is not open yet, so there is nothing to confirm"
        );

        flow.on_key(FormKey::Enter);
        flow.on_event(ClientEvent::Connected);
        assert_eq!(flow.on_room_key(RoomKey::Enter), vec![]);
        let commands = flow.on_room_key(RoomKey::Escape);
        assert!(
            matches!(
                &commands[..],
                [
                    Command::Send(ClientMsg::Leave),
                    Command::Disconnect,
                    Command::Save(_)
                ]
            ),
            "the join was sent, so the seat is given up: {commands:?}"
        );
        assert!(matches!(flow.screen(), Screen::Start(_)));
    }

    #[test]
    fn room_keys_on_the_start_screen_do_nothing() {
        let mut flow = start(launch(None, None), Memory::default());
        assert_eq!(flow.on_room_key(RoomKey::Escape), vec![]);
        assert_eq!(flow.on_room_key(RoomKey::Enter), vec![]);
        assert!(matches!(flow.screen(), Screen::Start(_)));
    }

    #[test]
    fn an_open_prompt_is_gone_when_the_connection_drops_and_the_next_room_starts_without_one() {
        let mut flow = joined("k7m2q");
        flow.on_room_key(RoomKey::Escape);
        assert!(prompt_is_open(&flow));

        flow.on_event(ClientEvent::Disconnected {
            reason: "the server closed the connection".to_string(),
        });
        assert!(matches!(flow.screen(), Screen::Start(_)));

        flow.on_key(FormKey::Enter);
        flow.on_event(ClientEvent::Connected);
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));
        assert!(flow.session().and_then(Session::joined).is_some());
        assert!(!prompt_is_open(&flow), "the new room has no prompt");
    }

    /// 입장한 뒤 `Save`가 알려 준 글자를 돌려줍니다.
    fn saved_text(commands: &[Command]) -> String {
        commands
            .iter()
            .find_map(|command| match command {
                Command::Save(text) => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected a Save in {commands:?}"))
    }

    #[test]
    fn the_seat_token_of_the_welcome_is_saved_with_the_room() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "");

        let commands = flow.on_event(ClientEvent::Message(welcome("k7m2q")));

        let saved = Memory::parse(&saved_text(&commands));
        assert_eq!(
            saved.seat(SERVER, &RoomCode::parse("k7m2q").unwrap()),
            Some(WELCOMED_TOKEN)
        );
    }

    #[test]
    fn after_a_disconnection_enter_asks_for_the_same_seat_in_the_room_the_server_made() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));
        flow.on_event(ClientEvent::Disconnected {
            reason: "the server closed the connection".to_string(),
        });
        assert_eq!(
            form(&flow).room.text(),
            "k7m2q",
            "a blank room became the server's"
        );

        let commands = flow.on_key(FormKey::Enter);
        assert_eq!(
            commands,
            vec![Command::Connect {
                server: SERVER.to_string()
            }]
        );
        let commands = flow.on_event(ClientEvent::Connected);

        assert_eq!(
            commands,
            vec![Command::Send(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: Some("k7m2q".to_string()),
                name: "KANG".to_string(),
                token: Some(WELCOMED_TOKEN),
            })]
        );
    }

    #[test]
    fn the_token_of_one_room_is_not_sent_to_another_room() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "k7m2q");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));
        flow.on_event(ClientEvent::Disconnected {
            reason: "gone".to_string(),
        });
        assert_eq!(form(&flow).focus, Field::Room, "the cursor is on the room");
        for _ in 0..5 {
            flow.on_key(FormKey::Backspace);
        }
        type_text(&mut flow, "night");
        flow.on_key(FormKey::Enter);

        let commands = flow.on_event(ClientEvent::Connected);

        assert_eq!(
            commands,
            vec![Command::Send(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: Some("night".to_string()),
                name: "KANG".to_string(),
                token: None,
            })]
        );
    }

    #[test]
    fn a_link_to_the_room_the_device_remembers_a_seat_in_returns_to_that_seat() {
        let memory = remembered("KANG", SERVER, "k7m2q");
        let (mut flow, _) = Flow::new(launch(Some("k7m2q"), Some("KANG")), memory, None);

        let commands = flow.on_event(ClientEvent::Connected);

        assert_eq!(
            commands,
            vec![Command::Send(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: Some("k7m2q".to_string()),
                name: "KANG".to_string(),
                token: Some(MEMORISED_TOKEN),
            })]
        );
    }

    #[test]
    fn escape_gives_up_the_seat_and_the_next_enter_asks_for_a_new_one() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));

        let commands = flow.leave();

        let [
            Command::Send(ClientMsg::Leave),
            Command::Disconnect,
            Command::Save(text),
        ] = &commands[..]
        else {
            panic!("expected Leave, Disconnect and Save, got {commands:?}");
        };
        let saved = Memory::parse(text);
        let room = RoomCode::parse("k7m2q").unwrap();
        assert_eq!(saved.seat(SERVER, &room), None, "the token is forgotten");
        assert_eq!(
            saved.last_room(SERVER),
            Some(&room),
            "the room stays for the next Enter"
        );
        assert_eq!(saved.name().map(PlayerName::as_str), Some("KANG"));

        flow.on_key(FormKey::Enter);
        let commands = flow.on_event(ClientEvent::Connected);
        assert_eq!(
            commands,
            vec![Command::Send(ClientMsg::Join {
                version: PROTOCOL_VERSION,
                room: Some("k7m2q".to_string()),
                name: "KANG".to_string(),
                token: None,
            })]
        );
    }

    #[test]
    fn escape_sends_leave_only_after_the_join_was_sent() {
        let mut flow = start(launch(None, None), Memory::default());
        type_text(&mut flow, "KANG");
        flow.on_key(FormKey::Enter);
        assert_eq!(
            flow.leave(),
            vec![Command::Disconnect],
            "the server knows nothing of a connection that is not open"
        );

        flow.on_key(FormKey::Enter);
        flow.on_event(ClientEvent::Connected);
        let commands = flow.leave();
        assert!(
            matches!(
                &commands[..],
                [
                    Command::Send(ClientMsg::Leave),
                    Command::Disconnect,
                    Command::Save(_)
                ]
            ),
            "{commands:?}"
        );
    }

    #[test]
    fn a_seat_taken_over_by_another_window_returns_to_the_form_with_the_reason() {
        let mut flow = joined("k7m2q");

        let commands = flow.on_event(ClientEvent::Message(ServerMsg::SeatTakenOver));

        assert_eq!(commands, vec![Command::Disconnect]);
        let form = form(&flow);
        assert_eq!(form.room.text(), "k7m2q");
        assert_eq!(form.name.text(), "KANG");
        assert_eq!(
            form.notice.as_deref(),
            Some("Seat taken over: Another window took over this seat")
        );

        flow.on_key(FormKey::Enter);
        let commands = flow.on_event(ClientEvent::Connected);
        assert!(
            matches!(
                &commands[..],
                [Command::Send(ClientMsg::Join { token: Some(_), .. })]
            ),
            "Enter takes the seat back: {commands:?}"
        );
    }

    #[test]
    fn leaving_on_the_start_screen_does_nothing() {
        let mut flow = start(launch(None, None), Memory::default());
        assert_eq!(flow.leave(), vec![]);
        assert!(matches!(flow.screen(), Screen::Start(_)));
    }

    #[test]
    fn keys_in_the_room_and_events_on_the_start_screen_are_ignored() {
        let mut flow = joined("k7m2q");
        assert_eq!(flow.on_key(FormKey::Char('x')), vec![]);
        assert!(matches!(flow.screen(), Screen::Playing(_)));

        let mut flow = start(launch(None, None), Memory::default());
        assert_eq!(flow.on_event(ClientEvent::Connected), vec![]);
        assert!(matches!(flow.screen(), Screen::Start(_)));
    }

    #[test]
    fn the_roster_the_server_sends_is_kept_for_the_screen() {
        let mut flow = joined("k7m2q");
        let members = vec![
            caravan_protocol::WireMember {
                id: 1,
                name: "KANG".to_string(),
                here: true,
            },
            caravan_protocol::WireMember {
                id: 2,
                name: "MIRO".to_string(),
                here: true,
            },
        ];
        flow.on_event(ClientEvent::Message(ServerMsg::Roster(
            caravan_protocol::WireRoster {
                revision: 1,
                members: members.clone(),
            },
        )));
        assert_eq!(
            flow.session()
                .and_then(Session::joined)
                .map(|j| j.roster.clone()),
            Some(members)
        );
    }

    #[test]
    fn a_failed_paste_says_so_on_the_form() {
        let mut flow = start(launch(None, None), Memory::default());
        flow.on_paste_failed();
        assert!(form(&flow).notice.as_deref().unwrap().contains("clipboard"));
    }

    #[test]
    fn the_web_invite_is_the_page_with_the_room() {
        let (mut flow, _) = Flow::new(
            launch(None, None),
            Memory::default(),
            Some(Page::open("http://127.0.0.1:8080", "/", None)),
        );
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));

        assert_eq!(
            flow.copy_invite(),
            Some(Command::Copy(
                "http://127.0.0.1:8080/?room=k7m2q".to_string()
            ))
        );
        assert_eq!(flow.invite_kind(), InviteKind::Link);
    }

    #[test]
    fn the_web_invite_names_a_server_that_is_not_the_default() {
        let options = LaunchOptions::from_values(
            None,
            None,
            Some("ws://10.0.0.5:9000/play?x=1&y=2".to_string()),
            false,
        );
        let (mut flow, _) = Flow::new(
            options,
            Memory::default(),
            Some(Page::open("https://game.example", "/", None)),
        );
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));

        assert_eq!(
            flow.copy_invite(),
            Some(Command::Copy(
                "https://game.example/?room=k7m2q&server=ws://10.0.0.5:9000/play%3Fx%3D1%26y%3D2"
                    .to_string()
            ))
        );
    }

    #[test]
    fn the_invite_of_a_page_the_game_server_served_names_no_server() {
        // 주인은 이 컴퓨터의 주소로 열었고 서버는 터널의 공개 주소를 압니다. 링크는 공개 주소의 페이지이고, 그 페이지는 자기 주소의 서버에 접속하므로 서버 주소가 없습니다.
        let public = "https://quiet-river-1234.trycloudflare.com";
        let page = Page::open("http://127.0.0.1:7777", "/", Some(public));
        let options = LaunchOptions::from_page(None, None, None, false, &page);
        let (mut flow, _) = Flow::new(options, Memory::default(), Some(page));
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));
        assert_eq!(
            flow.copy_invite(),
            Some(Command::Copy(format!("{public}/?room=k7m2q")))
        );

        // 친구가 공개 주소로 연 페이지에서도 같은 링크입니다.
        let page = Page::open(public, "/", Some(public));
        let options = LaunchOptions::from_page(None, None, None, false, &page);
        let (mut flow, commands) = Flow::new(options, Memory::default(), Some(page));
        assert!(commands.is_empty());
        connected(&mut flow, "MIRO", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));
        assert_eq!(
            flow.copy_invite(),
            Some(Command::Copy(format!("{public}/?room=k7m2q")))
        );
    }

    #[test]
    fn a_server_named_by_the_link_is_carried_even_on_a_page_the_game_server_served() {
        let page = Page::open("http://127.0.0.1:7777", "/", Some(""));
        let options = LaunchOptions::from_page(
            None,
            None,
            Some("ws://10.0.0.5:9000".to_string()),
            false,
            &page,
        );
        let (mut flow, _) = Flow::new(options, Memory::default(), Some(page));
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));
        assert_eq!(
            flow.copy_invite(),
            Some(Command::Copy(
                "http://127.0.0.1:7777/?room=k7m2q&server=ws://10.0.0.5:9000".to_string()
            ))
        );
    }

    #[test]
    fn the_desktop_invite_is_the_room_code_alone() {
        let mut flow = start(launch(None, None), Memory::default());
        connected(&mut flow, "KANG", "");
        flow.on_event(ClientEvent::Message(welcome("k7m2q")));

        assert_eq!(flow.copy_invite(), Some(Command::Copy("k7m2q".to_string())));
        assert_eq!(flow.invite_kind(), InviteKind::Code);
    }

    #[test]
    fn there_is_no_invite_before_the_room_is_joined() {
        let mut flow = start(launch(None, None), Memory::default());
        assert_eq!(flow.copy_invite(), None, "on the start screen");
        connected(&mut flow, "KANG", "k7m2q");
        assert_eq!(flow.copy_invite(), None, "while joining");
    }
}
