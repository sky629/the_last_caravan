//! 원정의 진행입니다. 원정은 첫날의 낮부터 성공이나 실패까지의 한 판이고, 날마다 낮 한 번과 밤 한 번이 있습니다.
//! 낮의 시간이 끝나면 밤이 되고, 밤의 무리를 한 마리씩 등장시키고, 무리를 모두 처치하면 다음 날의 낮이 시작됩니다.
//! 마지막 밤을 막거나 실패하면 원정이 끝나고, 방은 결과 화면(`Stage::Debrief`)으로 넘어갑니다.
//!
//! 단계를 바꾸는 곳은 이 파일의 `advance_phase`와 `settle_outcome` 둘뿐입니다.
//! 단계가 바뀔 때 함께 일어나는 일(낮이 시작되면 모든 플레이어가 회복되고 낮의 몬스터가 놓이고, 밤이 되면 낮의 몬스터가 마차를 향함)도
//! 단계를 바꾸는 바로 그 자리에서 부르므로, 단계와 그 일이 어긋날 수 없습니다.
//! 첫날의 낮은 `Sim::begin_expedition`이 `begin_day`를 불러 시작합니다.

use crate::boss;
use crate::camp::who_must_agree;
use crate::loot::{self, Seed};
use crate::math::Vec2;
use crate::monster::Order;
use crate::resources;
use crate::spawn::SpawnPlan;
use crate::tuning::{Finale, Tuning};
use crate::world::World;

/// 원정의 단계입니다. 단계마다 그 단계에서만 의미 있는 값을 가집니다. 그래서 「낮의 남은 시간」이나 「현재 무리 번호」 같은 별도의 필드가 없고,
/// 단계와 어긋날 수 없습니다.
///
/// ```text
///   새 원정 -> Day{1} --낮의 시간이 끝남--> Night{1, Waiting{1}} --대기--> Night{1, Fighting{1}}
///   Night{d, Fighting{w}} --몬스터가 모두 사라짐--> Night{d, Waiting{w + 1}}                (그 밤의 마지막 무리가 아니면)
///   Night{d, Fighting{마지막 무리}} --몬스터가 모두 사라짐--> Dawn{d + 1} --대기--> Day{d + 1}    (마지막 밤이 아니면)
///   Night{마지막 밤, Fighting{마지막 무리}} --몬스터가 모두 사라짐--> Night{마지막 밤, Warning} --경고가 끝남--> Night{마지막 밤, Boss}   (보스가 있는 설정)
///   Night{마지막 밤, Fighting{마지막 무리}} --몬스터가 모두 사라짐--> 원정이 끝남(Victory)                                      (보스가 없는 설정)
///   Night{마지막 밤, Boss} --보스가 쓰러짐--> 원정이 끝남(Victory)
///   모든 단계 --마차 HP 0, 또는 살아 있는 플레이어 없음--> 원정이 끝남(Defeat)
/// ```
/// 원정이 끝나면 `settle_outcome`이 결과를 돌려주고, `Sim`이 방을 결과 화면으로 옮깁니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// `day`번째 날의 낮입니다. `ticks_left`가 지나면 밤이 됩니다.
    Day { day: u32, ticks_left: u32 },
    /// `day`번째 날의 밤입니다. 그 밤의 무리가 차례로 나옵니다. `party`는 밤이 시작될 때 방에 있던 플레이어의 수(자리를 비운 플레이어 포함)이고,
    /// 그 밤의 무리의 크기를 정합니다(`NightTuning::wave_size`). 밤 도중에 들어오거나 나가도 바뀌지 않습니다.
    Night {
        day: u32,
        party: u32,
        step: NightStep,
    },
    /// 밤의 무리를 모두 처치했고, `ticks_left`가 지나면 `next_day`번째 날의 낮이 시작됩니다.
    Dawn { next_day: u32, ticks_left: u32 },
}

/// 밤 안에서의 진행입니다. 무리의 번호는 그 밤 안에서 1부터 셉니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NightStep {
    /// `next_wave`번 무리가 나타나기를 기다립니다. 플레이어는 움직이고, 싸우고, 부활과 수리와 건설을 할 수 있습니다.
    Waiting { next_wave: u32, ticks_left: u32 },
    /// `wave`번 무리가 나타나는 중이거나 싸우는 중입니다.
    /// 무리는 한꺼번에 나타나지 않고 몬스터가 한 마리씩 나타납니다.
    Fighting {
        wave: u32,
        /// 아직 나타나지 않은 몬스터의 수입니다.
        to_spawn: u32,
        /// 다음 몬스터가 나타날 때까지 남은 Tick입니다.
        spawn_in_ticks: u32,
    },
    /// 마지막 밤의 마지막 무리를 모두 처치했고, `ticks_left`가 지나면 보스가 나타납니다. 플레이어는 움직이고, 싸우고, 부활과 수리와 건설을 할 수 있습니다.
    Warning { ticks_left: u32 },
    /// 보스가 있습니다. 보스의 상태는 `World::boss`에 있고, 보스가 쓰러지면 원정이 성공으로 끝납니다.
    Boss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Victory,
    Defeat,
}

