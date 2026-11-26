//! 플레이어가 무기를 바꾸고 공격하는 일입니다. `Q`를 누르면 두 칸의 무기를 서로 바꾸고, 공격 버튼을 누르면 들고 있는 무기의 공격을 합니다.
//!
//! 무기마다 다른 것은 수치표(`WeaponTable`)의 한 행이고, 공격이 맞히는 곳까지 전달되는 방식(`Delivery`)도 그 행에 적힌 값입니다.
//! 이 파일은 한 Tick에 두 단계로 일합니다. `decide`가 플레이어마다 무기를 바꾸고 쿨다운을 정하면서 이번 Tick의 공격(`Attack`)을 모으고,
//! `deliver`가 모은 공격을 `Delivery`에 따라 처리합니다. 새 방식을 추가하면 `deliver`의 `match`에 갈래가 하나 늘고, 다른 갈래와 `decide`는 그대로입니다.

use crate::collision::Body;
use crate::ids::{PlayerId, Team};
use crate::math::{Rect, Vec2};
use crate::monster::Monster;
use crate::player::{LastAttack, Life};
use crate::projectile::Launch;
use crate::sweep;
use crate::tuning::{ArcTuning, Debuff, Delivery, HitscanTuning, arc_cos};
use crate::world::{Ctx, Hit, Source, Target, World};

/// 이번 Tick에 플레이어가 한 공격입니다. 공격을 정하는 단계가 모으고, 전달하는 단계가 `delivery`에 따라 처리합니다.
struct Attack {
    /// 공격하는 플레이어입니다. 곧바로 맞히는 무기는 사격 선이 끝난 자리를 이 플레이어의 마지막 공격에 적습니다.
    player: PlayerId,
    team: Team,
    /// 공격하는 플레이어의 중심입니다.
    origin: Vec2,
    /// 길이가 1인 조준 방향입니다.
    facing: Vec2,
    damage: u32,
    /// 맞힌 몬스터마다 거는 효과입니다.
    debuff: Option<Debuff>,
    delivery: Delivery,
}

/// 플레이어의 공격을 처리합니다. 맞힌 몬스터의 피해는 `hits`에 모으고, 탄환은 `world`에 새로 생깁니다.
/// `Sim::step`이 부르는 이 파일의 유일한 함수입니다.
pub(crate) fn use_weapons(world: &mut World, ctx: &Ctx, hits: &mut Vec<Hit>) {
    let attacks = decide(world, ctx);
    deliver(world, ctx, &attacks, hits);
}

/// 플레이어마다 무기를 바꾸고, 공격하는지 정합니다.
///
/// `Q`를 누르면 두 칸의 무기를 서로 바꿉니다. 한 칸만 차 있으면 아무것도 바뀌지 않습니다. 바꾸는 조건은 공격하는 조건과 같습니다. 살아 있고 부활, 수리, 채집, 건설, 상자 열기 중이 아니어야 하고,
/// 조건이 맞지 않을 때 누른 `Q`는 사라집니다. 바꾼 무기는 같은 Tick의 공격에 쓰이고, 쿨다운은 무기를 바꿔도 이어집니다.
/// 그래서 검을 휘두른 직후 활로 바꿔도 쿨다운이 끝나기 전에는 쏠 수 없습니다.
///
/// 공격 버튼을 누르고 쿨다운이 끝났으면 공격합니다. 맞히는 몬스터가 없어도 쿨다운이 시작됩니다.
/// 쿨다운은 살아 있는 플레이어마다 Tick마다 줄어듭니다.
fn decide(world: &mut World, ctx: &Ctx) -> Vec<Attack> {
    let t = ctx.t;
    let mut attacks = Vec::new();
    for player in world.players.values_mut() {
        let free = player.is_free_to_act();
        let input = ctx.inputs.get(&player.id).copied().unwrap_or_default();
        if free && input.switch_weapon {
            player.loadout.swap();
        }
        let weapon = player.weapon();
        let stats = player.stats(t);
        let row = t.weapons.get(weapon);
        let (delivery, debuff) = (row.delivery, row.on_hit);
        let Life::Alive { cooldown_ticks, .. } = &mut player.life else {
            continue;
        };
        *cooldown_ticks = cooldown_ticks.saturating_sub(1);
        if !free || !input.attack || *cooldown_ticks > 0 {
            continue;
        }

        *cooldown_ticks = stats.attack_cooldown_ticks;
        player.last_attack = Some(LastAttack {
            tick: ctx.tick,
            weapon,
            trace_end: None,
        });
        attacks.push(Attack {
            player: player.id,
            team: player.team,
            origin: player.pos,
            facing: player.facing,
            damage: stats.attack_damage,
            debuff,
            delivery,
        });
    }
    attacks
}

