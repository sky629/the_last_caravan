//! 원정(밤의 방어 한 번)의 진행입니다. 첫 무리를 기다리고, 무리를 한 마리씩 등장시키고, 다음 무리를 기다리다가,
//! 성공이나 실패로 끝나면 결과를 보여 주고 새 원정을 시작합니다.

use crate::math::Vec2;
use crate::tuning::Tuning;
use crate::world::World;

/// 원정의 단계입니다. 단계마다 그 단계에서만 의미 있는 값을 가집니다. 그래서 「현재 무리 번호」 같은 별도의 필드가 없고,
/// 단계와 어긋날 수 없습니다. 단계를 바꾸는 곳은 이 파일의 `advance_phase`와 `settle_outcome`입니다.
///
/// ```text
///   새 원정 -> Intermission{1} --카운트다운--> Fighting{1} --몬스터가 모두 사라짐--> Intermission{2} -> ...
///   Fighting{마지막 무리} --몬스터가 모두 사라짐--> Over{Victory}
///   Over를 뺀 모든 단계 --마차 HP 0, 또는 살아 있는 플레이어 없음--> Over{Defeat}
///   Over{..} --카운트다운--> 새 원정 (Sim::reset_expedition)
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// `next_wave`번 무리의 등장을 기다립니다. 플레이어는 움직이고, 싸우고, 부활과 수리를 할 수 있습니다.
    Intermission { next_wave: u32, ticks_left: u32 },
    /// `wave`번 무리가 나타나는 중이거나 싸우는 중입니다.
    /// 무리는 한꺼번에 나타나지 않고 몬스터가 한 마리씩 나타납니다.
    Fighting {
        wave: u32,
        /// 아직 나타나지 않은 몬스터의 수입니다.
        to_spawn: u32,
        /// 다음 몬스터가 나타날 때까지 남은 Tick입니다.
        spawn_in_ticks: u32,
    },
    /// 결과를 보여 주는 동안 세계가 멈춥니다. 카운트다운이 끝나면 새 원정이 시작됩니다.
    Over { outcome: Outcome, ticks_left: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Victory,
    Defeat,
}

/// 새 원정이 시작될 때의 단계입니다.
pub(crate) fn opening_phase(t: &Tuning) -> Phase {
    Phase::Intermission {
        next_wave: 1,
        ticks_left: t.ticks(t.waves.first_delay_secs),
    }
}

/// `Sim::step`이 이번 Tick의 나머지를 어떻게 처리할지 정합니다.
pub(crate) enum Advance {
    Simulate,
    /// 결과를 보여 주는 중이므로 세계를 진행하지 않습니다.
    Frozen,
    StartNewExpedition,
}

/// Tick의 맨 앞에서 카운트다운을 진행하고, 때가 되면 무리를 시작하거나 몬스터를 한 마리 등장시킵니다.
pub(crate) fn advance_phase(phase: &mut Phase, world: &mut World, t: &Tuning) -> Advance {
    match *phase {
        Phase::Over {
            outcome,
            ticks_left,
        } => {
            if ticks_left <= 1 {
                return Advance::StartNewExpedition;
            }
            *phase = Phase::Over {
                outcome,
                ticks_left: ticks_left - 1,
            };
            Advance::Frozen
        }
        Phase::Intermission {
            next_wave,
            ticks_left,
        } => {
            if ticks_left > 1 {
                *phase = Phase::Intermission {
                    next_wave,
                    ticks_left: ticks_left - 1,
                };
            } else {
                *phase = Phase::Fighting {
                    wave: next_wave,
                    to_spawn: t.waves.size_of(next_wave),
                    spawn_in_ticks: 0,
                };
                spawn_next(phase, world, t);
            }
            Advance::Simulate
        }
        Phase::Fighting { .. } => {
            spawn_next(phase, world, t);
            Advance::Simulate
        }
    }
}

/// 나타날 몬스터가 남아 있고 때가 되었으면 한 마리를 등장시킵니다. 그렇지 않으면 카운트다운만 진행합니다.
fn spawn_next(phase: &mut Phase, world: &mut World, t: &Tuning) {
    let Phase::Fighting {
        wave,
        to_spawn,
        spawn_in_ticks,
    } = *phase
    else {
        return;
    };
    if to_spawn == 0 {
        return;
    }
    if spawn_in_ticks > 0 {
        *phase = Phase::Fighting {
            wave,
            to_spawn,
            spawn_in_ticks: spawn_in_ticks - 1,
        };
        return;
    }
    let already_spawned = t.waves.size_of(wave) - to_spawn;
    world.spawn_monster(t.waves.kind, spawn_position(t, wave, already_spawned), t);
    *phase = Phase::Fighting {
        wave,
        to_spawn: to_spawn - 1,
        spawn_in_ticks: t.ticks(t.waves.spawn_interval_secs).saturating_sub(1),
    };
}

/// `wave`번 무리의 `nth`번째 몬스터가 나타날 위치입니다.
/// 무리마다 맵 가장자리의 등장 지점이 정해져 있고, 몬스터는 그 지점에서 가장자리를 따라 나란히 선 줄 가운데 하나에 나타납니다.
/// 난수를 쓰지 않으므로 같은 수치는 항상 같은 위치를 만들고, 겹쳐 나타나는 몬스터가 서로 다른 자리에 서게 됩니다.
fn spawn_position(t: &Tuning, wave: u32, nth: u32) -> Vec2 {
    let map = &t.map;
    let center = Vec2::new(map.width * 0.5, map.height * 0.5);
    let slot = (wave as usize).saturating_sub(1) % map.monster_spawns.len().max(1);
    let anchor = map.monster_spawns.get(slot).copied().unwrap_or(center);

    let inward = (center - anchor).normalize_or_zero();
    let along_edge = Vec2::new(-inward.y, inward.x);
    let lanes = t.waves.spawn_lanes.max(1);
    let lane = (nth % lanes) as f32 - (lanes - 1) as f32 * 0.5;
    anchor + along_edge * (lane * t.waves.spawn_lane_spacing)
}

/// Tick의 맨 끝에서 원정이 끝났는지, 무리를 모두 처치했는지 판정합니다.
/// 실패를 먼저 검사하므로, 마지막 몬스터가 죽은 Tick에 마차도 부서졌다면 실패입니다.
/// 플레이어가 한 명도 없는 방은 실패하지 않습니다.
pub(crate) fn settle_outcome(phase: &mut Phase, world: &World, t: &Tuning) {
    if matches!(phase, Phase::Over { .. }) {
        return;
    }
    let defeated = !world.players.is_empty()
        && (world.wagon.hp == 0 || world.players.values().all(|player| !player.is_alive()));
    if defeated {
        *phase = Phase::Over {
            outcome: Outcome::Defeat,
            ticks_left: t.ticks(t.waves.restart_secs),
        };
        return;
    }
    if let Phase::Fighting {
        wave, to_spawn: 0, ..
    } = *phase
        && world.monsters.is_empty()
    {
        *phase = if wave >= t.wave_count() {
            Phase::Over {
                outcome: Outcome::Victory,
                ticks_left: t.ticks(t.waves.restart_secs),
            }
        } else {
            Phase::Intermission {
                next_wave: wave + 1,
                ticks_left: t.ticks(t.waves.between_secs),
            }
        };
    }
}
