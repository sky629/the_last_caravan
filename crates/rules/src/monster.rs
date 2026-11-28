//! 몬스터입니다. 목표를 고르고, 흐름장을 따라 걸어가고, 가까이 가면 공격합니다.
//!
//! 몬스터에게는 명령(`Order`)이 있습니다. 낮에 채집 지점 곁에 놓인 몬스터는 자리를 지키고(`Guard`),
//! 밤에 나타난 몬스터와 밤이 된 뒤의 낮의 몬스터는 마차를 향합니다(`Raid`). 명령을 바꾸는 곳은 `expedition::nightfall` 하나입니다.
//! 야영지의 허수아비는 제자리에 서 있습니다(`Stand`). 보스는 대상과 걷는지 서 있는지를 보스의 상태(`World::boss`)가 정합니다(`Boss`).

use std::collections::{BTreeMap, BTreeSet};

use crate::boss::Aim;
use crate::collision::{Body, move_circle};
use crate::effect::MonsterEffects;
use crate::ids::{MonsterId, PlayerId, Team};
use crate::kind::MonsterKind;
use crate::math::Vec2;
use crate::nav::{FlowField, Paths};
use crate::stats::UnitStats;
use crate::tuning::{Prey, Tuning};
use crate::world::{Ctx, Hit, Source, Target, Tile, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Monster {
    pub id: MonsterId,
    pub kind: MonsterKind,
    pub team: Team,
    pub order: Order,
    pub pos: Vec2,
    pub facing: Vec2,
    /// 존재하는 동안에는 항상 1 이상입니다. 0이 되면 `World::apply_hits`가 제거합니다.
    pub hp: u32,
    /// 다음 공격까지 남은 Tick입니다.
    pub cooldown_ticks: u32,
    /// 마지막으로 공격한 Tick입니다. 이벤트가 아니라 상태이므로, 스냅샷을 한 번 놓친 클라이언트도 공격 효과를 그릴 수 있습니다.
    pub last_attack_tick: Option<u64>,
    /// 걸려 있는 효과입니다. 몬스터가 제거되면 함께 사라집니다.
    pub effects: MonsterEffects,
}

/// 몬스터가 받은 명령입니다. 종류(`MonsterKind`)가 「어떤 몬스터인가」라면, 명령은 「지금 무엇을 하라고 놓였는가」입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// `post`를 지킵니다. 자리에서 `chase_range` 안에 들어온 플레이어를 쫓고, 아무도 없으면 자리로 돌아갑니다.
    Guard { post: Tile },
    /// 마차를 향합니다. 가는 길에 무엇을 쫓는지는 종류의 수치(`MonsterTuning::prey`)가 정합니다.
    Raid,
    /// 제자리에 서 있습니다. 아무것도 쫓지 않고 공격하지 않습니다. 야영지의 허수아비가 받는 명령입니다.
    Stand,
    /// 보스의 몸이 받는 명령입니다. 걸을 때는 보스의 상태가 정한 대상(플레이어 또는 마차)을 향하고, 행동을 하는 동안에는 서 있습니다.
    Boss,
}

impl Monster {
    pub(crate) fn new(
        id: MonsterId,
        kind: MonsterKind,
        order: Order,
        pos: Vec2,
        t: &Tuning,
    ) -> Monster {
        Monster {
            id,
            kind,
            team: Team::HORDE,
            order,
            pos,
            facing: Vec2::new(0.0, 1.0),
            hp: t.monsters.get(kind).max_hp,
            cooldown_ticks: 0,
            last_attack_tick: None,
            effects: MonsterEffects::default(),
        }
    }

    /// 이동 속도, 공격 피해량, 쿨다운을 읽는 유일한 곳입니다. 감속은 스스로 걷는 속도에만 곱하고, 공격의 쿨다운은 바꾸지 않습니다.
    /// 스턴은 속도가 아니라 행동을 막으므로 `plan`이 따로 봅니다.
    pub(crate) fn stats(&self, t: &Tuning) -> UnitStats {
        let kind = t.monsters.get(self.kind);
        UnitStats {
            move_speed: kind.speed * self.effects.walk_factor(),
            attack_damage: kind.attack_damage,
            attack_cooldown_ticks: t.ticks(kind.attack_cooldown_secs),
        }
    }
}

