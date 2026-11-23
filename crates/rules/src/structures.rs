//! 마차와 시설(감시탑)입니다. 둘 다 HP를 가진 단단한 사각형입니다.
//!
//! 시설은 번호를 키로 하는 모음(`World::towers`)에 저장합니다. 1단계에는 감시탑 하나만 있지만,
//! 시설을 건설하는 단계에서도 구조를 바꾸지 않도록 처음부터 모음으로 둡니다.

use std::collections::BTreeMap;

use crate::ids::{MonsterId, Team, TowerId};
use crate::kind::StructureKind;
use crate::math::Rect;
use crate::tuning::{StructurePlacement, Tuning};
use crate::world::{Ctx, Hit, Target, World};

/// 마차입니다. HP가 0이 되면 원정이 실패합니다 (D43).
/// 같은 Tick에 원정이 끝나므로 「파괴된 상태」는 따로 없습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Wagon {
    pub team: Team,
    pub rect: Rect,
    pub hp: u32,
}

impl Wagon {
    pub(crate) fn new(t: &Tuning) -> Wagon {
        Wagon {
            team: Team::CARAVAN,
            rect: t.map.wagon,
            hp: t.wagon.max_hp,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tower {
    pub id: TowerId,
    pub kind: StructureKind,
    pub team: Team,
    pub rect: Rect,
    pub state: TowerState,
}

/// 감시탑의 상태입니다. 파괴된 감시탑에는 HP, 쿨다운, 사격 대상이 없으므로,
/// 「파괴된 감시탑이 사격한다」와 「파괴된 감시탑을 수리한다」는 코드로 표현할 수 없습니다 (D49).
/// 파괴된 감시탑은 잔해로 남아서 계속 길을 막습니다. 새 원정이 시작될 때만 다시 서 있게 됩니다 (D34).
#[derive(Clone, Debug, PartialEq)]
pub enum TowerState {
    Standing {
        /// 항상 1 이상입니다. HP가 0이 되면 `Destroyed`로 바뀝니다.
        hp: u32,
        /// 다음 사격까지 남은 Tick입니다.
        cooldown_ticks: u32,
        last_shot: Option<Shot>,
    },
    Destroyed,
}

/// 감시탑의 마지막 사격입니다. 이벤트가 아니라 상태이므로, 모든 클라이언트가 스냅샷만으로 같은 사격 효과를 그릴 수 있습니다 (C20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shot {
    pub target: MonsterId,
    pub tick: u64,
}

impl Tower {
    fn new(id: TowerId, placement: &StructurePlacement, t: &Tuning) -> Tower {
        Tower {
            id,
            kind: placement.kind,
            team: Team::CARAVAN,
            rect: placement.rect,
            state: TowerState::Standing {
                hp: t.structures.get(placement.kind).max_hp,
                cooldown_ticks: 0,
                last_shot: None,
            },
        }
    }

    /// 파괴된 감시탑의 HP는 0입니다.
    pub fn hp(&self) -> u32 {
        match self.state {
            TowerState::Standing { hp, .. } => hp,
            TowerState::Destroyed => 0,
        }
    }

    pub fn is_standing(&self) -> bool {
        matches!(self.state, TowerState::Standing { .. })
    }

    /// 피해를 받습니다. HP가 0이 되면 파괴됩니다 (C21). 이미 파괴된 감시탑은 피해를 받지 않습니다.
    pub(crate) fn take_damage(&mut self, amount: u32) {
        if let TowerState::Standing { hp, .. } = &mut self.state {
            *hp = hp.saturating_sub(amount);
            if *hp == 0 {
                self.state = TowerState::Destroyed;
            }
        }
    }

    /// HP를 `max_hp`까지 회복합니다. 파괴된 감시탑은 회복하지 않습니다 (D49).
    pub(crate) fn repair(&mut self, amount: u32, max_hp: u32) {
        if let TowerState::Standing { hp, .. } = &mut self.state {
            *hp = (*hp + amount).min(max_hp);
        }
    }
}

/// 맵에 미리 놓인 시설을 최대 HP의 서 있는 상태로 만듭니다. 번호는 맵에 적은 순서대로 1부터 붙입니다.
pub(crate) fn build_towers(t: &Tuning) -> BTreeMap<TowerId, Tower> {
    t.map
        .structures
        .iter()
        .enumerate()
        .map(|(index, placement)| {
            let id = TowerId(index as u32 + 1);
            (id, Tower::new(id, placement, t))
        })
        .collect()
}

/// 서 있는 감시탑이 사정거리 안의 가장 가까운 적대적 몬스터를 쏩니다 (C20).
/// 가장 가까운 몬스터가 여럿이면 번호가 작은 몬스터를 고릅니다. 쏘지 못하는 동안에도 쿨다운은 줄어듭니다.
pub(crate) fn towers_fire(world: &mut World, ctx: &Ctx, hits: &mut Vec<Hit>) {
    let t = ctx.t;
    let mut candidates = Vec::new();
    for tower in world.towers.values_mut() {
        let TowerState::Standing {
            cooldown_ticks,
            last_shot,
            ..
        } = &mut tower.state
        else {
            continue;
        };
        *cooldown_ticks = cooldown_ticks.saturating_sub(1);
        if *cooldown_ticks > 0 {
            continue;
        }

        let spec = t.structures.get(tower.kind);
        let center = tower.rect.center();
        ctx.grid.query(center, spec.range, &mut candidates);
        let mut nearest: Option<(f32, usize)> = None;
        for &index in &candidates {
            let monster = &world.monsters[index];
            if !tower.team.is_hostile_to(monster.team) {
                continue;
            }
            let distance = center.distance(monster.pos);
            let closer = match nearest {
                None => true,
                Some((best, best_index)) => {
                    distance < best || (distance == best && index < best_index)
                }
            };
            if closer {
                nearest = Some((distance, index));
            }
        }

        if let Some((_, index)) = nearest {
            let target = world.monsters[index].id;
            hits.push(Hit {
                target: Target::Monster(target),
                amount: spec.damage,
            });
            *cooldown_ticks = t.ticks(spec.cooldown_secs);
            *last_shot = Some(Shot {
                target,
                tick: ctx.tick,
            });
        }
    }
}
