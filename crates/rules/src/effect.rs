//! 대상에 붙었다가 시간이 지나면 끝나는 효과입니다. 몬스터가 입는 효과(감속, 출혈, 스턴)는 `MonsterEffects`이고,
//! 플레이어가 받는 버프(공격 속도, 이동 속도)는 `PlayerBuffs`입니다. 두 타입을 하나로 합치지 않으므로,
//! 스턴에 걸린 플레이어나 공격 속도 버프를 받은 몬스터는 코드로 만들 수 없습니다.
//!
//! 몬스터의 효과는 몬스터가 가진 상태(`Monster::effects`)이므로 몬스터가 사라지면 함께 사라집니다.
//! 플레이어의 버프는 살아 있는 플레이어(`Life::Alive`)만 가지므로, 플레이어가 쓰러지거나 새 원정이 시작되면 함께 사라집니다.
//! 효과마다 필드가 하나이고, 효과 하나의 값을 바꾸는 길은 `apply`(버프는 `grant`)와 `advance` 둘뿐입니다. 필드가 이 파일 밖에서는 보이지 않으므로,
//! 다른 파일은 스턴이 걸려 있거나 면역인 몬스터에게 스턴을 걸 수 없고, 남은 시간이나 세기를 직접 고칠 수 없습니다.
//! 같은 효과가 겹치면 센 쪽이 칸에 남고, 약한 쪽은 버려지며 따로 기억하지 않습니다(`new_one_stays`).
//! 다른 파일이 할 수 있는 것은 효과 전체를 빈 값(`MonsterEffects::default()`, `PlayerBuffs::default()`)으로 두는 것뿐이고, 새 몬스터와 살아난 플레이어를 만들 때 그렇게 합니다.
//!
//! 한 Tick의 순서는 다음과 같습니다(`Sim::simulate`).
//!
//! ```text
//!   (플레이어는 `Player::stats`가 버프를 읽으며 행동합니다)
//!   monsters_act       몬스터는 이전 Tick까지 걸린 효과로 행동합니다. (감속은 `Monster::stats`, 스턴은 `monster::plan`이 읽습니다)
//!   advance_effects    몬스터의 효과와 플레이어의 버프의 시간을 한 Tick 줄이고, 이번 Tick에 나는 출혈의 피해를 `hits`의 끝에 더합니다.
//!   apply_hits         모든 피해를 적용합니다. 출혈의 피해도 여기를 지나므로 처치의 주인이 정해집니다.
//!   apply_debuffs      이번 Tick에 맞은 몬스터 가운데 살아남은 몬스터에게 새 효과를 겁니다. 이 효과는 다음 Tick의 행동부터 바꿉니다.
//! ```
//!
//! 행동한 뒤에 줄이고 새 효과를 그 뒤에 걸기 때문에, `secs`초짜리 효과는 정확히 `secs`초 분량의 Tick 동안 대상의 행동을 바꿉니다.

use std::cmp::Ordering;

use crate::player::Life;
use crate::tuning::{Buff, Debuff, Tuning};
use crate::world::{Ctx, Hit, Source, Target, World};

/// 같은 칸에 걸려 있는 효과와 새로 받은 효과 가운데 새것이 칸에 남는지 정합니다. 몬스터의 효과와 플레이어의 버프가 이 규칙 하나를 씁니다.
/// 센 쪽이 남고, 세기가 같으면 남은 시간이 긴 쪽이 남고, 그것도 같으면 새것이 남습니다.
/// `new_is`는 새것의 세기를 걸려 있는 것의 세기와 견준 값이고 `Greater`이면 새것이 셉니다. 세기를 재는 방법은 효과마다 다르므로 부르는 쪽이 정합니다.
fn new_one_stays(new_is: Ordering, new_ticks: u32, held_ticks: u32) -> bool {
    new_is.then(new_ticks.cmp(&held_ticks)) != Ordering::Less
}