/// 쫓기는 플레이어가 서 있는 타일과 지키는 자리의 타일마다 만든 흐름장입니다. 플레이어는 움직이므로 마차와 포탑처럼 한 번에 만들어 둘 수 없습니다.
/// 마차와 포탑을 향하는 흐름장은 모든 몬스터가 공유하는 `Paths`에 있으므로 여기에 없습니다.
///
/// 표는 도착 타일을 키로 하므로, 같은 타일을 향하는 몬스터가 여럿이어도 표는 하나입니다.
/// 세계의 배치(`layout_revision`)가 바뀌면 낡은 것으로 보고 다시 만들고, 이번 Tick에 필요 없는 표는 버립니다.
pub(crate) struct TileFields {
    entries: BTreeMap<Tile, (u64, FlowField)>,
}

impl TileFields {
    pub fn new() -> TileFields {
        TileFields {
            entries: BTreeMap::new(),
        }
    }

    fn refresh(&mut self, goals: &[Goal], world: &World) {
        let needed: BTreeSet<Tile> = goals
            .iter()
            .filter_map(|goal| match goal {
                Goal::Player(id) => chase_tile(world, *id),
                Goal::Post(tile) => Some(*tile),
                Goal::Tower | Goal::Wagon | Goal::Stay => None,
            })
            .collect();
        self.entries.retain(|tile, _| needed.contains(tile));
        for tile in needed {
            let up_to_date = matches!(
                self.entries.get(&tile),
                Some((revision, _)) if *revision == world.layout_revision()
            );
            if !up_to_date {
                self.entries.insert(
                    tile,
                    (
                        world.layout_revision(),
                        FlowField::toward(&world.tiles, &[tile]),
                    ),
                );
            }
        }
    }

    fn get(&self, tile: Tile) -> Option<&FlowField> {
        self.entries.get(&tile).map(|(_, field)| field)
    }
}

/// 몬스터가 플레이어를 쫓아갈 때 향하는 타일입니다. 플레이어에게 가장 가까운 비어 있는 타일입니다.
fn chase_tile(world: &World, player: PlayerId) -> Option<Tile> {
    let player = world.players.get(&player)?;
    world.tiles.nearest_free(player.pos)
}

/// 몬스터가 이번 Tick에 향하는 곳입니다. 몬스터는 목표를 기억하지 않고 Tick마다 새로 고릅니다.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Player(PlayerId),
    /// 걸어서 가장 가까운 서 있는 포탑입니다. 어느 포탑을 공격할지는 공격이 닿는 거리에 들어온 뒤에 정합니다.
    Tower,
    Wagon,
    /// 지키는 자리로 돌아갑니다. 공격할 대상이 없습니다.
    Post(Tile),
    /// 움직이지도 공격하지도 않습니다.
    Stay,
}

/// 한 몬스터가 이번 Tick에 하기로 결정한 일입니다.
struct Action {
    /// 이번 Tick에 공격할 대상입니다. 공격이 닿는 거리 안에 있을 때만 값이 있습니다.
    hit: Option<Target>,
    /// 몸을 돌릴 방향입니다. 길이가 1이거나 0입니다.
    heading: Vec2,
    /// 이번 Tick에 옮길 거리입니다. 걸음과 다른 몬스터에게서 밀려나는 거리를 합친 값입니다.
    step: Vec2,
}

/// 모든 몬스터를 한 Tick 진행합니다. 먼저 모든 몬스터의 행동을 읽기만 하며 결정하고, 그 뒤에 적용합니다.
/// 그래서 몬스터를 어떤 순서로 처리해도 결과가 같습니다.
pub(crate) fn monsters_act(
    world: &mut World,
    ctx: &Ctx,
    paths: &Paths,
    fields: &mut TileFields,
    hits: &mut Vec<Hit>,
) {
    let t = ctx.t;
    let goals: Vec<Goal> = world
        .monsters
        .iter()
        .map(|monster| choose_goal(world, monster, ctx, paths))
        .collect();
    fields.refresh(&goals, world);

    let mut neighbours = Vec::new();
    let mut actions = Vec::with_capacity(goals.len());
    for (index, goal) in goals.iter().enumerate() {
        actions.push(plan(
            index,
            *goal,
            world,
            ctx,
            paths,
            fields,
            &mut neighbours,
        ));
    }

    let mut scratch = Vec::new();
    for (monster, action) in world.monsters.iter_mut().zip(&actions) {
        if action.heading != Vec2::ZERO {
            monster.facing = action.heading;
        }
        if action.step != Vec2::ZERO {
            monster.pos = move_circle(
                monster.pos,
                t.monsters.get(monster.kind).radius,
                action.step,
                &world.tiles,
                &mut scratch,
            );
        }
        monster.cooldown_ticks = monster.cooldown_ticks.saturating_sub(1);
        if let Some(target) = action.hit
            && monster.cooldown_ticks == 0
        {
            let stats = monster.stats(t);
            hits.push(Hit {
                target,
                amount: stats.attack_damage,
                by: Source::Monster,
                debuff: None,
            });
            monster.cooldown_ticks = stats.attack_cooldown_ticks;
            monster.last_attack_tick = Some(ctx.tick);
        }
    }
}

