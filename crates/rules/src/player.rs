//! 플레이어입니다. 생명 상태(살아 있음, 쓰러짐, 사망), 이동, 근접 공격, 상호작용(부활과 수리)을 다룹니다.

use crate::collision::move_circle;
use crate::ids::{PlayerId, Team, TowerId};
use crate::math::Vec2;
use crate::tuning::Tuning;
use crate::world::{Ctx, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub team: Team,
    pub pos: Vec2,
    /// 마지막으로 조준한 방향입니다. 근접 공격의 부채꼴이 이 방향을 향합니다.
    pub facing: Vec2,
    pub life: Life,
    /// 마지막으로 처리한 입력의 순서 번호입니다.
    pub last_input_seq: u32,
}

/// 플레이어의 생명 상태입니다. 상태마다 그 상태에서 의미가 있는 값만 가집니다.
/// 그래서 다음 상태는 코드로 표현할 수 없습니다.
/// - 쓰러졌거나 사망한 플레이어가 HP, 쿨다운, 상호작용을 가지는 상태 (공격, 부활, 수리를 할 수 없습니다: C18, D37).
/// - 사망한 플레이어가 출혈 시간을 가지는 상태.
///
/// ```text
///   Alive --(HP가 0이 됨)--> Downed --(출혈 시간이 0이 됨)--> Dead
///     ^                        |
///     +----(부활이 끝남)-------+
///
///   새 원정이 시작되면 모든 플레이어가 최대 HP의 Alive가 됩니다 (D34).
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum Life {
    Alive {
        /// 항상 1 이상입니다. HP가 0이 되면 `Alive`를 `Downed`로 바꿉니다.
        hp: u32,
        /// 다음 근접 공격까지 남은 Tick입니다.
        cooldown_ticks: u32,
        /// 상호작용 키(F)가 지금 하고 있는 일입니다. 값이 있으면 이동과 공격을 할 수 없습니다 (C18, D38, D48).
        interaction: Option<Interaction>,
    },
    Downed {
        /// 남은 출혈 시간입니다. 아무도 부활시키지 않는 동안에만 줄어듭니다 (D22).
        bleed_left_ticks: u32,
    },
    /// 이 원정이 끝날 때까지 부활할 수 없습니다 (C9, D39).
    Dead,
}

/// 상호작용 키(F)를 누르고 있는 동안의 행동입니다. 그 행동을 하는 플레이어가 가지고 있습니다.
///
/// 부활의 진행도는 부활시키는 플레이어의 `Interaction`에만 있습니다.
/// 「플레이어 X가 부활되고 있는가」는 `Reviving { target: X }`를 가진 플레이어를 찾아서 알아냅니다.
/// 쓰러진 플레이어에게는 아무것도 저장하지 않으므로, 두 곳의 값이 서로 어긋날 수 없습니다.
#[derive(Clone, Debug, PartialEq)]
pub enum Interaction {
    Reviving {
        target: PlayerId,
        /// 키를 끊지 않고 누른 Tick 수입니다. 누르기 시작한 Tick을 포함합니다.
        /// 취소하면 이 값을 통째로 버리므로 진행도가 처음으로 돌아갑니다 (D22).
        elapsed_ticks: u32,
    },
    Repairing {
        tower: TowerId,
        /// 마지막 수리 이후의 Tick 수입니다. 수리를 멈춰도 이미 회복한 HP는 유지됩니다 (D48).
        since_pulse_ticks: u32,
    },
}

impl Player {
    /// 최대 HP의 살아 있는 플레이어를 `pos`에 만듭니다.
    pub(crate) fn new(id: PlayerId, team: Team, pos: Vec2, t: &Tuning) -> Player {
        Player {
            id,
            team,
            pos,
            facing: Vec2::new(1.0, 0.0),
            life: Life::Alive {
                hp: t.player.max_hp,
                cooldown_ticks: 0,
                interaction: None,
            },
            last_input_seq: 0,
        }
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.life, Life::Alive { .. })
    }

    pub fn is_downed(&self) -> bool {
        matches!(self.life, Life::Downed { .. })
    }

    /// 쓰러졌거나 사망한 플레이어의 HP는 0입니다.
    pub fn hp(&self) -> u32 {
        match self.life {
            Life::Alive { hp, .. } => hp,
            Life::Downed { .. } | Life::Dead => 0,
        }
    }

    pub fn interaction(&self) -> Option<&Interaction> {
        match &self.life {
            Life::Alive { interaction, .. } => interaction.as_ref(),
            Life::Downed { .. } | Life::Dead => None,
        }
    }

    /// 지금 이동할 수 있는 속도입니다. 이동 속도를 읽는 곳은 이 함수 하나입니다.
    /// 상태 효과를 추가하면 여기에서 배율을 곱합니다.
    fn move_speed(&self, t: &Tuning) -> f32 {
        match &self.life {
            Life::Alive {
                interaction: None, ..
            } => t.player.speed,
            Life::Downed { .. } => t.player.crawl_speed,
            // 부활이나 수리를 하는 동안과 사망한 뒤에는 움직이지 못합니다.
            Life::Alive {
                interaction: Some(_),
                ..
            }
            | Life::Dead => 0.0,
        }
    }
}

/// 입력대로 플레이어를 움직입니다. 살아 있는 플레이어는 걷고, 쓰러진 플레이어는 기어갑니다 (D37).
pub(crate) fn move_players(world: &mut World, ctx: &Ctx) {
    let t = ctx.t;
    for player in world.players.values_mut() {
        let Some(input) = ctx.inputs.get(&player.id) else {
            continue;
        };
        player.last_input_seq = input.seq;
        if let Some(aim) = input.aim.get() {
            player.facing = aim;
        }
        let speed = player.move_speed(t);
        if speed > 0.0 {
            let delta = input.move_dir.get() * (speed * t.dt());
            player.pos = move_circle(
                player.pos,
                t.player.radius,
                delta,
                &ctx.statics.solids,
                ctx.statics.bounds,
            );
        }
    }
}
