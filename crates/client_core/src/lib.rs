//! 클라이언트에서 Bevy가 필요하지 않은 로직입니다. 이 크레이트의 테스트는 Bevy를 컴파일하지 않고 실행됩니다.
//!
//! - `launch`: 실행 인자(PC)나 페이지 주소의 값(웹)을 `LaunchOptions`로 해석합니다.
//! - `session`: 접속 방식의 이벤트에 따라 바뀌는 접속 상태입니다. 입장한 뒤에는 스냅샷 보관과 입력 순서 번호를 가집니다.
//! - `fps`: 프레임 속도를 1초마다 알려 주는 계수기입니다.
//! - `hud`: 화면 위에 표시할 문구와 막대의 비율을 접속 상태와 `Frame`에서 정합니다.
//! - `input`: 키보드와 마우스의 상태를 입력 메시지로 바꾸고, 서버 Tick마다 하나씩 내보냅니다.
//! - `iso`: 지면 좌표를 쿼터뷰 화면 좌표로 바꾸는 투영과 그리는 순서입니다.
//! - `camera`: 카메라가 내 캐릭터를 따라가는 방식과, 화면 밖에 있는 마차를 가리키는 표시의 자리입니다.
//! - `interp`: 스냅샷을 보관하고 100ms 이전의 방 모습을 보간해서 `Frame`으로 만듭니다.

pub mod camera;
mod fps;
mod hud;
mod input;
mod interp;
mod iso;
mod launch;
mod session;

pub use fps::FpsMeter;
pub use hud::{Banner, BannerTone, Hud, Meter, Tone, bar_fraction};
pub use input::Controls;
pub use interp::{Frame, MonsterView, PlayerView, effect_progress};
pub use iso::{
    TILE_HEIGHT_PX, TILE_WIDTH_PX, circle_to_screen_radii, depth, ground_to_screen,
    screen_dir_to_ground, screen_to_ground, tile_centers,
};
pub use launch::{DEFAULT_ROOM, DEFAULT_SERVER, LaunchError, LaunchOptions};
pub use session::{Joined, Problem, Session};
