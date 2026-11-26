//! 날아가는 탄환입니다. 탄환은 쏜 플레이어와 관계없이 세계(`World::projectiles`)에 있는 개체이고, 번호 순서로 하나씩 날아갑니다.
//!
//! 한 Tick에 탄환은 `속도 * Tick 길이`만큼의 선분을 움직입니다. 그 선분 전체를 검사해서(`sweep`) 막힌 타일과 몬스터를 닿는 순서대로 봅니다.
//! 몬스터에 닿을 때마다 피해(`Hit`)를 하나 만들고, 맞힌 몬스터가 `pierce`마리에 이르면 그 몬스터에서 사라집니다.
//! 맞힌 몬스터는 기억하므로(`struck`) 몬스터 안을 지나가는 몇 Tick 동안에도 같은 몬스터를 두 번 맞히지 않습니다. 몬스터마다 같은 피해를 줍니다.
//! 막힌 곳이 몬스터보다 가까우면 그 뒤의 몬스터는 맞히지 못하고 막힌 곳에서 사라집니다. 사거리를 다 날아도 사라집니다.
//! 플레이어와 탄환은 서로 지나갑니다. 탄환이 피해를 주는 대상은 탄환의 진영과 적대적인 몬스터뿐입니다.
//!
//! 끝난 탄환(`Flight::Landed`)은 끝난 자리에 한 Tick 더 남아서 스냅샷에 한 번 실립니다. 그래서 클라이언트가 탄환을 날아간 끝 자리까지 그립니다.
//! 남은 탄환은 다음 Tick의 처음에 지웁니다. 세계가 멈춘 결과 화면의 Tick에도 지웁니다.