/// 새 원정이 시작될 때의 단계입니다. 원정은 첫날의 낮으로 시작합니다.
pub(crate) fn opening_phase(t: &Tuning) -> Phase {
    Phase::Day {
        day: 1,
        ticks_left: t.ticks(t.cycle.day_secs),
    }
}

/// Tick의 맨 앞에서 카운트다운을 진행하고, 때가 되면 밤이나 낮을 시작하거나 몬스터를 한 마리 등장시킵니다.
pub(crate) fn advance_phase(
    phase: &mut Phase,
    world: &mut World,
    spawns: &SpawnPlan,
    expedition: Seed,
    t: &Tuning,
) {
    match *phase {
        Phase::Day { day, ticks_left } => {
            *phase = if ticks_left > 1 {
                Phase::Day {
                    day,
                    ticks_left: ticks_left - 1,
                }
            } else {
                nightfall(world);
                Phase::Night {
                    day,
                    party: world.players.len() as u32,
                    step: NightStep::Waiting {
                        next_wave: 1,
                        ticks_left: t.ticks(t.nights.first_wave_delay_secs),
                    },
                }
            };
        }
        Phase::Dawn {
            next_day,
            ticks_left,
        } => {
            if ticks_left > 1 {
                *phase = Phase::Dawn {
                    next_day,
                    ticks_left: ticks_left - 1,
                };
            } else {
                *phase = Phase::Day {
                    day: next_day,
                    ticks_left: t.ticks(t.cycle.day_secs),
                };
                begin_day(world, spawns, expedition, next_day, t);
            }
        }
        Phase::Night { day, party, step } => match step {
            NightStep::Waiting {
                next_wave,
                ticks_left,
            } => {
                if ticks_left > 1 {
                    *phase = Phase::Night {
                        day,
                        party,
                        step: NightStep::Waiting {
                            next_wave,
                            ticks_left: ticks_left - 1,
                        },
                    };
                } else {
                    let to_spawn = t.nights.wave_size(day, next_wave, party);
                    *phase = Phase::Night {
                        day,
                        party,
                        step: NightStep::Fighting {
                            wave: next_wave,
                            to_spawn,
                            spawn_in_ticks: 0,
                        },
                    };
                    spawn_next(phase, world, spawns, t);
                }
            }
            NightStep::Fighting { .. } => spawn_next(phase, world, spawns, t),
            NightStep::Warning { ticks_left } => {
                *phase = if ticks_left > 1 {
                    Phase::Night {
                        day,
                        party,
                        step: NightStep::Warning {
                            ticks_left: ticks_left - 1,
                        },
                    }
                } else {
                    // 경고는 보스가 있는 설정에서만 시작합니다. 보스가 없는 설정에서 단계만 바꾼 경우에는 보스 없이 보스의 단계가 되고, 곧바로 성공으로 끝납니다.
                    if let Finale::Boss(cfg) = &t.nights.finale {
                        world.spawn_boss(boss::arrival_spot(spawns, t), party, cfg, t);
                    }
                    Phase::Night {
                        day,
                        party,
                        step: NightStep::Boss,
                    }
                };
            }
            NightStep::Boss => {}
        },
    }
}

/// 낮이 시작됩니다. 원정의 첫날과 밤을 막은 뒤의 새 날, 하루가 시작되는 모든 때에 한 번씩 부릅니다.
///
/// 모든 플레이어가 최대 HP의 살아 있는 상태가 됩니다. 쓰러졌거나 사망한 플레이어도 그 자리에서 되살아납니다.
/// 위치, 하고 있던 일, 든 자원, 살아 있던 플레이어가 받은 버프는 그대로입니다. 모든 채집 지점이 가득 다시 채워지고, 지점마다 낮의 몬스터가 자리를 지키며 섭니다.
/// 밤을 막고 나면 몬스터가 모두 사라졌으므로 낮의 몬스터가 겹쳐서 놓이지 않습니다.
/// 무기 상자가 놓이고 상자마다 몬스터가 선 뒤입니다(`loot::start_day`). 어느 후보 자리에 어떤 무기가 놓이는지는 원정의 씨앗(`expedition`)과 날(`day`)이 정합니다.
/// 가진 무기, 바닥의 무기, 포탑과 그 HP, 마차의 HP, 보관함의 자원, 떨어진 자원은 이 함수가 건드리지 않으므로 다음 날로 그대로 이어집니다.
pub(crate) fn begin_day(
    world: &mut World,
    spawns: &SpawnPlan,
    expedition: Seed,
    day: u32,
    t: &Tuning,
) {
    for player in world.players.values_mut() {
        player.restore_for_new_day(t);
    }
    resources::refill_nodes(world, t);
    for post in spawns.guard_posts() {
        world.spawn_monster(t.guards.kind, Order::Guard { post }, post.center(), t);
    }
    loot::start_day(world, spawns, expedition, day, t);
}

