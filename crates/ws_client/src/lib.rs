//! WebSocket으로 `ClientTransport`를 구현합니다. PC와 웹에서 같은 코드를 사용합니다 (ewebsock).
//!
//! ewebsock은 PC에서는 별도 스레드의 tungstenite로, 웹에서는 브라우저의 WebSocket으로 동작합니다.
//! 웹의 송신 핸들은 다른 스레드로 옮길 수 없으므로, Bevy 앱은 이 값을 non-send 리소스로 보관합니다.

use caravan_protocol::{ClientEvent, ClientMsg, ClientTransport, decode_server, encode_client};
use ewebsock::{Options, WsEvent, WsMessage, WsReceiver, WsSender};

/// 서버가 보내는 메시지 하나의 최대 크기(바이트)입니다. 몬스터 한 마리가 약 10바이트이므로 수만 마리까지 담는 크기입니다.
///
/// 정상적인 서버의 메시지는 이보다 훨씬 작습니다. 이 한계는 터무니없이 큰 메시지로 클라이언트의 메모리를 쓰게 하는 서버를 막습니다.
/// PC에서는 ewebsock이 이 크기를 넘는 프레임을 읽지 않고 접속을 끊고, 브라우저에서는 ewebsock이 이 설정을 무시하므로 받은 뒤에 크기를 확인합니다.
pub const MAX_SERVER_MESSAGE_BYTES: usize = 256 * 1024;

/// 닫힘을 받은 뒤 `Disconnected`를 알리기 전에, 아무것도 오지 않은 `poll`을 이만큼 더 기다립니다.
/// 브라우저에서는 ewebsock이 첫 송신부터 이진 메시지를 Blob으로 받고, Blob은 비동기로 읽습니다.
/// 서버가 마지막 메시지(입장 거절의 이유 등)를 보내고 바로 닫으면 닫힘이 그 메시지보다 먼저 도착할 수 있으므로, 늦게 오는 메시지를 기다립니다.
/// `poll`은 프레임마다 읽을 것이 없어질 때까지 부르므로, 이 횟수는 프레임의 수입니다.
const CLOSE_GRACE_POLLS: u32 = 3;

const CLOSED: &str = "the server closed the connection";

pub struct WsClient {
    sender: WsSender,
    inbox: Inbox,
}

impl WsClient {
    /// 접속을 시작하고 바로 돌아옵니다. 접속의 성공과 실패는 나중에 `poll`로 도착합니다.
    pub fn connect(url: &str) -> Result<WsClient, String> {
        let options = Options {
            max_incoming_frame_size: MAX_SERVER_MESSAGE_BYTES,
            ..Options::default()
        };
        let (sender, receiver) = ewebsock::connect(url, options)?;
        Ok(WsClient {
            sender,
            inbox: Inbox::new(receiver),
        })
    }
}

impl ClientTransport for WsClient {
    fn poll(&mut self) -> Option<ClientEvent> {
        let event = self.inbox.poll();
        if matches!(event, Some(ClientEvent::Disconnected { .. })) {
            self.sender.close();
        }
        event
    }

    fn send(&mut self, msg: &ClientMsg) {
        if !self.inbox.finished {
            self.sender.send(WsMessage::Binary(encode_client(msg)));
        }
    }
}

/// ewebsock이 알려 준 일을 `ClientEvent`로 바꿉니다. 접속과 따로 시험하려고 `WsClient`에서 나눴습니다.
struct Inbox {
    receiver: WsReceiver,
    /// ewebsock은 오류 뒤에 닫힘을 다시 알리기도 하고 알리지 않기도 합니다.
    /// 약속대로 `Disconnected`를 한 번만 돌려주기 위해 끝난 상태를 기억합니다.
    finished: bool,
    /// 닫힘을 받은 뒤에 `Disconnected`를 알리기까지 남은, 아무것도 오지 않은 `poll`의 횟수입니다. 닫힘을 받기 전에는 없습니다.
    closing: Option<u32>,
}

impl Inbox {
    fn new(receiver: WsReceiver) -> Inbox {
        Inbox {
            receiver,
            finished: false,
            closing: None,
        }
    }

    fn finish(&mut self, reason: String) -> Option<ClientEvent> {
        self.finished = true;
        Some(ClientEvent::Disconnected { reason })
    }

