//! 클라이언트에서 Bevy가 필요하지 않은 로직입니다. 이 크레이트의 테스트는 Bevy를 컴파일하지 않고 실행됩니다.
//!
//! - `launch`: 실행 인자(PC)나 페이지 주소의 값(웹)을 `LaunchOptions`로 해석합니다.
//! - `page`: 웹 페이지가 열린 곳에서 기본 서버의 주소와 초대 링크의 앞부분을 정합니다.
//! - `start`: 시작 화면의 입력 칸과 키 처리입니다. `memory`: 이 기기가 기억하는 이름과 마지막 방 코드입니다.
//! - `flow`: 시작 화면과 방 안의 접속 상태 가운데 어느 쪽인지, 그리고 접속, 저장, 복사처럼 클라이언트가 해야 할 일(`Command`)을 정합니다.
//! - `session`: 접속 방식의 이벤트에 따라 바뀌는 접속 상태입니다. 입장한 뒤에는 명단, 스냅샷 보관, 입력 순서 번호, 나가기 확인 창의 상태를 가집니다.
//! - `leave`: 방을 나가기 전에 한 번 더 묻는 확인 창입니다. 창이 떠 있는지와 Esc와 Enter가 무엇을 하는지를 정합니다.
//! - `fps`: 프레임 속도를 1초마다 알려 주는 계수기입니다.
//! - `hud`: 화면 위에 표시할 문구와 막대의 비율을 접속 상태와 `Frame`에서 정합니다.
//! - `boss`: 보스의 HP 막대, 노려지는 플레이어, 위험한 영역(내려찍기의 원, 돌진의 띠, 포효의 고리)과 경고의 남은 시간을 `Frame`에서 정합니다.
//! - `input`: 키보드와 마우스의 상태를 입력 메시지로 바꾸고, 서버 Tick마다 하나씩 내보냅니다.
//! - `iso`: 지면 좌표를 쿼터뷰 화면 좌표로 바꾸는 투영과 그리는 순서입니다.
//! - `camera`: 카메라가 내 캐릭터를 따라가는 방식과, 화면 밖에 있는 마차와 보스를 가리키는 표시의 자리입니다.
//! - `interp`: 스냅샷을 보관하고 100ms 이전의 방 모습을 보간해서 `Frame`으로 만듭니다.
//! - `preview`: 건설 키를 누르면 포탑이 놓일 타일을 서버와 같은 함수로 구하고, 범위를 미리 보여 줄 때와 자리를 정합니다.
//! - `ranges`: 화면에 그릴 포탑의 범위를 정합니다. 범위형 포탑의 원과 건설 미리 보기의 원입니다.

mod boss;
pub mod camera;
mod flow;
mod fps;
mod hud;
mod input;
mod interp;
mod iso;
mod launch;
mod leave;
mod memory;
mod page;
mod preview;
mod ranges;
mod session;
mod start;

pub use boss::{BossView, Danger, boss_view, lane_corners, warning_secs};
pub use flow::{Command, Flow, Screen};
pub use fps::FpsMeter;
pub use hud::{
    Banner, BannerTone, CampPanel, DebriefPanel, Hud, Invite, InviteKind, LeavePanel, Meter, Tone,
    bar_fraction, reject_text, structure_name, weapon_name,
};
pub use input::Controls;
pub use interp::{
    Frame, MonsterView, PlayerView, ProjectileView, Setting, ShownEffect, effect_progress,
};
pub use iso::{
    TILE_HEIGHT_PX, TILE_WIDTH_PX, circle_to_screen_radii, depth, ground_to_screen,
    screen_dir_to_ground, screen_to_ground, tile_centers,
};
pub use launch::{DEFAULT_SERVER, LaunchError, LaunchOptions};
pub use leave::{LeavePrompt, PromptStep, RoomKey};
pub use memory::Memory;
pub use page::Page;
pub use preview::{build_preview, range_preview};
pub use ranges::{RangeRing, range_rings};
pub use session::{Joined, Problem, Session};
pub use start::{Field, FormKey, JoinRequest, StartForm, TextField};
