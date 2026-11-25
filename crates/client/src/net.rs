//! 접속 방식, 저장소, 클립보드를 Bevy와 연결합니다. 무엇을 할지는 `Flow`가 `Command`로 정하고, 이 파일은 실행만 합니다.
//! 프레임마다 접속 방식의 이벤트를 모두 `Flow`에 넘기고, 재생 시각을 앞으로 보내서 화면에 그릴 `Frame`을 만들고,
//! 입력을 서버 Tick마다 하나씩 보냅니다.

use std::collections::VecDeque;

use bevy::clipboard::Clipboard;
use bevy::prelude::*;
use caravan_client_core::{Command, Flow, Frame, Joined, Session};
use caravan_protocol::{ClientEvent, ClientTransport, ServerMsg};
use caravan_ws_client::WsClient;

use crate::input::HeldControls;
use crate::storage;

/// 접속 방식입니다. 웹의 WebSocket은 다른 스레드로 옮길 수 없으므로 non-send 리소스로 보관합니다.
/// Bevy는 non-send 리소스를 사용하는 시스템을 항상 메인 스레드에서 실행합니다. 접속이 없으면 `transport`가 없습니다.
pub struct Net {
    transport: Option<Box<dyn ClientTransport>>,
    debug_net: bool,
}

/// 화면 흐름입니다. `Flow`는 Bevy를 모르는 크레이트의 타입이므로 리소스로 쓰려면 감쌉니다.
#[derive(Resource)]
pub struct ClientFlow(pub Flow);

impl ClientFlow {
    /// 방에 들어갔을 때의 상태입니다. 시작 화면이거나 아직 입장하는 중이면 없습니다.
    pub fn joined(&self) -> Option<&Joined> {
        self.0.session().and_then(Session::joined)
    }
}

/// 이번 프레임에 화면에 그릴 방의 모습입니다. 입장하기 전과 첫 스냅샷을 받기 전에는 없습니다.
#[derive(Resource, Default)]
pub struct CurrentFrame(pub Option<Frame>);

/// 앱이 시작할 때 `Flow`가 돌려준 일입니다. 첫 프레임에 실행합니다.
#[derive(Resource)]
pub struct StartupCommands(Vec<Command>);

/// 리소스를 넣습니다. 시스템은 `main.rs`의 프레임 순서에 있습니다.
pub fn install(app: &mut App, flow: Flow, first: Vec<Command>, debug_net: bool) {
    app.insert_resource(ClientFlow(flow))
        .insert_resource(StartupCommands(first))
        .init_resource::<CurrentFrame>()
        .insert_non_send(Net {
            transport: None,
            debug_net,
        });
}

/// `Flow`가 돌려준 일을 순서대로 실행합니다. `Command`를 실행하는 곳은 이 함수 하나입니다.
/// 접속을 열지 못하면 `Flow`에 끊김을 알리고, `Flow`가 돌려준 일도 이어서 실행합니다.
pub fn run_commands(
    commands: Vec<Command>,
    flow: &mut Flow,
    net: &mut Net,
    clipboard: &mut Clipboard,
) {
    let mut queue: VecDeque<Command> = commands.into();
    while let Some(command) = queue.pop_front() {
        match command {
            Command::Connect { server } => {
                info!("connecting to {server}");
                match WsClient::connect(&server) {
                    Ok(client) => net.transport = Some(Box::new(client)),
                    Err(reason) => {
                        queue.extend(flow.on_event(ClientEvent::Disconnected { reason }))
                    }
                }
            }
            Command::Send(msg) => {
                if let Some(transport) = &mut net.transport {
                    if net.debug_net {
                        info!("net -> {msg:?}");
                    }
                    transport.send(&msg);
                }
            }
            // 접속을 버리면 닫힙니다.
            Command::Disconnect => net.transport = None,
            Command::Save(text) => storage::save(&text),
            Command::Copy(text) => {
                if let Err(err) = clipboard.set_text(text) {
                    warn!("cannot copy to the clipboard: {err}");
                }
            }
        }
    }
}

/// 첫 프레임에 시작 일을 실행하고, 접속 방식에 도착한 이벤트를 모두 `Flow`에 넘깁니다.
pub fn pump_network(
    mut net: NonSendMut<Net>,
    mut flow: ResMut<ClientFlow>,
    mut clipboard: ResMut<Clipboard>,
    mut startup: ResMut<StartupCommands>,
) {
    let first = std::mem::take(&mut startup.0);
    run_commands(first, &mut flow.0, &mut net, &mut clipboard);
    while let Some(event) = net
        .transport
        .as_mut()
        .and_then(|transport| transport.poll())
    {
        if net.debug_net {
            info!("net <- {}", describe(&event));
        }
        let is_snapshot = matches!(event, ClientEvent::Message(ServerMsg::Snapshot(_)));
        let commands = flow.0.on_event(event);
        run_commands(commands, &mut flow.0, &mut net, &mut clipboard);
        // 스냅샷은 초당 20개이므로 상태가 바뀔 수 있는 이벤트만 기록합니다.
        if !is_snapshot && let Some(session) = flow.0.session() {
            info!("{}", session.status_text());
        }
    }
}

