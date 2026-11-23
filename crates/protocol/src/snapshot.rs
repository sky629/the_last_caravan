//! 규칙의 상태를 전송 형식으로 바꾸는 곳입니다. 게임 상태가 규칙에서 전송 형식으로 건너가는 길은 `Snapshot::from_sim` 하나입니다.
//!
//! 전송 형식의 타입은 규칙의 타입과 따로 정의합니다. 규칙의 내부 표현이 바뀌어도 전송 형식은 이 파일에서만 고칩니다.
//! 값은 클라이언트가 수치표를 읽지 않고 곧바로 그릴 수 있게 정합니다.
//! 시간은 Tick이 아니라 초로, 진행도는 0.0에서 1.0 사이의 실수로, 위치와 방향은 `units`의 정수로 보냅니다.
//! 번호는 규칙의 번호 타입이 아니라 `u32`입니다. 규칙의 타입은 전송 형식에 나타나지 않습니다.
//!
//! 몬스터는 수백 마리일 수 있으므로 `WireMonster` 하나가 postcard로 9바이트 안팎이 되게 정했습니다
//! (번호 2, 종류 1, 위치 4, 방향 1, HP 1). postcard는 필드 이름을 기록하지 않으므로 필드 이름은 크기에 영향이 없습니다.

use caravan_rules::{
    Interaction, Life, Monster, MonsterKind, Outcome, Phase, Player, Sim, Tower, TowerState, Tuning,
};
use serde::{Deserialize, Serialize};

use crate::units::{WireAngle, WirePos};

/// Tick 하나가 끝난 뒤의 방 전체 상태입니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 서버의 Tick 번호입니다. 같은 방의 모든 클라이언트는 같은 Tick에 같은 상태를 받습니다.
    pub tick: u64,
    /// 이 방의 몇 번째 원정인지입니다. 첫 원정이 1입니다.
    pub expedition: u32,
    pub phase: WirePhase,
    /// 한 원정의 무리 수입니다. 「무리 2 / 3」 같은 표시에 씁니다.
    pub wave_count: u32,
    pub wagon: WireWagon,
    /// 번호가 작은 시설부터입니다.
    pub towers: Vec<WireTower>,
    /// 번호가 작은 플레이어부터입니다.
    pub players: Vec<WirePlayer>,
    /// 번호가 작은 몬스터부터입니다.
    pub monsters: Vec<WireMonster>,
}

/// 원정의 단계입니다. 단계마다 그 단계에서 표시할 값만 가집니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WirePhase {
    /// `next_wave`번 무리가 나타나기까지 `secs_left`초 남았습니다.
    Intermission { next_wave: u32, secs_left: f32 },
    /// `wave`번 무리가 나타나는 중이거나 싸우는 중입니다.
    Fighting { wave: u32 },
    /// 결과를 보여 주는 중이며, `secs_left`초 뒤에 새 원정이 시작됩니다.
    Over {
        outcome: WireOutcome,
        secs_left: f32,
    },
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireTower {
    pub id: u32,
    pub max_hp: u32,
    pub state: WireTowerState,
}

/// 파괴된 감시탑에는 HP와 사격 대상이 없습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireTowerState {
    Standing {
        hp: u32,
        last_shot: Option<WireShot>,
    },
    Destroyed,
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
    /// 마지막으로 근접 공격을 휘두른 서버 Tick입니다. 클라이언트는 스냅샷의 `tick`과 비교해서 공격 효과를 그립니다.
    pub last_attack_tick: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireLife {
    Alive {
        hp: u32,
        max_hp: u32,
        /// 상호작용 키가 지금 하고 있는 일입니다.
        interaction: Option<WireInteraction>,
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
        /// 수리하는 감시탑의 번호입니다.
        tower: u32,
    },
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

impl Snapshot {
    pub fn from_sim(sim: &Sim) -> Snapshot {
        let tuning = sim.tuning();
        Snapshot {
            tick: sim.tick(),
            expedition: sim.expedition_number(),
            phase: wire_phase(sim.phase(), tuning),
            wave_count: tuning.wave_count(),
            wagon: WireWagon {
                hp: sim.wagon().hp,
                max_hp: tuning.wagon.max_hp,
            },
            towers: sim
                .towers()
                .map(|tower| wire_tower(tower, tuning))
                .collect(),
            players: sim
                .players()
                .map(|player| wire_player(sim, player))
                .collect(),
            monsters: sim.monsters().map(wire_monster).collect(),
        }
    }
}

/// Tick 수를 초로 바꿉니다. 나눗셈으로 구하므로 3초 분량의 Tick은 정확히 3.0입니다.
fn secs(ticks: u32, tuning: &Tuning) -> f32 {
    ticks as f32 / tuning.tick_hz as f32
}

fn wire_phase(phase: Phase, tuning: &Tuning) -> WirePhase {
    match phase {
        Phase::Intermission {
            next_wave,
            ticks_left,
        } => WirePhase::Intermission {
            next_wave,
            secs_left: secs(ticks_left, tuning),
        },
        Phase::Fighting { wave, .. } => WirePhase::Fighting { wave },
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
        TowerState::Standing { hp, last_shot, .. } => WireTowerState::Standing {
            hp: *hp,
            last_shot: last_shot.map(|shot| WireShot {
                target: shot.target.0,
                tick: shot.tick,
            }),
        },
        TowerState::Destroyed => WireTowerState::Destroyed,
    };
    WireTower {
        id: tower.id.0,
        max_hp: tuning.structures.get(tower.kind).max_hp,
        state,
    }
}

fn wire_player(sim: &Sim, player: &Player) -> WirePlayer {
    let tuning = sim.tuning();
    let life = match &player.life {
        Life::Alive {
            hp, interaction, ..
        } => WireLife::Alive {
            hp: *hp,
            max_hp: tuning.player.max_hp,
            interaction: interaction
                .as_ref()
                .map(|interaction| wire_interaction(sim, interaction)),
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
    }
}

fn wire_interaction(sim: &Sim, interaction: &Interaction) -> WireInteraction {
    match interaction {
        Interaction::Reviving { target, .. } => WireInteraction::Reviving {
            target: target.0,
            // 진행도는 부활시키는 플레이어에게 있으므로 `Sim`이 그 플레이어를 찾아서 계산합니다.
            // 대상이 방금 사라진 Tick에는 값이 없으므로 0으로 둡니다. 부활은 다음 Tick에 취소됩니다.
            progress: sim.revive_progress(*target).map_or(0.0, clamp_progress),
        },
        Interaction::Repairing { tower, .. } => WireInteraction::Repairing { tower: tower.0 },
    }
}

/// 진행도가 0.0에서 1.0 사이라는 약속을 전송 형식에서 지킵니다.
fn clamp_progress(progress: f32) -> f32 {
    progress.clamp(0.0, 1.0)
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
