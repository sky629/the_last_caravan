//! 클라이언트에서 Bevy가 필요하지 않은 로직입니다. 이 크레이트의 테스트는 Bevy를 컴파일하지 않고 실행됩니다.
//!
//! - `launch`: 실행 인자(PC)나 페이지 주소의 값(웹)을 `LaunchOptions`로 해석합니다.
//! - `session`: 접속 방식의 이벤트에 따라 바뀌는 접속 상태입니다.
//! - `iso`: 지면 좌표를 쿼터뷰 화면 좌표로 바꾸는 투영과 그리는 순서입니다.
//! - `interp`: 스냅샷을 보관하고 100ms 이전의 방 모습을 보간해서 `Frame`으로 만듭니다.

mod interp;
mod iso;
mod launch;
mod session;

pub use interp::{
    Frame, INTERP_DELAY_TICKS, MonsterView, PlayerView, SnapshotBuffer, effect_progress,
};
pub use iso::{
    MapView, TILE_HEIGHT_PX, TILE_WIDTH_PX, depth, ground_to_screen, map_view,
    screen_dir_to_ground, screen_to_ground, tile_centers,
};
pub use launch::{DEFAULT_ROOM, DEFAULT_SERVER, LaunchError, LaunchOptions};
pub use session::{Reaction, Session};