/// 재생 시각을 프레임 시간만큼 앞으로 보내고, 그릴 모습을 만듭니다.
pub fn advance_playback(
    time: Res<Time<Real>>,
    mut flow: ResMut<ClientFlow>,
    mut frame: ResMut<CurrentFrame>,
) {
    if let Some(session) = flow.0.session_mut() {
        session.advance(time.delta_secs_f64());
    }
    frame.0 = flow.0.session().and_then(Session::frame);
}

/// 방 안에서 Esc를 누르면 시작 화면으로 돌아갑니다. 입장하는 중에도 됩니다.
pub fn read_room_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut net: NonSendMut<Net>,
    mut flow: ResMut<ClientFlow>,
    mut clipboard: ResMut<Clipboard>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        let commands = flow.0.leave();
        run_commands(commands, &mut flow.0, &mut net, &mut clipboard);
    }
}

/// 입력을 보낼 때가 되었으면 지금의 조작을 서버에 보냅니다.
pub fn send_input(
    mut net: NonSendMut<Net>,
    time: Res<Time<Real>>,
    controls: Res<HeldControls>,
    frame: Res<CurrentFrame>,
    mut flow: ResMut<ClientFlow>,
) {
    let Some(you) = flow.joined().map(|joined| joined.welcome.you) else {
        return;
    };
    let me = frame
        .0
        .as_ref()
        .and_then(|frame| frame.player(you))
        .map(|player| player.pos);
    let Some(session) = flow.0.session_mut() else {
        return;
    };
    let debug_net = net.debug_net;
    if let Some(input) = session.input_due(time.elapsed_secs_f64(), &controls.0, me)
        && let Some(transport) = &mut net.transport
    {
        if debug_net {
            info!("net -> {input:?}");
        }
        transport.send(&input);
    }
}

/// 스냅샷은 몬스터가 수백 마리일 수 있으므로 요약해서 출력합니다.
fn describe(event: &ClientEvent) -> String {
    match event {
        ClientEvent::Message(ServerMsg::Snapshot(snapshot)) => format!(
            "Snapshot {{ tick: {}, players: {}, monsters: {}, projectiles: {} }}",
            snapshot.tick,
            snapshot.players.len(),
            snapshot.monsters.len(),
            snapshot.projectiles.len()
        ),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use caravan_client_core::{LaunchOptions, Memory};
    use caravan_protocol::ClientMsg;

    /// 보낸 메시지를 모아 두는 접속 방식입니다.
    struct Recorder(Rc<RefCell<Vec<ClientMsg>>>);

    impl ClientTransport for Recorder {
        fn poll(&mut self) -> Option<ClientEvent> {
            None
        }

        fn send(&mut self, msg: &ClientMsg) {
            self.0.borrow_mut().push(msg.clone());
        }
    }

    fn start_screen() -> Flow {
        let launch = LaunchOptions::from_values(None, None, None, false);
        Flow::new(launch, Memory::default(), None).0
    }

    #[test]
    fn what_the_flow_sends_goes_out_on_the_open_connection_and_leaving_closes_it() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let mut net = Net {
            transport: Some(Box::new(Recorder(sent.clone()))),
            debug_net: false,
        };
        let mut flow = start_screen();
        let mut clipboard = Clipboard::default();
        let join = ClientMsg::Join {
            version: 8,
            room: None,
            name: "KANG".to_string(),
        };

        run_commands(
            vec![Command::Send(join.clone())],
            &mut flow,
            &mut net,
            &mut clipboard,
        );
        assert_eq!(*sent.borrow(), vec![join]);
        assert!(net.transport.is_some(), "sending keeps the connection");

        run_commands(
            vec![Command::Disconnect],
            &mut flow,
            &mut net,
            &mut clipboard,
        );
        assert!(net.transport.is_none(), "leaving closes the connection");
    }

    #[test]
    fn a_message_with_no_open_connection_is_dropped_without_an_error() {
        let mut net = Net {
            transport: None,
            debug_net: false,
        };
        let join = ClientMsg::Join {
            version: 8,
            room: None,
            name: "KANG".to_string(),
        };
        run_commands(
            vec![Command::Send(join)],
            &mut start_screen(),
            &mut net,
            &mut Clipboard::default(),
        );
        assert!(net.transport.is_none());
    }
}
