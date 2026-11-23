//! 클라이언트에서 Bevy가 필요하지 않은 로직입니다. 이 크레이트의 테스트는 Bevy를 컴파일하지 않고 실행됩니다.
//!
//! - `launch`: 실행 인자(PC)나 페이지 주소의 값(웹)을 `LaunchOptions`로 해석합니다.
//! - `session`: 접속 방식의 이벤트에 따라 바뀌는 접속 상태입니다.

mod launch;
mod session;

pub use launch::{DEFAULT_ROOM, DEFAULT_SERVER, LaunchError, LaunchOptions};
pub use session::{Reaction, Session};