/// 몬스터의 목표를 고릅니다. 몬스터는 목표를 기억하지 않고 Tick마다 새로 고릅니다.
/// 이 한 함수가 「쫓다가 범위를 벗어나면 다시 마차로(밤) 또는 자리로(낮)」, 「쓰러지거나 사망했거나 자리를 비운 플레이어는 대상이 아님」,
/// 「파괴된 포탑은 대상이 아님」을 정합니다. 명령(`Order`)이 어느 쪽 규칙을 쓸지 정합니다.
fn choose_goal(world: &World, monster: &Monster, ctx: &Ctx, paths: &Paths) -> Goal {
    match monster.order {
        Order::Raid => raid_goal(world, monster, ctx, paths),
        Order::Guard { post } => guard_goal(world, monster, post, ctx),
        Order::Stand => Goal::Stay,
        Order::Boss => boss_goal(world),
    }
}

/// 보스의 목표입니다. 걷는 동안에는 보스의 상태가 정한 대상이고, 예고하는 동안에는 서 있습니다.
fn boss_goal(world: &World) -> Goal {
    match world.boss.as_ref().and_then(|boss| boss.walk_aim()) {
        Some(Aim::Player(id)) => Goal::Player(id),
        Some(Aim::Wagon) => Goal::Wagon,
        None => Goal::Stay,
    }
}

/// 자리를 지키는 몬스터의 목표입니다. 자리의 중심에서 `chase_range` 안에 있고 몬스터와 적대하는 노릴 수 있는(`Player::is_targetable`) 플레이어 가운데
/// 몬스터에게 가장 가까운 플레이어를 쫓고, 없으면 자리로 돌아갑니다. 범위를 몬스터가 아니라 자리에서 재므로,
/// 플레이어가 자리에서 멀어지면 쫓아 나오던 몬스터가 곧바로 돌아서고 자리에서 계속 멀어지지 않습니다.
fn guard_goal(world: &World, monster: &Monster, post: Tile, ctx: &Ctx) -> Goal {
    let chase_range = ctx.t.guards.chase_range;
    let nearest = world
        .players
        .values()
        .filter(|player| player.is_targetable() && monster.team.is_hostile_to(player.team))
        .filter(|player| post.center().distance(player.pos) <= chase_range)
        .min_by(|a, b| {
            let (da, db) = (monster.pos.distance(a.pos), monster.pos.distance(b.pos));
            da.total_cmp(&db)
        });
    match nearest {
        Some(player) => Goal::Player(player.id),
        None => Goal::Post(post),
    }
}

/// 마차를 향하는 몬스터의 목표입니다. 무엇을 쫓는지는 종류의 수치(`MonsterTuning::prey`)가 정합니다.
fn raid_goal(world: &World, monster: &Monster, ctx: &Ctx, paths: &Paths) -> Goal {
    match ctx.t.monsters.get(monster.kind).prey {
        Prey::Nearby => nearby_goal(world, monster, ctx, paths),
        Prey::Wagon => Goal::Wagon,
    }
}