    fn poll(&mut self) -> Option<ClientEvent> {
        if self.finished {
            return None;
        }
        loop {
            let Some(event) = self.receiver.try_recv() else {
                // 읽을 것이 없습니다. 닫힘을 받았다면 늦게 올 메시지를 기다릴 횟수가 줄어듭니다.
                let left = self.closing.as_mut()?;
                if *left == 0 {
                    return self.finish(CLOSED.to_string());
                }
                *left -= 1;
                return None;
            };
            match event {
                WsEvent::Opened => return Some(ClientEvent::Connected),
                WsEvent::Message(WsMessage::Binary(bytes)) => {
                    if bytes.len() > MAX_SERVER_MESSAGE_BYTES {
                        return self.finish(format!(
                            "the server sent a message of {} bytes, above the limit of {MAX_SERVER_MESSAGE_BYTES}",
                            bytes.len()
                        ));
                    }
                    return match decode_server(&bytes) {
                        Ok(msg) => Some(ClientEvent::Message(msg)),
                        Err(err) => self.finish(format!("bad message from the server: {err}")),
                    };
                }
                // 서버는 이진 프레임만 보냅니다. Ping과 Pong은 ewebsock이 처리합니다.
                WsEvent::Message(_) => {}
                WsEvent::Error(err) => return self.finish(err),
                WsEvent::Closed => {
                    self.closing.get_or_insert(CLOSE_GRACE_POLLS);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;

    use caravan_protocol::{RejectReason, ServerMsg, encode_server};

    use super::*;

    type Push = Box<dyn Send + Fn(WsEvent) -> ControlFlow<()>>;

    fn inbox() -> (Inbox, Push) {
        let (receiver, push) = WsReceiver::new();
        (Inbox::new(receiver), push)
    }

    fn rejection() -> WsEvent {
        let reason = RejectReason::RoomFull { capacity: 4 };
        WsEvent::Message(WsMessage::Binary(encode_server(&ServerMsg::JoinRejected {
            reason,
        })))
    }

    /// `ClientEvent`는 같은지 비교할 수 없으므로 글자로 바꿔서 비교합니다.
    fn shown(event: Option<ClientEvent>) -> String {
        format!("{event:?}")
    }

    fn rejected() -> String {
        shown(Some(ClientEvent::Message(ServerMsg::JoinRejected {
            reason: RejectReason::RoomFull { capacity: 4 },
        })))
    }

    fn disconnected() -> String {
        shown(Some(ClientEvent::Disconnected {
            reason: CLOSED.to_string(),
        }))
    }

    #[test]
    fn a_message_before_the_close_comes_out_first_and_the_disconnect_last() {
        let (mut inbox, push) = inbox();
        let _ = push(rejection());
        let _ = push(WsEvent::Closed);

        assert_eq!(shown(inbox.poll()), rejected());
        for _ in 0..CLOSE_GRACE_POLLS {
            assert_eq!(shown(inbox.poll()), "None");
        }
        assert_eq!(shown(inbox.poll()), disconnected());
        assert_eq!(shown(inbox.poll()), "None", "reported only once");
    }

    #[test]
    fn a_message_that_arrives_after_the_close_is_still_delivered_before_the_disconnect() {
        let (mut inbox, push) = inbox();
        let _ = push(WsEvent::Closed);
        assert_eq!(shown(inbox.poll()), "None", "the close is held back");

        let _ = push(rejection());

        assert_eq!(shown(inbox.poll()), rejected());
        // 닫힘을 받고 기다린 `poll`이 하나 지났으므로 나머지만 기다립니다.
        for _ in 1..CLOSE_GRACE_POLLS {
            assert_eq!(shown(inbox.poll()), "None");
        }
        assert_eq!(shown(inbox.poll()), disconnected());
        assert_eq!(shown(inbox.poll()), "None");
    }

    #[test]
    fn a_close_with_nothing_late_ends_after_the_grace() {
        let (mut inbox, push) = inbox();
        let _ = push(WsEvent::Closed);
        for _ in 0..CLOSE_GRACE_POLLS {
            assert_eq!(shown(inbox.poll()), "None");
        }
        assert_eq!(shown(inbox.poll()), disconnected());
    }

    #[test]
    fn an_error_ends_at_once_without_the_grace() {
        let (mut inbox, push) = inbox();
        let _ = push(WsEvent::Error("refused".to_string()));
        assert_eq!(
            shown(inbox.poll()),
            shown(Some(ClientEvent::Disconnected {
                reason: "refused".to_string()
            }))
        );
        assert_eq!(shown(inbox.poll()), "None");
    }
}
