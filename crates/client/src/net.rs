//! 접속 방식과 Bevy를 연결합니다. 프레임마다 접속 방식의 이벤트를 모두 읽어서 `Session`에 넘기고,
//! 재생 시각을 앞으로 보내서 화면에 그릴 `Frame`을 만들고, 입력을 서버 Tick마다 하나씩 보냅니다.

use bevy::prelude::*;
use caravan_client_core::{Frame, LaunchError, LaunchOptions, Session};
use caravan_protocol::{ClientEvent, ClientTransport, ServerMsg};
use caravan_ws_client::WsClient;

use crate::input::HeldControls;

/// 접속 방식입니다. 웹의 WebSocket은 다른 스레드로 옮길 수 없으므로 non-send 리소스로 보관합니다.
/// Bevy는 non-send 리소스를 사용하는 시스템을 항상 메인 스레드에서 실행합니다.
pub struct Net {
    transport: Box<dyn ClientTransport>,
    debug_net: bool,
}

/// 접속 상태입니다. `Session`은 Bevy를 모르는 크레이트의 타입이므로 리소스로 쓰려면 감싸야 합니다.
#[derive(Resource)]
pub struct ClientSession(pub Session);

/// 이번 프레임에 화면에 그릴 방의 모습입니다. 입장하기 전과 첫 스냅샷을 받기 전에는 없습니다.
#[derive(Resource, Default)]
pub struct CurrentFrame(pub Option<Frame>);

/// 리소스를 넣고 접속을 시작합니다. 시스템은 `main.rs`의 프레임 순서에 있습니다.
pub fn install(app: &mut App, launch: Result<LaunchOptions, LaunchError>) {
    let (session, net) = open(launch);
    info!("{}", session.status_text());
    app.insert_resource(ClientSession(session))
        .init_resource::<CurrentFrame>();
    if let Some(net) = net {
        app.insert_non_send(net);
    }
}

/// 클라이언트가 접속 방식을 고르는 곳은 이 함수 하나입니다. 다른 접속 방식을 추가하면 여기만 바꿉니다.
fn open(launch: Result<LaunchOptions, LaunchError>) -> (Session, Option<Net>) {
    let options = match launch {
        Ok(options) => options,
        Err(err) => {
            let reason = err.to_string();
            return (Session::Failed { reason }, None);
        }
    };
    info!(
        "connecting to {} (room {})",
        options.server_url, options.room
    );
    match WsClient::connect(&options.server_url) {
        Ok(client) => {
            let net = Net {
                transport: Box::new(client),
                debug_net: options.debug_net,
            };
            (Session::new(options.room), Some(net))
        }
        Err(reason) => (Session::Failed { reason }, None),
    }
}

/// 접속 방식에 도착한 이벤트를 모두 `Session`에 넘기고, 서버에 보낼 답이 있으면 보냅니다.
pub fn pump_network(
    mut commands: Commands,
    net: Option<NonSendMut<Net>>,
    mut session: ResMut<ClientSession>,
) {
    let Some(mut net) = net else {
        return;
    };
    while let Some(event) = net.transport.poll() {
        if net.debug_net {
            info!("net <- {}", describe(&event));
        }
        let is_snapshot = matches!(event, ClientEvent::Message(ServerMsg::Snapshot(_)));
        if let Some(reply) = session.0.on_event(event) {
            if net.debug_net {
                info!("net -> {reply:?}");
            }
            net.transport.send(&reply);
        }
        // 스냅샷은 초당 20개이므로 상태가 바뀔 수 있는 이벤트만 기록합니다.
        if !is_snapshot {
            info!("{}", session.0.status_text());
        }
    }
    // 입장이 끝났거나 거절되었거나 믿을 수 없는 서버이면 접속 방식을 버립니다. 버리면 접속이 닫힙니다.
    if session.0.is_over() {
        commands.queue(|world: &mut World| {
            world.remove_non_send::<Net>();
        });
    }
}

/// 재생 시각을 프레임 시간만큼 앞으로 보내고, 그릴 모습을 만듭니다.
pub fn advance_playback(
    time: Res<Time<Real>>,
    mut session: ResMut<ClientSession>,
    mut frame: ResMut<CurrentFrame>,
) {
    session.0.advance(time.delta_secs_f64());
    frame.0 = session.0.frame();
}

/// 입력을 보낼 때가 되었으면 지금의 조작을 서버에 보냅니다.
pub fn send_input(
    net: Option<NonSendMut<Net>>,
    time: Res<Time<Real>>,
    controls: Res<HeldControls>,
    frame: Res<CurrentFrame>,
    mut session: ResMut<ClientSession>,
) {
    let Some(mut net) = net else {
        return;
    };
    let Some(joined) = session.0.joined() else {
        return;
    };
    let me = frame
        .0
        .as_ref()
        .and_then(|frame| frame.player(joined.welcome.you))
        .map(|player| player.pos);
    if let Some(input) = session
        .0
        .input_due(time.elapsed_secs_f64(), &controls.0, me)
    {
        if net.debug_net {
            info!("net -> {input:?}");
        }
        net.transport.send(&input);
    }
}

/// 스냅샷은 몬스터가 수백 마리일 수 있으므로 요약해서 출력합니다.
fn describe(event: &ClientEvent) -> String {
    match event {
        ClientEvent::Message(ServerMsg::Snapshot(snapshot)) => format!(
            "Snapshot {{ tick: {}, players: {}, monsters: {} }}",
            snapshot.tick,
            snapshot.players.len(),
            snapshot.monsters.len()
        ),
        other => format!("{other:?}"),
    }
}
