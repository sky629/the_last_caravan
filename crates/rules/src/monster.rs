//! 몬스터입니다. 목표를 고르고, 흐름장을 따라 걸어가고, 가까이 가면 공격합니다.

use std::collections::{BTreeMap, BTreeSet};

use crate::collision::move_circle;
use crate::ids::{MonsterId, PlayerId, Team};
use crate::kind::MonsterKind;
use crate::math::Vec2;
use crate::nav::{Cell, FlowField, NavGrid};
use crate::stats::UnitStats;
use crate::tuning::Tuning;
use crate::world::{Ctx, Hit, Target, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Monster {
    pub id: MonsterId,
    pub kind: MonsterKind,
    pub team: Team,
    pub pos: Vec2,
    pub facing: Vec2,
    /// 존재하는 동안에는 항상 1 이상입니다. 0이 되면 `World::apply_hits`가 제거합니다.
    pub hp: u32,
    /// 다음 공격까지 남은 Tick입니다.
    pub cooldown_ticks: u32,
    /// 마지막으로 공격한 Tick입니다. 이벤트가 아니라 상태이므로, 스냅샷을 한 번 놓친 클라이언트도 공격 효과를 그릴 수 있습니다.
    pub last_attack_tick: Option<u64>,
}

impl Monster {
    pub(crate) fn new(id: MonsterId, kind: MonsterKind, pos: Vec2, t: &Tuning) -> Monster {
        Monster {
            id,
            kind,
            team: Team::HORDE,
            pos,
            facing: Vec2::new(0.0, 1.0),
            hp: t.monsters.get(kind).max_hp,
            cooldown_ticks: 0,
            last_attack_tick: None,
        }
    }

    /// 이동 속도, 공격 피해량, 쿨다운을 읽는 유일한 곳입니다. 상태 효과를 추가하면 여기에서 배율을 곱합니다.
    pub(crate) fn stats(&self, t: &Tuning) -> UnitStats {
        let kind = t.monsters.get(self.kind);
        UnitStats {
            move_speed: kind.speed,
            attack_damage: kind.attack_damage,
            attack_cooldown_ticks: t.ticks(kind.attack_cooldown_secs),
        }
    }
}

/// 추적당하는 플레이어마다 만든 흐름장입니다. 플레이어는 움직이므로 흐름장이 필요합니다.
/// 플레이어가 다른 칸으로 옮겨 갔을 때만 다시 만들고, 추적이 끝난 플레이어의 흐름장은 버립니다.
pub(crate) struct PlayerFields {
    entries: BTreeMap<PlayerId, (Cell, FlowField)>,
}

impl PlayerFields {
    pub fn new() -> PlayerFields {
        PlayerFields {
            entries: BTreeMap::new(),
        }
    }

    fn refresh(&mut self, goals: &[Target], world: &World, nav: &NavGrid) {
        let chased: BTreeSet<PlayerId> = goals
            .iter()
            .filter_map(|goal| match goal {
                Target::Player(id) => Some(*id),
                _ => None,
            })
            .collect();
        self.entries.retain(|id, _| chased.contains(id));
        for id in chased {
            let Some(player) = world.players.get(&id) else {
                continue;
            };
            let Some(cell) = nav.nearest_free(player.pos) else {
                continue;
            };
            let up_to_date = matches!(self.entries.get(&id), Some((known, _)) if *known == cell);
            if !up_to_date {
                self.entries
                    .insert(id, (cell, FlowField::toward(nav, &[cell])));
            }
        }
    }

    fn get(&self, id: PlayerId) -> Option<&FlowField> {
        self.entries.get(&id).map(|(_, field)| field)
    }
}

/// 한 몬스터가 이번 Tick에 하기로 결정한 일입니다.
struct Action {
    goal: Target,
    /// 몸을 돌릴 방향입니다. 길이가 1이거나 0입니다.
    heading: Vec2,
    /// 이번 Tick에 옮길 거리입니다. 걸음과 다른 몬스터에게서 밀려나는 거리를 합친 값입니다.
    step: Vec2,
    /// 목표가 공격 거리 안에 있습니다.
    in_reach: bool,
}

/// 모든 몬스터를 한 Tick 진행합니다. 먼저 모든 몬스터의 행동을 읽기만 하며 결정하고, 그 뒤에 적용합니다.
/// 그래서 몬스터를 어떤 순서로 처리해도 결과가 같습니다.
pub(crate) fn monsters_act(
    world: &mut World,
    ctx: &Ctx,
    fields: &mut PlayerFields,
    hits: &mut Vec<Hit>,
) {
    let t = ctx.t;
    let goals: Vec<Target> = world
        .monsters
        .iter()
        .map(|monster| choose_goal(world, monster, t))
        .collect();
    fields.refresh(&goals, world, &ctx.statics.nav);

    let mut neighbours = Vec::new();
    let mut actions = Vec::with_capacity(goals.len());
    for (index, goal) in goals.iter().enumerate() {
        actions.push(plan(index, *goal, world, ctx, fields, &mut neighbours));
    }

    for (monster, action) in world.monsters.iter_mut().zip(&actions) {
        if action.heading != Vec2::ZERO {
            monster.facing = action.heading;
        }
        if action.step != Vec2::ZERO {
            monster.pos = move_circle(
                monster.pos,
                t.monsters.get(monster.kind).radius,
                action.step,
                &ctx.statics.solids,
                ctx.statics.bounds,
            );
        }
        monster.cooldown_ticks = monster.cooldown_ticks.saturating_sub(1);
        if action.in_reach && monster.cooldown_ticks == 0 {
            let stats = monster.stats(t);
            hits.push(Hit {
                target: action.goal,
                amount: stats.attack_damage,
            });
            monster.cooldown_ticks = stats.attack_cooldown_ticks;
            monster.last_attack_tick = Some(ctx.tick);
        }
    }
}

/// 몬스터의 목표를 고릅니다. 몬스터는 목표를 기억하지 않고 Tick마다 새로 고릅니다.
/// 이 한 함수가 「쫓다가 범위를 벗어나면 다시 마차로」(D17), 「쓰러지거나 사망한 플레이어는 대상이 아님」(D33)을 정합니다.
///
/// 후보는 추적 거리(`aggro_range`) 안에 있고 몬스터와 적대하는 살아 있는 플레이어입니다.
/// 후보 가운데 가장 가까운 대상을 고르고, 후보가 없으면 마차를 고릅니다.
fn choose_goal(world: &World, monster: &Monster, t: &Tuning) -> Target {
    let aggro_range = t.monsters.get(monster.kind).aggro_range;
    let mut best: Option<(f32, Target)> = None;
    let mut consider = |target: Target, distance: f32| {
        let closer = best.is_none_or(|(best_distance, _)| distance < best_distance);
        if distance <= aggro_range && closer {
            best = Some((distance, target));
        }
    };
    for player in world.players.values() {
        if player.is_alive() && monster.team.is_hostile_to(player.team) {
            let distance = (monster.pos.distance(player.pos) - t.player.radius).max(0.0);
            consider(Target::Player(player.id), distance);
        }
    }
    match best {
        Some((_, target)) => target,
        None => Target::Wagon,
    }
}

/// 목표를 향해 걷거나, 가까이 있으면 멈춰서 공격하기로 결정합니다.
fn plan(
    index: usize,
    goal: Target,
    world: &World,
    ctx: &Ctx,
    fields: &PlayerFields,
    neighbours: &mut Vec<usize>,
) -> Action {
    let t = ctx.t;
    let monster = &world.monsters[index];
    let kind = t.monsters.get(monster.kind);
    let Some(body) = world.body_of(goal, t) else {
        return Action {
            goal,
            heading: Vec2::ZERO,
            step: Vec2::ZERO,
            in_reach: false,
        };
    };

    let in_reach = body.gap_from(monster.pos, kind.radius) <= kind.attack_reach;
    let (heading, walk) = if in_reach {
        let toward_target = body.closest_point(monster.pos) - monster.pos;
        (toward_target.normalize_or_zero(), Vec2::ZERO)
    } else {
        let field = match goal {
            Target::Wagon => Some(&ctx.statics.wagon_field),
            Target::Player(id) => fields.get(id),
            Target::Monster(_) => None,
        };
        // 흐름장이 알려 주는 다음 칸이 없으면(도착 칸 안이거나 길이 없으면) 목표의 가장 가까운 점으로 곧바로 걸어갑니다.
        let aim = field
            .and_then(|field| field.next_waypoint(&ctx.statics.nav, monster.pos))
            .unwrap_or_else(|| body.closest_point(monster.pos));
        let direction = (aim - monster.pos).normalize_or_zero();
        (
            direction,
            direction * (monster.stats(t).move_speed * t.dt()),
        )
    };
    let push = separation(index, world, ctx, neighbours) * (kind.separation_speed * t.dt());
    Action {
        goal,
        heading,
        step: walk + push,
        in_reach,
    }
}

/// 겹친 몬스터에게서 멀어지는 방향입니다. 겹친 정도가 클수록 길고, 길이는 1을 넘지 않습니다.
/// 이 힘이 몬스터를 부드럽게 밀어내므로, 몬스터가 한 점에 쌓이지 않고 목표 둘레에 고리 모양으로 모입니다.
fn separation(index: usize, world: &World, ctx: &Ctx, neighbours: &mut Vec<usize>) -> Vec2 {
    let t = ctx.t;
    let monster = &world.monsters[index];
    let radius = t.monsters.get(monster.kind).radius;
    ctx.grid.query(
        monster.pos,
        radius + t.monsters.largest_radius(),
        neighbours,
    );

    let mut push = Vec2::ZERO;
    for &other_index in neighbours.iter() {
        if other_index == index {
            continue;
        }
        let other = &world.monsters[other_index];
        let touching = radius + t.monsters.get(other.kind).radius;
        let offset = monster.pos - other.pos;
        let distance = offset.length();
        if distance >= touching {
            continue;
        }
        let away = if distance > 1e-4 {
            offset * (1.0 / distance)
        } else if monster.id < other.id {
            // 두 몬스터가 정확히 같은 자리에 있으면 방향을 알 수 없습니다. 번호로 서로 반대 방향을 정합니다.
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::new(-1.0, 0.0)
        };
        push = push + away * ((touching - distance) / touching);
    }
    if push.length() > 1.0 {
        push.normalize_or_zero()
    } else {
        push
    }
}