/// 가까운 대상을 쫓는 몬스터(`Prey::Nearby`)의 목표입니다.
///
/// 후보는 추적 거리(`aggro_range`) 안에 있고 몬스터와 적대하는 노릴 수 있는(`Player::is_targetable`) 플레이어와, 걸어서 추적 거리 안에 있는 서 있는 포탑입니다.
/// 플레이어까지의 거리는 직선으로 재고, 포탑까지의 거리는 포탑 흐름장의 걸음 수로 잽니다.
/// 그래서 벽 너머에 있는 포탑을 쫓아서 돌아가지 않습니다.
/// 후보 가운데 가장 가까운 대상을 고르고, 후보가 없으면 마차를 고릅니다.
fn nearby_goal(world: &World, monster: &Monster, ctx: &Ctx, paths: &Paths) -> Goal {
    let t = ctx.t;
    let aggro_range = t.monsters.get(monster.kind).aggro_range;
    let mut best: Option<(f32, Goal)> = None;
    let mut consider = |goal: Goal, distance: f32| {
        let closer = best.is_none_or(|(best_distance, _)| distance < best_distance);
        if distance <= aggro_range && closer {
            best = Some((distance, goal));
        }
    };
    for player in world.players.values() {
        if player.is_targetable() && monster.team.is_hostile_to(player.team) {
            let distance = (monster.pos.distance(player.pos) - t.player.radius).max(0.0);
            consider(Goal::Player(player.id), distance);
        }
    }
    let here = world.tiles.tile_at(monster.pos);
    if let Some(steps) = paths.towers().steps_at(&world.tiles, here) {
        consider(Goal::Tower, f32::from(steps));
    }
    match best {
        Some((_, goal)) => goal,
        None => Goal::Wagon,
    }
}

/// 목표를 향해 걷거나, 가까이 있으면 멈춰서 공격하기로 결정합니다.
/// 걷는 길은 목표에 따라 `Paths`의 마차 흐름장이나 포탑 흐름장, 또는 `TileFields`의 표에서 읽고,
/// 길이 없으면 목표의 가장 가까운 점으로 곧바로 걷습니다.
fn plan(
    index: usize,
    goal: Goal,
    world: &World,
    ctx: &Ctx,
    paths: &Paths,
    fields: &TileFields,
    neighbours: &mut Vec<usize>,
) -> Action {
    let t = ctx.t;
    let monster = &world.monsters[index];
    let kind = t.monsters.get(monster.kind);
    // 스턴에 걸린 몬스터는 스스로 걷지도 공격하지도 않지만, 겹친 몬스터에게서 밀려나는 것은 그대로입니다.
    // 공격의 쿨다운은 `monsters_act`가 계속 줄이므로, 스턴이 풀린 뒤에 밀린 공격이 한꺼번에 나오지 않고 공격 한 번이 바로 나갈 뿐입니다.
    if monster.effects.is_stunned() {
        return Action {
            hit: None,
            heading: Vec2::ZERO,
            step: crowd_step(index, Vec2::ZERO, world, ctx, neighbours),
        };
    }
    if let Goal::Post(post) = goal {
        let walk = walk_to_post(post, monster, world, fields, t);
        return Action {
            hit: None,
            heading: walk.normalize_or_zero(),
            step: crowd_step(index, walk, world, ctx, neighbours),
        };
    }
    let Some((target, body)) = approached(goal, world, monster, t) else {
        return Action {
            hit: None,
            heading: Vec2::ZERO,
            step: Vec2::ZERO,
        };
    };

    let in_reach = body.gap_from(monster.pos, kind.radius) <= kind.attack_reach;
    let (heading, walk) = if in_reach {
        let toward_target = body.closest_point(monster.pos) - monster.pos;
        (toward_target.normalize_or_zero(), Vec2::ZERO)
    } else {
        let field = match goal {
            Goal::Wagon => Some(paths.wagon()),
            Goal::Tower => Some(paths.towers()),
            Goal::Player(id) => chase_tile(world, id).and_then(|tile| fields.get(tile)),
            Goal::Post(post) => fields.get(post),
            Goal::Stay => None,
        };
        // 흐름장이 알려 주는 다음 타일이 없으면(도착 타일 안이거나 길이 없으면) 목표의 가장 가까운 점으로 곧바로 걸어갑니다.
        let aim = field
            .and_then(|field| field.next_waypoint(&world.tiles, monster.pos))
            .unwrap_or_else(|| body.closest_point(monster.pos));
        let direction = (aim - monster.pos).normalize_or_zero();
        (
            direction,
            direction * (monster.stats(t).move_speed * t.dt()),
        )
    };
    // 근접 피해가 0인 몬스터는 대상에게 다가가서 멈출 뿐 공격하지 않습니다. 0의 피해도 하는 일(상자 열기)을 취소하기 때문입니다.
    Action {
        hit: in_reach
            .then_some(target)
            .filter(|_| kind.attack_damage > 0),
        heading,
        step: crowd_step(index, walk, world, ctx, neighbours),
    }
}