/// 시간이 지나면 끝나는 배율 하나입니다. 몬스터의 감속과 플레이어의 두 버프(공격 속도, 이동 속도)가 같은 값을 씁니다.
/// 같은 칸에 배율은 하나이므로 세기는 겹치지 않습니다. 다시 받았을 때 어느 쪽이 남는지는 `new_one_stays`가 정합니다.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Scaling {
    factor: f32,
    /// 앞으로 배율이 적용될 Tick입니다. 존재하는 동안에는 항상 1 이상이고, 0이 되면 칸이 비워집니다.
    ticks_left: u32,
}

impl Scaling {
    fn new(factor: f32, secs: f32, t: &Tuning) -> Scaling {
        Scaling {
            factor,
            ticks_left: t.ticks(secs),
        }
    }

    /// 배율을 한 Tick 진행합니다. 시간이 다 되면 칸을 비웁니다.
    fn advance(slot: &mut Option<Scaling>) {
        if let Some(scaling) = slot {
            scaling.ticks_left = scaling.ticks_left.saturating_sub(1);
            if scaling.ticks_left == 0 {
                *slot = None;
            }
        }
    }

    fn factor_of(slot: &Option<Scaling>) -> f32 {
        slot.map_or(1.0, |scaling| scaling.factor)
    }
}

/// 살아 있는 플레이어가 받은 버프 전부입니다. 다른 버프는 함께 걸리고, 같은 버프를 다시 받으면 남은 시간이 긴 쪽이 남습니다. 세기는 겹치지 않습니다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerBuffs {
    haste: Option<Scaling>,
    swift: Option<Scaling>,
}

impl PlayerBuffs {
    pub fn has_haste(&self) -> bool {
        self.haste.is_some()
    }

    pub fn has_swift(&self) -> bool {
        self.swift.is_some()
    }

    /// 공격의 쿨다운에 곱하는 값입니다. 공격 속도 버프가 없으면 1입니다.
    pub(crate) fn cooldown_factor(&self) -> f32 {
        Scaling::factor_of(&self.haste)
    }

    /// 이동 속도에 곱하는 값입니다. 달리기의 배율과 곱해집니다. 이동 속도 버프가 없으면 1입니다.
    pub(crate) fn speed_factor(&self) -> f32 {
        Scaling::factor_of(&self.swift)
    }

    /// 버프를 받습니다. 버프를 거는 길은 이 함수 하나입니다. 세기는 `Tuning::effects`의 값이고, 길이는 거는 쪽이 정합니다.
    /// 이미 같은 버프가 걸려 있으면 세기가 같으므로 남은 시간이 긴 쪽이 남고, 남은 시간이 같으면 새 버프가 남습니다.
    /// 길이가 한 Tick도 되지 않는 버프(0초 이하이거나 숫자가 아닌 길이)는 받지 않고, 걸려 있던 버프도 그대로 둡니다.
    pub(crate) fn grant(&mut self, buff: Buff, t: &Tuning) {
        let (slot, factor, secs) = match buff {
            Buff::Haste { secs } => (&mut self.haste, t.effects.haste_cooldown_factor, secs),
            Buff::Swift { secs } => (&mut self.swift, t.effects.swift_speed_factor, secs),
        };
        let ticks = t.ticks(secs);
        let stays = slot.is_none_or(|held| new_one_stays(Ordering::Equal, ticks, held.ticks_left));
        if ticks > 0 && stays {
            *slot = Some(Scaling::new(factor, secs, t));
        }
    }

    /// 모든 버프를 한 Tick 진행합니다.
    fn advance(&mut self) {
        Scaling::advance(&mut self.haste);
        Scaling::advance(&mut self.swift);
    }
}

/// 몬스터의 출혈입니다. 걸려 있는 동안 정해진 간격마다 피해를 줍니다.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bleed {
    /// 칸에 남은 출혈을 건 쪽입니다. 출혈의 피해로 몬스터가 죽으면 이 쪽이 처치를 가져갑니다.
    by: Source,
    damage: u32,
    ticks_left: u32,
    /// 다음 피해까지 남은 Tick입니다. 칸에 남는 출혈이 바뀌어도 되돌리지 않으므로 피해는 계속 같은 간격으로 납니다.
    ticks_to_pulse: u32,
}