/// 밤이 시작됩니다. 낮에 처치하지 않은 몬스터는 자리를 떠나 마차를 향하고, 그때부터 밤의 몬스터와 같이 움직입니다.
/// 열지 않은 무기 상자는 사라지고 타일이 비워집니다. 상자를 지키던 몬스터도 채집 지점을 지키던 몬스터와 같이 마차를 향합니다.
fn nightfall(world: &mut World) {
    for monster in &mut world.monsters {
        monster.order = Order::Raid;
    }
    loot::clear_chests(world);
}

/// 나타날 몬스터가 남아 있고 때가 되었으면 한 마리를 등장시킵니다. 그렇지 않으면 카운트다운만 진행합니다.
/// 살아 있는 몬스터가 상한에 이르렀으면 카운트다운을 멈추고 기다리므로, 한 마리가 죽는 Tick에 다음 몬스터가 나타납니다.
fn spawn_next(phase: &mut Phase, world: &mut World, spawns: &SpawnPlan, t: &Tuning) {
    let Phase::Night {
        day,
        party,
        step:
            NightStep::Fighting {
                wave,
                to_spawn,
                spawn_in_ticks,
            },
    } = *phase
    else {
        return;
    };
    if to_spawn == 0 || world.monsters.len() as u32 >= t.nights.max_alive {
        return;
    }
    let fighting = |to_spawn, spawn_in_ticks| Phase::Night {
        day,
        party,
        step: NightStep::Fighting {
            wave,
            to_spawn,
            spawn_in_ticks,
        },
    };
    if spawn_in_ticks > 0 {
        *phase = fighting(to_spawn, spawn_in_ticks - 1);
        return;
    }
    let Some(plan) = t.nights.wave(day, wave) else {
        return;
    };
    let nth = t.nights.wave_size(day, wave, party) - to_spawn;
    let spot: Vec2 = spawns.night_spot(t.nights.wave_serial(day, wave), nth);
    world.spawn_monster(plan.kind, Order::Raid, spot, t);
    *phase = fighting(
        to_spawn - 1,
        t.ticks(t.nights.spawn_interval_secs).saturating_sub(1),
    );
}

/// 단계가 속한 날입니다. 새벽은 막 지난 밤의 날입니다.
pub(crate) fn day_of(phase: Phase) -> u32 {
    match phase {
        Phase::Day { day, .. } | Phase::Night { day, .. } => day,
        Phase::Dawn { next_day, .. } => next_day.saturating_sub(1),
    }
}

/// Tick의 맨 끝에서 원정이 끝났는지, 무리를 모두 처치했는지 판정합니다. 원정이 끝났으면 결과를 돌려주고, 단계는 그대로 둡니다.
/// 성공은 보스가 없는 설정(`Finale::LastWave`)에서는 마지막 밤의 마지막 무리를 모두 처치한 때이고, 보스가 있는 설정에서는 마지막 무리를 처치한 뒤 경고가 끝나고 나타난 보스가 쓰러진 때입니다.
/// 실패를 먼저 검사하므로, 마지막 몬스터가 죽은 Tick에 마차도 부서졌다면 실패입니다. 실패는 낮에도 밤에도 같습니다.
/// 실패는 자리에 있는 플레이어를 기준으로 판정합니다. 자리에 있는 사람이 모두 쓰러지거나 죽으면 실패이고,
/// 보호받는 캐릭터가 살아 있다고 해서 실패가 미뤄지지 않습니다. 자리에 있는 플레이어가 한 명도 없는 방은 실패하지 않습니다.
pub(crate) fn settle_outcome(phase: &mut Phase, world: &World, t: &Tuning) -> Option<Outcome> {
    let mut present = who_must_agree(world).peekable();
    let defeated = present.peek().is_some()
        && (world.wagon.hp == 0 || present.all(|player| !player.is_alive()));
    if defeated {
        return Some(Outcome::Defeat);
    }
    match *phase {
        Phase::Night {
            day,
            party,
            step: NightStep::Fighting {
                wave, to_spawn: 0, ..
            },
        } if world.monsters.is_empty() => {
            if wave < t.nights.wave_count(day) {
                *phase = Phase::Night {
                    day,
                    party,
                    step: NightStep::Waiting {
                        next_wave: wave + 1,
                        ticks_left: t.ticks(t.nights.between_waves_secs),
                    },
                };
            } else if day < t.day_count() {
                *phase = Phase::Dawn {
                    next_day: day + 1,
                    ticks_left: t.ticks(t.cycle.dawn_secs),
                };
            } else if let Finale::Boss(cfg) = &t.nights.finale {
                *phase = Phase::Night {
                    day,
                    party,
                    step: NightStep::Warning {
                        ticks_left: t.ticks(cfg.warning_secs),
                    },
                };
            } else {
                return Some(Outcome::Victory);
            }
        }
        Phase::Night {
            step: NightStep::Boss,
            ..
        } if world.boss.is_none() => return Some(Outcome::Victory),
        Phase::Day { .. } | Phase::Night { .. } | Phase::Dawn { .. } => {}
    }
    None
}
