//! 규칙의 상태를 전송 형식으로 바꾸는 곳입니다. 게임 상태를 규칙에서 전송 형식으로 바꾸는 함수는 `Snapshot::from_sim` 하나입니다.
//!
//! 전송 형식의 타입은 규칙의 타입과 따로 정의합니다. 규칙의 내부 표현이 바뀌어도 전송 형식은 이 파일에서만 고칩니다.
//! 값은 클라이언트가 수치표를 읽지 않고 곧바로 그릴 수 있게 정합니다.
//! 시간은 Tick이 아니라 초로, 진행도는 0.0에서 1.0 사이의 실수로, 위치와 방향은 `units`의 정수로 보냅니다.
//! 번호는 규칙의 번호 타입이 아니라 `u32`입니다. 규칙의 타입은 전송 형식에 나타나지 않습니다.
//!
//! 몬스터는 수백 마리일 수 있으므로 `WireMonster` 하나가 postcard로 9바이트 안팎이 되게 정했습니다
//! (번호 2, 종류 1, 위치 4, 방향 1, HP 1). postcard는 필드 이름을 기록하지 않으므로 필드 이름은 크기에 영향이 없습니다.

use caravan_rules::{
    BuildRefusal, CannotBuild, GatherNode, Interaction, Life, Monster, MonsterKind, NightStep,
    Outcome, Phase, Pile, Player, Projectile, RepairTarget, Sim, Tower, TowerState, Tuning,
    WeaponKind,
};
use serde::{Deserialize, Serialize};

use crate::units::{WireAngle, WirePos, WireTile};

/// Tick 하나가 끝난 뒤의 방 전체 상태입니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 서버의 Tick 번호입니다. 같은 방의 모든 클라이언트는 같은 Tick에 같은 상태를 받습니다.
    pub tick: u64,
    /// 이 방의 몇 번째 원정인지입니다. 첫 원정이 1입니다.
    pub expedition: u32,
    pub phase: WirePhase,
    pub wagon: WireWagon,
    /// 마차의 보관함에 있는 자원입니다. 모든 플레이어가 함께 씁니다.
    pub stash: u32,
    /// 짓는 중인 감시탑을 포함하고, 번호가 작은 감시탑부터입니다. 파괴된 감시탑은 없습니다.
    pub towers: Vec<WireTower>,
    /// 번호가 작은 채집 지점부터입니다. 위치는 `Welcome`에 있고 여기에는 남은 양만 있습니다.
    pub nodes: Vec<WireNode>,
    /// 땅에 떨어진 자원입니다. 번호가 작은 더미부터이고, 다 주워 간 더미는 없습니다.
    pub piles: Vec<WirePile>,
    /// 번호가 작은 플레이어부터입니다.
    pub players: Vec<WirePlayer>,
    /// 번호가 작은 몬스터부터입니다.
    pub monsters: Vec<WireMonster>,
    /// 날아가는 탄환과 이번 Tick에 끝난 탄환입니다. 번호가 작은 탄환부터입니다. 끝난 탄환이 한 번 실리므로 클라이언트가 탄환이 닿은 자리까지 그립니다.
    pub projectiles: Vec<WireProjectile>,
}

/// 원정의 단계입니다. 단계마다 그 단계에서 표시할 값만 가집니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WirePhase {
    /// `day`번째 날의 낮이고, `secs_left`초 뒤에 밤이 됩니다.
    Day { day: u32, secs_left: f32 },
    /// `day`번째 날의 밤이고, 그 밤의 무리는 `wave_count`개입니다.
    Night {
        day: u32,
        wave_count: u32,
        step: WireNightStep,
    },
    /// 밤의 무리를 모두 처치했고, `secs_left`초 뒤에 `next_day`번째 날의 낮이 시작됩니다.
    Dawn { next_day: u32, secs_left: f32 },
    /// 결과를 보여 주는 중이며, `secs_left`초 뒤에 새 원정이 시작됩니다.
    Over {
        outcome: WireOutcome,
        secs_left: f32,
    },
}