/// 지키는 자리로 돌아가는 한 걸음입니다. 길은 자리를 도착 타일로 하는 흐름장이 알려 주고, 자리의 중심에 닿으면 멈춥니다.
/// 걸음이 남은 거리보다 길면 남은 거리만큼만 걸으므로, 자리를 지나쳤다가 되돌아오는 떨림이 없습니다.
fn walk_to_post(
    post: Tile,
    monster: &Monster,
    world: &World,
    fields: &TileFields,
    t: &Tuning,
) -> Vec2 {
    let center = post.center();
    let aim = fields
        .get(post)
        .and_then(|field| field.next_waypoint(&world.tiles, monster.pos))
        .unwrap_or(center);
    let toward = aim - monster.pos;
    let reach = monster.stats(t).move_speed * t.dt();
    if toward.length() <= reach {
        toward
    } else {
        toward.normalize_or_zero() * reach
    }
}

/// 몬스터가 공격하려고 다가가는 대상과 그 모양입니다. 대상이 사라졌으면 `None`입니다.
/// 포탑을 향하는 몬스터는 직선으로 가장 가까운 서 있는 포탑을 대상으로 삼습니다.
/// 걸어서 닿는 포탑은 흐름장이 안내하고, 공격할 포탑은 닿는 거리에 들어온 뒤에야 의미가 있기 때문입니다.
fn approached(goal: Goal, world: &World, monster: &Monster, t: &Tuning) -> Option<(Target, Body)> {
    let target = match goal {
        Goal::Player(id) => Target::Player(id),
        Goal::Wagon => Target::Wagon,
        Goal::Post(_) | Goal::Stay => return None,
        Goal::Tower => {
            let nearest = world
                .towers
                .values()
                .filter(|tower| tower.is_standing() && monster.team.is_hostile_to(tower.team))
                .min_by(|a, b| {
                    let (da, db) = (
                        a.rect().distance_to(monster.pos),
                        b.rect().distance_to(monster.pos),
                    );
                    da.total_cmp(&db)
                })?;
            Target::Tower(nearest.id)
        }
    };
    world.body_of(target, t).map(|body| (target, body))
}

/// 걸음(`walk`)에 주변 몬스터와의 관계를 반영해서, 이번 Tick에 실제로 옮길 거리를 돌려줍니다.
///
/// - 겹친 몬스터 쪽으로 걷는 성분은 없앱니다. 앞선 몬스터를 밀고 들어가지 않고 그 뒤에 줄을 서므로, 몬스터가 한 점에 쌓이지 않습니다.
/// - 겹친 몬스터에게서 멀어지는 방향으로 부드럽게 밀려납니다. 겹친 정도가 클수록 많이 밀립니다.
///
/// 이 두 가지 덕분에 몬스터가 목표 둘레에 고리 모양으로 모이고, 몬스터가 수백 마리여도 한 칸에 몰리는 수가 늘지 않습니다.
fn crowd_step(
    index: usize,
    mut walk: Vec2,
    world: &World,
    ctx: &Ctx,
    neighbours: &mut Vec<usize>,
) -> Vec2 {
    let t = ctx.t;
    let monster = &world.monsters[index];
    let kind = t.monsters.get(monster.kind);
    ctx.grid.query(
        monster.pos,
        kind.radius + t.monsters.largest_radius(),
        neighbours,
    );

    let mut push = Vec2::ZERO;
    for &other_index in neighbours.iter() {
        if other_index == index {
            continue;
        }
        let other = &world.monsters[other_index];
        let touching = kind.radius + t.monsters.get(other.kind).radius;
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
        let into_neighbour = -walk.dot(away);
        if into_neighbour > 0.0 {
            walk = walk + away * into_neighbour;
        }
    }
    if push.length() > 1.0 {
        push = push.normalize_or_zero();
    }
    walk + push * (kind.separation_speed * t.dt())
}