/// 스턴은 한 방향으로만 바뀝니다. 스턴이 걸린 몬스터에게 스턴을 또 거는 일도, 면역인 몬스터에게 거는 일도 `apply`가 무시합니다.
///
/// ```text
///   Ready --(스턴을 받음)--> Stunned --(시간이 다 됨)--> Immune --(시간이 다 됨)--> Ready
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Stun {
    /// 스턴에 걸리지 않았고, 걸릴 수 있습니다.
    #[default]
    Ready,
    Stunned {
        ticks_left: u32,
    },
    /// 스턴이 끝났습니다. 시간이 다 될 때까지 스턴에 걸리지 않습니다. 화면에 보이지 않습니다.
    Immune {
        ticks_left: u32,
    },
}

impl Stun {
    fn advance(&mut self, immunity_ticks: u32) {
        *self = match *self {
            Stun::Ready => Stun::Ready,
            Stun::Stunned { ticks_left } if ticks_left > 1 => Stun::Stunned {
                ticks_left: ticks_left - 1,
            },
            Stun::Stunned { .. } => Stun::Immune {
                ticks_left: immunity_ticks,
            },
            Stun::Immune { ticks_left } if ticks_left > 1 => Stun::Immune {
                ticks_left: ticks_left - 1,
            },
            Stun::Immune { .. } => Stun::Ready,
        };
    }
}

/// 한 몬스터에게 걸린 효과 전부입니다. 서로 다른 효과는 함께 걸리고, 같은 효과를 다시 받으면 센 쪽이 남습니다. 세기는 겹치지 않습니다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonsterEffects {
    slow: Option<Scaling>,
    bleed: Option<Bleed>,
    stun: Stun,
}

/// 출혈이 이번 Tick에 낸 피해입니다.
pub(crate) struct BleedPulse {
    pub by: Source,
    pub damage: u32,
}

impl MonsterEffects {
    pub fn is_slowed(&self) -> bool {
        self.slow.is_some()
    }

    pub fn is_bleeding(&self) -> bool {
        self.bleed.is_some()
    }

    /// 스턴이 걸려 있는 동안만 참입니다. 스턴이 끝난 뒤의 면역은 참이 아닙니다.
    pub fn is_stunned(&self) -> bool {
        matches!(self.stun, Stun::Stunned { .. })
    }

    /// 몬스터가 스스로 걷는 속도에 곱하는 값입니다. 감속이 없으면 1입니다.
    pub(crate) fn walk_factor(&self) -> f32 {
        Scaling::factor_of(&self.slow)
    }

    /// `debuff`를 지금 받으면 칸이 바뀌는지입니다. `apply`가 이 질문으로 시작하므로, 거짓이면 `apply`는 아무것도 바꾸지 않습니다.
    /// 감속과 출혈은 `new_one_stays`를 따르고, 스턴은 걸려 있지 않고 면역도 아닐 때만 걸립니다.
    /// 효과 하나를 거는 포탑이 대상을 고를 때 이 질문으로 걸리지 않을 몬스터를 건너뜁니다.
    pub(crate) fn would_take(&self, debuff: Debuff, t: &Tuning) -> bool {
        match debuff {
            // 비율이 작을수록 센 감속입니다.
            Debuff::Slow { factor, secs } => self.slow.is_none_or(|held| {
                new_one_stays(
                    held.factor.total_cmp(&factor),
                    t.ticks(secs),
                    held.ticks_left,
                )
            }),
            // 한 번의 피해가 클수록 센 출혈입니다.
            Debuff::Bleed { damage, secs } => self.bleed.is_none_or(|held| {
                new_one_stays(damage.cmp(&held.damage), t.ticks(secs), held.ticks_left)
            }),
            Debuff::Stun { .. } => self.stun == Stun::Ready,
        }
    }

