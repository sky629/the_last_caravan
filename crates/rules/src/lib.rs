//! THE LAST CARAVAN의 게임 규칙입니다.
//!
//! 규칙은 시계, 소켓, 파일, 화면을 알지 못합니다. 시간은 `Sim::step` 한 번이 서버 Tick 하나라는 약속으로만 흐릅니다.
//! 난수의 근원도 `Sim::new`가 받는 씨앗(`Seed`) 하나뿐이고, 무기 상자의 자리와 내용만 그 씨앗에서 정해집니다(`loot`).
//! 그래서 같은 씨앗과 같은 입력은 항상 같은 결과를 내고, 같은 코드가 서버에서 실행되며, 클라이언트 예측을 추가하면 클라이언트에서도 실행됩니다.
//!
//! 바깥에서 쓰는 것은 `Sim`(생성, 입장, 퇴장, `step`, 읽기 함수), `Tuning`(모든 수치), `PlayerInput`, 그리고 스냅샷이 읽는 상태 타입입니다.
//! 몬스터에게 걸린 효과(`MonsterEffects`)와 플레이어가 받은 버프(`PlayerBuffs`)도 스냅샷이 읽는 상태 타입이고, 값을 바꾸는 길은 `effect` 모듈 안에만 있습니다.
//! 플레이어에게 버프를 거는 함수(`Player::grant`)는 크레이트 밖에 공개하지 않습니다.
//! 타일 지도, 충돌, 경로 탐색 같은 내부 모듈은 공개하지 않습니다.
//!
//! 읽는 순서: `tuning`(모든 수치), `sim`(Tick 하나의 처리 순서), `stage`(야영지, 원정, 결과 화면을 도는 방의 단계), `camp`, `debrief`, `player`, `loadout`, `weapon`, `projectile`, `stamina`, `resources`, `effect`, `monster`, `structures`, `build`, `expedition`, `loot`, `spawn`.
//! `world`(타일 지도 포함), `collision`, `sweep`, `nav`, `spatial`은 이 모듈들이 쓰는 도구입니다.

mod build;
mod camp;
mod collision;
mod debrief;
mod effect;
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
mod stage;
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
pub use camp::Camp;
pub use debrief::{Debrief, Report};
pub use effect::{MonsterEffects, PlayerBuffs};
pub use expedition::{NightStep, Outcome, Phase};
pub use ids::{
    ChestId, GroundWeaponId, MonsterId, NodeId, PileId, PlayerId, ProjectileId, Team, TowerId,
};
pub use input::{AimDir, MoveDir, PlayerInput, PlayerInputs};
pub use kind::{MonsterKind, StructureKind, WeaponKind};
pub use loadout::Loadout;
pub use loot::{Chest, GroundWeapon, Prompt, Rack, Seed};
pub use math::{Rect, Vec2};
pub use monster::{Monster, Order};
pub use player::{Interaction, LastAttack, Life, Player, Presence, Record, RepairTarget};
pub use projectile::{Flight, Projectile};
pub use resources::{GatherNode, Pile, nodes_of};
pub use sim::{JoinError, Sim};
pub use stage::Stage;
pub use stamina::Stamina;
pub use structures::{Shot, Tower, TowerState, Wagon};
pub use tuning::{
    ArcTuning, Buff, CampLayout, CampTuning, CycleTuning, Debuff, Delivery, EffectTuning,
    GuardTuning, HealTuning, HitscanTuning, LootTuning, MapSpec, MonsterTable, MonsterTuning,
    NightPlan, NightTuning, PlayerTuning, ProjectileTuning, RepairTuning, ResourceTuning,
    ReviveTuning, ShootTuning, SprintTuning, Start, StructureTable, StructureTuning, TowerAction,
    Tuning, VillageTuning, WagonTuning, WavePlan, WaveScaling, WeaponTable, WeaponTuning,
};
pub use world::Tile;