/// 모은 공격을 방식에 따라 전달합니다. `match`에 `_` 갈래가 없으므로 방식을 추가하면 여기서 컴파일 오류가 납니다.
fn deliver(world: &mut World, ctx: &Ctx, attacks: &[Attack], hits: &mut Vec<Hit>) {
    let mut candidates = Vec::new();
    let mut solids = Vec::new();
    let mut strikes = sweep::Strikes::default();
    for attack in attacks {
        match attack.delivery {
            Delivery::Arc(arc) => swing(attack, arc, &world.monsters, ctx, &mut candidates, hits),
            Delivery::Projectile(spec) => {
                world.launch(Launch {
                    shooter: attack.player,
                    team: attack.team,
                    from: attack.origin,
                    dir: attack.facing,
                    damage: attack.damage,
                    debuff: attack.debuff,
                    spec,
                });
            }
            Delivery::Hitscan(shot) => {
                shoot(attack, shot, world, ctx, &mut solids, &mut strikes, hits);
            }
        }
    }
}

/// 조준 방향의 사거리 안에서 처음 닿는 몬스터 한 마리를 맞힙니다. 막힌 타일이 몬스터보다 가까우면 몬스터를 맞히지 못합니다.
/// 같은 거리에 닿으면 몬스터가 먼저입니다. 사격 선이 끝난 자리(맞힌 몬스터의 가장자리, 막힌 타일, 사거리의 끝)는
/// 쏜 플레이어의 마지막 공격에 적어서 모든 클라이언트가 같은 선을 그리게 합니다.
fn shoot(
    attack: &Attack,
    shot: HitscanTuning,
    world: &mut World,
    ctx: &Ctx,
    solids: &mut Vec<Rect>,
    strikes: &mut sweep::Strikes,
    hits: &mut Vec<Hit>,
) {
    let delta = attack.facing * shot.range;
    let wall = sweep::first_blocked(&world.tiles, attack.origin, delta, 0.0, solids);
    sweep::monsters_along(
        attack.team,
        0.0,
        attack.origin,
        delta,
        &world.monsters,
        ctx,
        strikes,
    );
    let first = strikes
        .found
        .first()
        .filter(|(at, _)| wall.is_none_or(|wall_at| wall_at >= *at));
    let end = match first {
        Some(&(at, id)) => {
            hits.push(Hit {
                target: Target::Monster(id),
                amount: attack.damage,
                by: Source::Player(attack.player),
                debuff: attack.debuff,
            });
            at
        }
        None => wall.unwrap_or(1.0),
    };
    if let Some(last_attack) = world
        .players
        .get_mut(&attack.player)
        .and_then(|player| player.last_attack.as_mut())
    {
        last_attack.trace_end = Some(attack.origin + delta * end);
    }
}

/// 부채꼴 안에 있고 공격 거리 안에 있는 적대적인 몬스터를 모두 맞힙니다.
fn swing(
    attack: &Attack,
    arc: ArcTuning,
    monsters: &[Monster],
    ctx: &Ctx,
    candidates: &mut Vec<usize>,
    hits: &mut Vec<Hit>,
) {
    let t = ctx.t;
    let cos = arc_cos(arc.half_angle_deg);
    let search_radius = t.player.radius + arc.reach + t.monsters.largest_radius();
    ctx.grid.query(attack.origin, search_radius, candidates);
    for &index in candidates.iter() {
        let monster = &monsters[index];
        if !attack.team.is_hostile_to(monster.team) {
            continue;
        }
        let body = Body::Circle {
            center: monster.pos,
            radius: t.monsters.get(monster.kind).radius,
        };
        let in_reach = body.gap_from(attack.origin, t.player.radius) <= arc.reach;
        if in_reach && in_arc(attack.origin, attack.facing, monster.pos, cos) {
            hits.push(Hit {
                target: Target::Monster(monster.id),
                amount: attack.damage,
                by: Source::Player(attack.player),
                debuff: attack.debuff,
            });
        }
    }
}

/// `target`이 `origin`에서 `facing` 방향의 부채꼴 안에 있는지 판정합니다.
fn in_arc(origin: Vec2, facing: Vec2, target: Vec2, arc_cos: f32) -> bool {
    let offset = target - origin;
    let distance = offset.length();
    // 중심이 거의 겹쳤으면 방향을 알 수 없으므로 부채꼴 안으로 봅니다.
    distance < 1e-4 || facing.dot(offset * (1.0 / distance)) >= arc_cos
}