    /// 효과를 겁니다. 효과를 거는 길은 이 함수 하나이고, 걸리는지는 `would_take`가 정합니다.
    ///
    /// - 감속: 비율과 시간이 새 값으로 바뀝니다.
    /// - 출혈: 주인, 피해량, 남은 시간이 새 값으로 바뀝니다. 피해가 나는 시점은 이어집니다. 처음 걸리면 `bleed_interval_secs` 뒤에 첫 피해가 납니다.
    /// - 스턴: 시간만큼 걸립니다.
    pub(crate) fn apply(&mut self, debuff: Debuff, by: Source, t: &Tuning) {
        if !self.would_take(debuff, t) {
            return;
        }
        match debuff {
            Debuff::Slow { factor, secs } => self.slow = Some(Scaling::new(factor, secs, t)),
            Debuff::Bleed { damage, secs } => {
                let ticks_to_pulse = self.bleed.map_or_else(
                    || t.ticks(t.effects.bleed_interval_secs),
                    |bleed| bleed.ticks_to_pulse,
                );
                self.bleed = Some(Bleed {
                    by,
                    damage,
                    ticks_left: t.ticks(secs),
                    ticks_to_pulse,
                });
            }
            Debuff::Stun { secs } => {
                self.stun = Stun::Stunned {
                    ticks_left: t.ticks(secs),
                };
            }
        }
    }

    /// 모든 효과를 한 Tick 진행합니다. 이번 Tick에 출혈이 피해를 내면 그 피해를 돌려줍니다.
    /// 피해가 나는 Tick에 출혈이 끝나도 그 피해는 납니다.
    pub(crate) fn advance(&mut self, t: &Tuning) -> Option<BleedPulse> {
        Scaling::advance(&mut self.slow);
        self.stun.advance(t.ticks(t.effects.stun_immunity_secs));
        // 출혈이 없으면 여기서 `None`을 돌려줍니다.
        let bleed = self.bleed.as_mut()?;
        let mut pulse = None;
        bleed.ticks_to_pulse = bleed.ticks_to_pulse.saturating_sub(1);
        if bleed.ticks_to_pulse == 0 {
            pulse = Some(BleedPulse {
                by: bleed.by,
                damage: bleed.damage,
            });
            bleed.ticks_to_pulse = t.ticks(t.effects.bleed_interval_secs);
        }
        bleed.ticks_left = bleed.ticks_left.saturating_sub(1);
        if bleed.ticks_left == 0 {
            self.bleed = None;
        }
        pulse
    }
}

/// 모든 몬스터의 효과와 모든 플레이어의 버프를 한 Tick 진행하고, 이번 Tick에 나는 출혈의 피해를 `hits`의 끝에 더합니다.
/// 자리를 비운 플레이어의 버프도 같이 흐르고, 야영지의 Tick도 이 함수를 부릅니다.
/// 출혈의 피해도 `Hit`이므로 `World::apply_hits`를 지나고, 그 피해로 몬스터가 죽으면 처치는 출혈을 건 쪽의 것입니다.
/// 몬스터와 플레이어가 행동한 뒤에 부르므로, 이번 Tick에 끝나는 효과는 마지막으로 행동을 바꾼 뒤에 풀립니다.
pub(crate) fn advance_effects(world: &mut World, ctx: &Ctx, hits: &mut Vec<Hit>) {
    for monster in &mut world.monsters {
        if let Some(pulse) = monster.effects.advance(ctx.t) {
            hits.push(Hit {
                target: Target::Monster(monster.id),
                amount: pulse.damage,
                by: pulse.by,
                debuff: None,
            });
        }
    }
    for player in world.players.values_mut() {
        if let Life::Alive { buffs, .. } = &mut player.life {
            buffs.advance();
        }
    }
}

/// 이번 Tick의 `hits`가 싣고 온 효과를 맞은 몬스터에게 겁니다. 야영지의 Tick도 부르므로 피해는 적용하지 않습니다.
/// 같은 Tick에 죽어서 이미 제거된 몬스터는 목록에 없으므로 효과가 걸리지 않습니다. 그래서 `World::apply_hits`가 몬스터를 제거한 뒤에 불러야 합니다.
pub(crate) fn apply_debuffs(world: &mut World, hits: &[Hit], t: &Tuning) {
    for hit in hits {
        let (Target::Monster(id), Some(debuff)) = (hit.target, hit.debuff) else {
            continue;
        };
        if let Ok(index) = world
            .monsters
            .binary_search_by_key(&id, |monster| monster.id)
        {
            world.monsters[index].effects.apply(debuff, hit.by, t);
        }
    }
}
