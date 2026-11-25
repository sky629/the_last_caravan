//! THE LAST CARAVAN의 게임 규칙입니다.
//!
//! 규칙은 시계, 소켓, 파일, 화면을 알지 못하고 난수도 쓰지 않습니다. 시간은 `Sim::step` 한 번이 서버 Tick 하나라는 약속으로만 흐릅니다.
//! 그래서 같은 입력은 항상 같은 결과를 내고, 같은 코드가 서버에서 실행되며, 클라이언트 예측을 추가하면 클라이언트에서도 실행됩니다.
//!
//! 바깥에서 쓰는 것은 `Sim`(생성, 입장, 퇴장, `step`, 읽기 함수), `Tuning`(모든 수치), `PlayerInput`, 그리고 스냅샷이 읽는 상태 타입입니다.
//! 타일 지도, 충돌, 경로 탐색 같은 내부 모듈은 공개하지 않습니다.
//!
//! 읽는 순서: `tuning`(모든 수치), `sim`(Tick 하나의 처리 순서), `player`, `loadout`, `weapon`, `projectile`, `stamina`, `resources`, `monster`, `structures`, `build`, `expedition`, `spawn`.
//! `world`(타일 지도 포함), `collision`, `sweep`, `nav`, `spatial`은 이 모듈들이 쓰는 도구입니다.

mod build;
mod collision;
mod expedition;
mod ids;
mod input;
mod kind;
mod loadout;
mod loot;
mod math;
mod monster;
mod nav;
mod player;
mod projectile;
mod resources;
mod sim;
mod spatial;
mod spawn;
mod stamina;
mod stats;
mod structures;
mod sweep;
mod tuning;
mod weapon;
mod world;

#[cfg(test)]
mod tests;

pub use build::{BuildRefusal, CannotBuild, front_tile};
pub use expedition::{NightStep, Outcome, Phase};
pub use ids::{MonsterId, NodeId, PileId, PlayerId, ProjectileId, Team, TowerId};
pub use input::{AimDir, MoveDir, PlayerInput, PlayerInputs};
pub use kind::{MonsterKind, StructureKind, WeaponKind};
pub use loadout::Loadout;
pub use loot::Seed;
pub use math::{Rect, Vec2};
pub use monster::{Monster, Order};
pub use player::{Interaction, LastAttack, Life, Player, RepairTarget};
pub use projectile::{Flight, Projectile};
pub use resources::{GatherNode, Pile};
pub use sim::{JoinError, Sim};
pub use stamina::Stamina;
pub use structures::{Shot, Tower, TowerState, Wagon};
pub use tuning::{
    ArcTuning, CycleTuning, Delivery, GuardTuning, HealTuning, MapSpec, MonsterTable,
    MonsterTuning, NightPlan, NightTuning, PlayerTuning, ProjectileTuning, RepairTuning,
    ResourceTuning, ReviveTuning, SprintTuning, StructureTable, StructureTuning, Tuning,
    VillageTuning, WagonTuning, WavePlan, WeaponTable, WeaponTuning,
};
pub use world::Tile;
