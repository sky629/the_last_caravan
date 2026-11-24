//! 날아가는 탄환입니다. 탄환은 쏜 플레이어와 관계없이 세계(`World::projectiles`)에 있는 개체이고, 번호 순서로 하나씩 날아갑니다.
//!
//! 한 Tick에 탄환은 `속도 * Tick 길이`만큼의 선분을 움직입니다. 그 선분 전체를 검사해서(`sweep`) 막힌 타일과 몬스터 가운데
//! 먼저 닿는 쪽에서 멈춥니다. 몬스터에 닿으면 피해(`Hit`)를 하나 만들고 사라지며, 막힌 곳에 닿거나 사거리를 다 날면 피해 없이 사라집니다.
//! 플레이어와 탄환은 서로 지나갑니다. 탄환이 피해를 주는 대상은 탄환의 진영과 적대적인 몬스터뿐입니다.
//!
//! 끝난 탄환(`Flight::Landed`)은 끝난 자리에 한 Tick 더 남아서 스냅샷에 한 번 실립니다. 그래서 클라이언트가 탄환을 날아간 끝 자리까지 그립니다.
//! 남은 탄환은 다음 Tick의 처음에 지웁니다. 세계가 멈춘 결과 화면의 Tick에도 지웁니다.

use crate::ids::{MonsterId, ProjectileId, Team};
use crate::math::Vec2;
use crate::monster::Monster;
use crate::sweep;
use crate::tuning::ProjectileTuning;
use crate::world::{Ctx, Hit, Target, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Projectile {
    pub id: ProjectileId,
    /// 쏜 플레이어의 진영입니다. 이 진영과 적대적인 몬스터만 맞힙니다. 쏜 플레이어의 번호는 저장하지 않으므로, 플레이어가 나가도 탄환은 끝까지 날아갑니다.
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
            team: launch.team,
            pos: launch.from,
            dir: launch.dir,
            flight: Flight::Flying {
                range_left: launch.spec.range,
            },
            spec: launch.spec,
            damage: launch.damage,
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
/// 몬스터는 이번 Tick에 아직 움직이지 않았으므로, 몬스터의 격자는 탄환이 닿는 위치와 같습니다.
/// 같은 Tick에 부채꼴 공격이나 감시탑이 쓰러뜨릴 몬스터도 아직 피해를 적용하기 전이라서 탄환이 맞힐 수 있고, 두 피해가 모두 적용됩니다.
pub(crate) fn advance_projectiles(world: &mut World, ctx: &Ctx, hits: &mut Vec<Hit>) {
    remove_landed(world);

    let mut solids = Vec::new();
    let mut nearby = Vec::new();
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
        let strike = first_monster(projectile, from, delta, &world.monsters, ctx, &mut nearby);
        match first_contact(strike, wall) {
            Contact::Monster { at, id } => {
                hits.push(Hit {
                    target: Target::Monster(id),
                    amount: projectile.damage,
                });
                projectile.land_at(from + delta * at);
            }
            Contact::Wall { at } => projectile.land_at(from + delta * at),
            Contact::Nothing => {
                projectile.pos = from + delta;
                if range_left > travel {
                    projectile.flight = Flight::Flying {
                        range_left: range_left - travel,
                    };
                } else {
                    projectile.flight = Flight::Landed;
                }
            }
        }
    }
}

/// 한 Tick의 선분이 처음 닿은 것입니다.
enum Contact {
    Monster { at: f32, id: MonsterId },
    Wall { at: f32 },
    Nothing,
}

/// 같은 거리에 닿으면 몬스터가 먼저입니다. 벽에 붙어 선 몬스터를 맞힐 수 있어야 하기 때문입니다.
fn first_contact(strike: Option<(f32, MonsterId)>, wall: Option<f32>) -> Contact {
    match (strike, wall) {
        (Some((at, _)), Some(wall_at)) if wall_at < at => Contact::Wall { at: wall_at },
        (Some((at, id)), _) => Contact::Monster { at, id },
        (None, Some(at)) => Contact::Wall { at },
        (None, None) => Contact::Nothing,
    }
}

/// 이동하는 선분이 처음 닿는 적대적인 몬스터와 닿는 `t`입니다. 같은 `t`이면 번호가 작은 몬스터입니다.
/// 탄환의 중심이 몬스터의 반지름에 탄환의 반지름을 더한 거리 안으로 지나가면 닿은 것입니다.
fn first_monster(
    projectile: &Projectile,
    from: Vec2,
    delta: Vec2,
    monsters: &[Monster],
    ctx: &Ctx,
    nearby: &mut Vec<usize>,
) -> Option<(f32, MonsterId)> {
    let t = ctx.t;
    // 선분 위의 모든 점은 중점에서 길이의 절반 안에 있으므로, 그 둘레의 몬스터만 후보입니다.
    let middle = from + delta * 0.5;
    let search_radius = delta.length() * 0.5 + projectile.spec.radius + t.monsters.largest_radius();
    ctx.grid.query(middle, search_radius, nearby);

    let mut first: Option<(f32, MonsterId)> = None;
    for &index in nearby.iter() {
        let monster = &monsters[index];
        if !projectile.team.is_hostile_to(monster.team) {
            continue;
        }
        let reach = t.monsters.get(monster.kind).radius + projectile.spec.radius;
        let Some(at) = sweep::circle_entry(from, delta, monster.pos, reach) else {
            continue;
        };
        let is_first = first.is_none_or(|(best_at, best_id)| {
            at < best_at || (at == best_at && monster.id < best_id)
        });
        if is_first {
            first = Some((at, monster.id));
        }
    }
    first
}
