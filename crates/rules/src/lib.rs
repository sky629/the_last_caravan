//! THE LAST CARAVAN의 게임 규칙입니다.
//!
//! 규칙은 시계, 소켓, 파일, 화면을 알지 못합니다. 시간은 `Sim::step` 한 번이 서버 Tick 하나라는 약속으로만 흐릅니다.
//! 그래서 같은 코드가 서버에서 실행되고, 클라이언트 예측을 추가하면 클라이언트에서도 실행됩니다.
//!
//! 읽는 순서: `tuning`(모든 수치), `sim`(Tick 하나의 처리), `ids`, `math`.

mod ids;
mod math;
mod sim;
mod tuning;

pub use ids::PlayerId;
pub use math::Vec2;
pub use sim::{JoinError, Sim};
pub use tuning::Tuning;