use crate::ids::{MonsterId, PlayerId, ProjectileId, Team};
use crate::math::Vec2;
use crate::sweep;
use crate::tuning::ProjectileTuning;
use crate::world::{Ctx, Hit, Source, Target, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Projectile {
    pub id: ProjectileId,
    /// 쏜 플레이어입니다. 탄환이 몬스터를 쓰러뜨리면 그 처치를 이 플레이어의 것으로 셉니다. 플레이어가 나가도 탄환은 끝까지 날아가고, 이때의 처치는 셀 사람이 없습니다.
    pub(crate) shooter: PlayerId,
    /// 쏜 플레이어의 진영입니다. 이 진영과 적대적인 몬스터만 맞힙니다.
    pub team: Team,
    /// 날아가는 중이면 이번 Tick에 움직인 뒤의 위치이고, 끝났으면 끝난 자리(맞힌 자리, 막힌 자리, 사거리의 끝)입니다.
    pub pos: Vec2,
    /// 길이가 1인 날아가는 방향입니다. 날아가는 동안 바뀌지 않습니다.
    pub dir: Vec2,
    pub flight: Flight,
    /// 쏠 때 무기의 행에서 가져온 수치입니다. 탄환이 쏜 뒤에 수치표를 다시 읽지 않으므로 날아가는 동안 값이 바뀌지 않습니다.
    pub(crate) spec: ProjectileTuning,
    /// 쏠 때 플레이어의 `stats`가 정한 피해량입니다.
    pub(crate) damage: u32,
    /// 지금까지 맞힌 몬스터입니다. 같은 몬스터를 두 번 맞히지 않게 하고, 맞힌 수가 `spec.pierce`에 이르면 탄환이 끝납니다.
    pub(crate) struck: Vec<MonsterId>,
}

/// 탄환의 비행 상태입니다. 날아가는 탄환만 남은 사거리를 가지고, 끝난 탄환은 아무 값도 가지지 않습니다.
///
/// ```text
///   쏨 --> Flying --(몬스터에 닿음, 막힌 곳에 닿음, 사거리를 다 날았음)--> Landed --(다음 Tick의 처음)--> 지워짐
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flight {
    Flying {
        /// 앞으로 더 날아갈 수 있는 거리(타일)입니다. 항상 0보다 큽니다.
        range_left: f32,
    },
    Landed,
}

/// 이번 Tick에 결정된 발사입니다. `World::launch`가 번호를 붙여서 탄환으로 만듭니다.
/// `Hit`이 「피해를 입힌다는 결정」이듯이, 이 구조체는 「탄환을 만든다는 결정」입니다.
pub(crate) struct Launch {
    pub shooter: PlayerId,
    pub team: Team,
    /// 쏘는 플레이어의 중심입니다.
    pub from: Vec2,
    /// 길이가 1인 조준 방향입니다.
    pub dir: Vec2,
    pub damage: u32,
    pub spec: ProjectileTuning,
}

impl Projectile {
    /// 날아가는 상태의 새 탄환입니다. 처음 위치는 쏘는 플레이어의 중심입니다.
    pub(crate) fn new(id: ProjectileId, launch: Launch) -> Projectile {
        Projectile {
            id,
            shooter: launch.shooter,
            team: launch.team,
            pos: launch.from,
            dir: launch.dir,
            flight: Flight::Flying {
                range_left: launch.spec.range,
            },
            spec: launch.spec,
            damage: launch.damage,
            struck: Vec::new(),
        }
    }

    pub fn is_flying(&self) -> bool {
        matches!(self.flight, Flight::Flying { .. })
    }

    /// `pos`에서 끝납니다.
    fn land_at(&mut self, pos: Vec2) {
        self.pos = pos;
        self.flight = Flight::Landed;
    }
}

/// 지난 Tick에 끝난 탄환을 지웁니다. 끝난 탄환은 스냅샷에 한 번 실렸습니다.
pub(crate) fn remove_landed(world: &mut World) {
    world.projectiles.retain(Projectile::is_flying);
}

/// 날아가는 탄환을 모두 한 Tick만큼 움직입니다. 이번 Tick에 쏜 탄환도 같은 Tick에 처음 움직입니다.
///
/// 선분이 닿는 몬스터를 닿는 순서대로 맞힙니다. 막힌 타일이 몬스터보다 가까우면 그 뒤의 몬스터는 맞히지 못하고,
/// 맞힌 수가 `pierce`에 이르면 그 몬스터에서 끝납니다. 같은 거리에 닿으면 몬스터가 먼저입니다. 벽에 붙어 선 몬스터를 맞힐 수 있어야 하기 때문입니다.
///
/// 몬스터는 이번 Tick에 아직 움직이지 않았으므로, 몬스터의 격자는 탄환이 닿는 위치와 같습니다.
/// 같은 Tick에 부채꼴 공격이나 감시탑이 쓰러뜨릴 몬스터도 아직 피해를 적용하기 전이라서 탄환이 맞힐 수 있고, 두 피해가 모두 적용됩니다.
pub(crate) fn advance_projectiles(world: &mut World, ctx: &Ctx, hits: &mut Vec<Hit>) {
    remove_landed(world);

    let mut solids = Vec::new();
    let mut strikes = sweep::Strikes::default();
    for projectile in &mut world.projectiles {
        let Flight::Flying { range_left } = projectile.flight else {
            continue;
        };
        let travel = (projectile.spec.speed * ctx.t.dt()).min(range_left);
        let from = projectile.pos;
        let delta = projectile.dir * travel;
        let wall = sweep::first_blocked(
            &world.tiles,
            from,
            delta,
            projectile.spec.radius,
            &mut solids,
        );
        sweep::monsters_along(
            projectile.team,
            projectile.spec.radius,
            from,
            delta,
            &world.monsters,
            ctx,
            &mut strikes,
        );

        let mut ended_at = None;
        for &(at, id) in &strikes.found {
            if wall.is_some_and(|wall_at| wall_at < at) {
                break;
            }
            if projectile.struck.contains(&id) {
                continue;
            }
            hits.push(Hit {
                target: Target::Monster(id),
                amount: projectile.damage,
                by: Source::Player(projectile.shooter),
            });
            projectile.struck.push(id);
            if projectile.struck.len() >= projectile.spec.pierce as usize {
                ended_at = Some(at);
                break;
            }
        }
        match ended_at.or(wall) {
            Some(at) => projectile.land_at(from + delta * at),
            None => {
                projectile.pos = from + delta;
                projectile.flight = if range_left > travel {
                    Flight::Flying {
                        range_left: range_left - travel,
                    }
                } else {
                    Flight::Landed
                };
            }
        }
    }
}