/// 밤 안에서의 진행입니다. 무리의 번호는 그 밤 안에서 1부터 셉니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireNightStep {
    /// `next_wave`번 무리가 나타나기까지 `secs_left`초 남았습니다.
    Waiting { next_wave: u32, secs_left: f32 },
    /// `wave`번 무리가 나타나는 중이거나 싸우는 중입니다.
    Fighting { wave: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireOutcome {
    Victory,
    Defeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWagon {
    pub hp: u32,
    pub max_hp: u32,
}

/// 채집 지점 하나의 남은 자원입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireNode {
    pub id: u32,
    pub amount: u32,
}

/// 땅에 떨어진 자원 한 더미입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePile {
    pub id: u32,
    pub pos: WirePos,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireTower {
    pub id: u32,
    pub tile: WireTile,
    pub max_hp: u32,
    pub state: WireTowerState,
}

/// 짓는 중인 감시탑에는 HP와 사격 대상이 없고, 서 있는 감시탑에는 건설의 진행도가 없습니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireTowerState {
    Building {
        /// 건설의 진행도(0.0에서 1.0)입니다.
        progress: f32,
    },
    Standing {
        hp: u32,
        last_shot: Option<WireShot>,
    },
}

/// 감시탑의 마지막 사격입니다. 클라이언트는 `tick`이 최근일 때만 `target`을 향한 사격 선을 그립니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireShot {
    /// 맞은 몬스터의 번호입니다. 그 몬스터가 이미 사라졌을 수 있습니다.
    pub target: u32,
    pub tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WirePlayer {
    pub id: u32,
    pub team: u8,
    pub pos: WirePos,
    pub facing: WireAngle,
    pub life: WireLife,
    /// 서버가 마지막으로 처리한 입력의 순서 번호입니다. 클라이언트 예측이 어디까지 확인되었는지 알려 줍니다.
    pub last_input_seq: u32,
    /// 마지막으로 공격한 서버 Tick입니다. 클라이언트는 스냅샷의 `tick`과 비교해서 공격 효과를 그립니다.
    pub last_attack_tick: Option<u64>,
    /// 들고 있는 무기입니다. 쓰러졌거나 사망한 플레이어도 무기를 들고 있습니다.
    pub weapon: WireWeaponKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireWeaponKind {
    Sword,
    Bow,
}

impl WireWeaponKind {
    /// `match`가 모든 종류를 다루므로, 규칙에 무기 종류를 추가하면 여기서 컴파일 오류가 납니다.
    pub fn of(kind: WeaponKind) -> WireWeaponKind {
        match kind {
            WeaponKind::Sword => WireWeaponKind::Sword,
            WeaponKind::Bow => WireWeaponKind::Bow,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireLife {
    Alive {
        hp: u32,
        max_hp: u32,
        /// 들고 있는 자원입니다. 상한은 `Welcome`의 `carry_capacity`입니다.
        carried: u32,
        /// 달리기 게이지가 가득 찬 값에 대한 백분율입니다(0에서 100). 모든 클라이언트가 받고, 화면은 자기 것만 표시합니다.
        stamina: u8,
        /// 상호작용 키나 건설 키가 지금 하고 있는 일입니다.
        interaction: Option<WireInteraction>,
        /// 건설 키를 누르고 있지만 세울 수 없을 때의 이유입니다. 모든 클라이언트가 받고, 화면은 자기 것만 표시합니다.
        build_refusal: Option<WireBuildRefusal>,
    },
    Downed {
        /// 남은 출혈 시간(초)입니다. 부활이 진행되는 동안에는 줄지 않습니다.
        bleed_secs_left: f32,
        /// 동료가 부활시키는 중이면 진행도(0.0에서 1.0)입니다.
        revive_progress: Option<f32>,
    },
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireInteraction {
    Reviving {
        /// 부활시키는 대상의 플레이어 번호입니다.
        target: u32,
        /// 부활의 진행도(0.0에서 1.0)입니다.
        progress: f32,
    },
    Repairing {
        target: WireRepairTarget,
        /// 보관함의 자원이 수리의 비용보다 적어서 다음 수리가 회복시키지 못합니다. 이벤트가 아니라 상태이고, 자원이 생기면 곧바로 거짓이 됩니다.
        stash_short: bool,
    },
    Building {
        /// 짓고 있는 감시탑의 번호입니다. 진행도는 스냅샷의 그 감시탑에 있습니다.
        tower: u32,
    },
    Gathering {
        /// 채집하는 지점의 번호입니다. 남은 양은 스냅샷의 그 지점에 있습니다.
        node: u32,
    },
}

/// 수리하는 대상입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireRepairTarget {
    Tower {
        /// 수리하는 감시탑의 번호입니다.
        tower: u32,
    },
    Wagon,
}

/// 건설이 거절된 이유와 놓으려던 타일입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireBuildRefusal {
    pub tile: WireTile,
    pub why: WireCannotBuild,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireCannotBuild {
    NotEnoughStash,
    TowerLimit,
    OutsideVillage,
    TileTaken,
    WouldBlockPath,
}

impl WireCannotBuild {
    /// `match`가 모든 이유를 다루므로, 규칙에 거절의 이유를 추가하면 여기서 컴파일 오류가 납니다.
    pub fn of(why: CannotBuild) -> WireCannotBuild {
        match why {
            CannotBuild::NotEnoughStash => WireCannotBuild::NotEnoughStash,
            CannotBuild::TowerLimit => WireCannotBuild::TowerLimit,
            CannotBuild::OutsideVillage => WireCannotBuild::OutsideVillage,
            CannotBuild::TileTaken => WireCannotBuild::TileTaken,
            CannotBuild::WouldBlockPath => WireCannotBuild::WouldBlockPath,
        }
    }
}

impl WireBuildRefusal {
    fn of(refusal: &BuildRefusal) -> WireBuildRefusal {
        WireBuildRefusal {
            tile: WireTile::from_tile(refusal.tile),
            why: WireCannotBuild::of(refusal.why),
        }
    }
}

/// 몬스터 하나입니다. 몬스터는 수백 마리일 수 있으므로 필드를 이 여섯 개로 한정합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireMonster {
    pub id: u32,
    pub kind: WireMonsterKind,
    pub pos: WirePos,
    pub facing: WireAngle,
    /// 현재 HP입니다. 최대 HP는 `Welcome`의 몬스터 종류표에 있습니다.
    pub hp: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireMonsterKind {
    Grunt,
}

impl WireMonsterKind {
    /// `match`가 모든 종류를 다루므로, 규칙에 몬스터 종류를 추가하면 여기서 컴파일 오류가 납니다.
    pub fn of(kind: MonsterKind) -> WireMonsterKind {
        match kind {
            MonsterKind::Grunt => WireMonsterKind::Grunt,
        }
    }
}

/// 탄환 하나입니다. 날아가는 방향으로 짧은 선을 그리는 데 필요한 값만 보냅니다. 피해량과 반지름은 보내지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireProjectile {
    pub id: u32,
    pub pos: WirePos,
    /// 날아가는 방향입니다.
    pub facing: WireAngle,
}

impl Snapshot {
    pub fn from_sim(sim: &Sim) -> Snapshot {
        let tuning = sim.tuning();
        Snapshot {
            tick: sim.tick(),
            expedition: sim.expedition_number(),
            phase: wire_phase(sim.phase(), tuning),
            wagon: WireWagon {
                hp: sim.wagon().hp,
                max_hp: tuning.wagon.max_hp,
            },
            stash: sim.stash(),
            towers: sim
                .towers()
                .map(|tower| wire_tower(tower, tuning))
                .collect(),
            nodes: sim.nodes().map(wire_node).collect(),
            piles: sim.piles().map(wire_pile).collect(),
            players: sim
                .players()
                .map(|player| wire_player(sim, player))
                .collect(),
            monsters: sim.monsters().map(wire_monster).collect(),
            projectiles: sim.projectiles().map(wire_projectile).collect(),
        }
    }
}

/// Tick 수를 초로 바꿉니다. 나눗셈으로 구하므로 3초 분량의 Tick은 정확히 3.0입니다.
fn secs(ticks: u32, tuning: &Tuning) -> f32 {
    ticks as f32 / tuning.tick_hz as f32
}

fn wire_phase(phase: Phase, tuning: &Tuning) -> WirePhase {
    match phase {
        Phase::Day { day, ticks_left } => WirePhase::Day {
            day,
            secs_left: secs(ticks_left, tuning),
        },
        Phase::Night { day, step } => WirePhase::Night {
            day,
            wave_count: tuning.nights.wave_count(day),
            step: match step {
                NightStep::Waiting {
                    next_wave,
                    ticks_left,
                } => WireNightStep::Waiting {
                    next_wave,
                    secs_left: secs(ticks_left, tuning),
                },
                NightStep::Fighting { wave, .. } => WireNightStep::Fighting { wave },
            },
        },
        Phase::Dawn {
            next_day,
            ticks_left,
        } => WirePhase::Dawn {
            next_day,
            secs_left: secs(ticks_left, tuning),
        },
        Phase::Over {
            outcome,
            ticks_left,
        } => WirePhase::Over {
            outcome: match outcome {
                Outcome::Victory => WireOutcome::Victory,
                Outcome::Defeat => WireOutcome::Defeat,
            },
            secs_left: secs(ticks_left, tuning),
        },
    }
}

fn wire_tower(tower: &Tower, tuning: &Tuning) -> WireTower {
    let state = match &tower.state {
        TowerState::Building { .. } => WireTowerState::Building {
            progress: clamp_progress(tower.build_progress(tuning).unwrap_or(0.0)),
        },
        TowerState::Standing { hp, last_shot, .. } => WireTowerState::Standing {
            hp: *hp,
            last_shot: last_shot.map(|shot| WireShot {
                target: shot.target.0,
                tick: shot.tick,
            }),
        },
    };
    WireTower {
        id: tower.id.0,
        tile: WireTile::from_tile(tower.tile),
        max_hp: tuning.structures.get(tower.kind).max_hp,
        state,
    }
}

fn wire_node(node: &GatherNode) -> WireNode {
    WireNode {
        id: node.id.0,
        amount: node.amount,
    }
}

fn wire_pile(pile: &Pile) -> WirePile {
    WirePile {
        id: pile.id.0,
        pos: WirePos::from_tiles(pile.pos),
        amount: pile.amount,
    }
}

fn wire_player(sim: &Sim, player: &Player) -> WirePlayer {
    let tuning = sim.tuning();
    let life = match &player.life {
        Life::Alive {
            hp,
            carried,
            stamina,
            interaction,
            build_refusal,
            ..
        } => WireLife::Alive {
            hp: *hp,
            max_hp: tuning.player.max_hp,
            carried: *carried,
            stamina: stamina.percent(tuning),
            interaction: interaction
                .as_ref()
                .map(|interaction| wire_interaction(sim, player, interaction)),
            build_refusal: build_refusal.as_ref().map(WireBuildRefusal::of),
        },
        Life::Downed { bleed_left_ticks } => WireLife::Downed {
            bleed_secs_left: secs(*bleed_left_ticks, tuning),
            revive_progress: sim.revive_progress(player.id).map(clamp_progress),
        },
        Life::Dead => WireLife::Dead,
    };
    WirePlayer {
        id: player.id.0,
        team: player.team.0,
        pos: WirePos::from_tiles(player.pos),
        facing: WireAngle::from_direction(player.facing),
        life,
        last_input_seq: player.last_input_seq,
        last_attack_tick: player.last_attack_tick,
        weapon: WireWeaponKind::of(player.weapon),
    }
}

fn wire_interaction(sim: &Sim, player: &Player, interaction: &Interaction) -> WireInteraction {
    match interaction {
        Interaction::Reviving { target, .. } => WireInteraction::Reviving {
            target: target.0,
            // 진행도는 부활시키는 플레이어에게 있으므로 `Sim`이 그 플레이어를 찾아서 계산합니다.
            // 대상이 방금 사라진 Tick에는 값이 없으므로 0으로 둡니다. 부활은 다음 Tick에 취소됩니다.
            progress: sim.revive_progress(*target).map_or(0.0, clamp_progress),
        },
        Interaction::Repairing { target, .. } => WireInteraction::Repairing {
            target: match target {
                RepairTarget::Tower(tower) => WireRepairTarget::Tower { tower: tower.0 },
                RepairTarget::Wagon => WireRepairTarget::Wagon,
            },
            stash_short: sim.lacks_stash_to_repair(player.id),
        },
        Interaction::Gathering { node, .. } => WireInteraction::Gathering { node: node.0 },
        Interaction::Building { tower } => WireInteraction::Building { tower: tower.0 },
    }
}

/// 진행도가 0.0에서 1.0 사이라는 약속을 전송 형식에서 지킵니다.
fn clamp_progress(progress: f32) -> f32 {
    progress.clamp(0.0, 1.0)
}

fn wire_projectile(projectile: &Projectile) -> WireProjectile {
    WireProjectile {
        id: projectile.id.0,
        pos: WirePos::from_tiles(projectile.pos),
        facing: WireAngle::from_direction(projectile.dir),
    }
}

fn wire_monster(monster: &Monster) -> WireMonster {
    WireMonster {
        id: monster.id.0,
        kind: WireMonsterKind::of(monster.kind),
        pos: WirePos::from_tiles(monster.pos),
        facing: WireAngle::from_direction(monster.facing),
        hp: monster.hp,
    }
}
