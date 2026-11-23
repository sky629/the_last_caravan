//! 접속 방식과 Bevy를 연결합니다. 프레임마다 접속 방식의 이벤트를 모두 읽어서 `Session`에 넘기고, `Session`이 정한 일을 합니다.

use bevy::prelude::*;
use caravan_client_core::{LaunchError, LaunchOptions, Reaction, Session};
use caravan_protocol::{ClientEvent, ClientTransport, ServerMsg, Snapshot};
use caravan_ws_client::WsClient;

/// 접속 방식입니다. 웹의 WebSocket은 다른 스레드로 옮길 수 없으므로 non-send 리소스로 보관합니다.
/// Bevy는 non-send 리소스를 사용하는 시스템을 항상 메인 스레드에서 실행합니다.
struct Net {
    transport: Box<dyn ClientTransport>,
    debug_net: bool,
}

/// 화면을 그리는 시스템이 읽는 접속 상태와 가장 최근의 스냅샷입니다.
#[derive(Resource)]
pub struct Link {
    pub session: Session,
    pub latest: Option<Snapshot>,
}

pub fn install(app: &mut App, launch: Result<LaunchOptions, LaunchError>) {
    let (session, net) = open(launch);
    info!("{}", session.status_text());
    app.insert_resource(Link {
        session,
        latest: None,
    });
    if let Some(net) = net {
        app.insert_non_send(net);
    }
    app.add_systems(PreUpdate, pump_network)
        .add_systems(Update, log_progress);
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

fn pump_network(net: Option<NonSendMut<Net>>, mut link: ResMut<Link>, time: Res<Time>) {
    let Some(mut net) = net else {
        return;
    };
    while let Some(event) = net.transport.poll() {
        if net.debug_net {
            info!("net <- {event:?}");
        }
        let is_snapshot = matches!(event, ClientEvent::Message(ServerMsg::Snapshot(_)));
        match link.session.on_event(event) {
            Reaction::Nothing => {}
            Reaction::Send(msg) => {
                if net.debug_net {
                    info!("net -> {msg:?}");
                }
                net.transport.send(&msg);
            }
            Reaction::Show(snapshot) => {
                if link.latest.is_none() {
                    info!(
                        "first snapshot: tick {} players {:?}",
                        snapshot.tick, snapshot.players
                    );
                }
                link.latest = Some(snapshot);
            }
        }
        // 스냅샷은 초당 20개이므로 상태가 바뀌는 이벤트만 기록합니다.
        if !is_snapshot {
            info!("{}", link.session.status_text());
        }
    }
    if let Some(input) = link.session.input_due(time.elapsed_secs_f64()) {
        if net.debug_net {
            info!("net -> {input:?}");
        }
        net.transport.send(&input);
    }
}

/// 창을 보지 않고도 동기화를 확인할 수 있도록, 1초에 한 번 가장 최근의 Tick을 기록합니다.
fn log_progress(link: Res<Link>, time: Res<Time>, mut next_report: Local<f64>) {
    let now = time.elapsed_secs_f64();
    if now < *next_report {
        return;
    }
    *next_report = now + 1.0;
    if let Some(snapshot) = &link.latest {
        info!(
            "latest tick {} players {:?}",
            snapshot.tick, snapshot.players
        );
    }
}
