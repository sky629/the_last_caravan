//! THE LAST CARAVAN의 게임 규칙입니다.
//!
//! 규칙은 시계, 소켓, 파일, 화면을 알지 못하고 난수도 쓰지 않습니다. 시간은 `Sim::step` 한 번이 서버 Tick 하나라는 약속으로만 흐릅니다.
//! 그래서 같은 입력은 항상 같은 결과를 내고, 같은 코드가 서버에서 실행되며, 클라이언트 예측을 추가하면 클라이언트에서도 실행됩니다.
//!
//! 바깥에서 쓰는 것은 `Sim`(생성, 입장, 퇴장, `step`, 읽기 함수), `Tuning`(모든 수치), `PlayerInput`, 그리고 스냅샷이 읽는 상태 타입입니다.
//! 충돌과 경로 탐색 같은 내부 모듈은 공개하지 않습니다.
//!
//! 읽는 순서: `tuning`(모든 수치), `sim`(Tick 하나의 처리 순서), `player`, `monster`, `structures`, `expedition`.
//! `world`, `collision`, `nav`, `spatial`은 이 모듈들이 쓰는 도구입니다.

mod collision;
mod expedition;
mod ids;
mod input;
mod kind;
mod math;
mod monster;
mod nav;
mod player;
mod sim;
mod spatial;
mod stats;
mod structures;
mod tuning;
mod world;

#[cfg(test)]
mod tests;

pub use expedition::{Outcome, Phase};
pub use ids::{MonsterId, PlayerId, Team, TowerId};
pub use input::{AimDir, MoveDir, PlayerInput, PlayerInputs};
pub use kind::{MonsterKind, StructureKind};
pub use math::{Rect, Vec2};
pub use monster::Monster;
pub use player::{Interaction, Life, Player};
pub use sim::{JoinError, Sim};
pub use structures::{Shot, Tower, TowerState, Wagon};
pub use tuning::{
    MapSpec, MonsterTable, MonsterTuning, PlayerTuning, ReviveTuning, StructurePlacement,
    StructureTable, StructureTuning, Tuning, WagonTuning, WaveTuning,
};
