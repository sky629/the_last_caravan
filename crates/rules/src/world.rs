//! 한 원정 동안 변하는 상태(`World`), 변하지 않는 맵 정보(`Statics`), 한 Tick 동안 규칙이 읽는 값(`Ctx`),
//! 그리고 피해가 상태로 바뀌는 유일한 곳(`World::apply_hits`)입니다.
//!
//! Tick을 처리하는 함수는 모두 같은 순서로 일합니다.
//! 1. `World`를 읽기만 하면서 무엇을 할지 결정해서 `Vec`에 모읍니다.
//! 2. 결정을 `&mut World`로 적용합니다.
//!
//! 공격하는 쪽은 `Hit`만 만들고, 모든 공격이 결정된 뒤에 `apply_hits`가 한꺼번에 적용합니다.
//! 그래서 개체를 어떤 순서로 방문해도 결과가 같고, 같은 Tick에 죽는 몬스터도 자기 공격은 마칩니다.

use std::collections::BTreeMap;

use crate::collision::Body;
use crate::ids::{MonsterId, PlayerId, TowerId};
use crate::input::PlayerInputs;
use crate::kind::MonsterKind;
use crate::math::{Rect, Vec2};
use crate::monster::Monster;
use crate::nav::{FlowField, NavGrid};
use crate::player::Player;
use crate::spatial::MonsterGrid;
use crate::structures::{Tower, Wagon};
use crate::tuning::Tuning;

/// 원정 동안 변하는 상태 전체입니다. `Sim`이 하나만 가집니다.
/// 순회 순서가 번호 순서로 정해지도록 `HashMap` 대신 `BTreeMap`을 씁니다. 그래야 같은 입력이 항상 같은 결과를 냅니다.
pub(crate) struct World {
    pub players: BTreeMap<PlayerId, Player>,
    /// 번호가 작은 몬스터가 앞에 옵니다. 몬스터는 번호 순서로 추가되고 순서를 바꾸지 않고 제거하므로 이 순서가 유지됩니다.
    /// 수백 마리를 차례로 처리하기에 가장 단순하고 빠른 연속 배열입니다.
    pub monsters: Vec<Monster>,
    /// 시설은 번호를 키로 하는 모음입니다. 1단계에는 감시탑 하나만 있습니다.
    pub towers: BTreeMap<TowerId, Tower>,
    pub wagon: Wagon,
    next_monster: u32,
}

impl World {
    pub fn new(wagon: Wagon, towers: BTreeMap<TowerId, Tower>) -> World {
        World {
            players: BTreeMap::new(),
            monsters: Vec::new(),
            towers,
            wagon,
            next_monster: 1,
        }
    }

    /// 새 몬스터를 만들어 몬스터 목록의 끝에 추가합니다. 번호는 새로 받고 다시 사용하지 않습니다.
    pub fn spawn_monster(&mut self, kind: MonsterKind, pos: Vec2, t: &Tuning) -> MonsterId {
        let id = MonsterId(self.next_monster);
        self.next_monster += 1;
        self.monsters.push(Monster::new(id, kind, pos, t));
        id
    }
}

/// `Sim::new`에서 한 번 만들고 그 뒤로 바뀌지 않는 맵 정보입니다.
pub(crate) struct Statics {
    pub bounds: Rect,
    /// 장애물, 마차, 시설입니다. 파괴된 시설도 잔해로 남아서 길을 막습니다.
    pub solids: Vec<Rect>,
    pub nav: NavGrid,
    /// 마차와 시설은 움직이지 않으므로 이들을 향하는 흐름장은 한 번만 만듭니다.
    pub wagon_field: FlowField,
    pub tower_fields: BTreeMap<TowerId, FlowField>,
}

/// 한 Tick 동안 규칙이 읽기만 하는 값을 묶은 것입니다.
pub(crate) struct Ctx<'a> {
    pub t: &'a Tuning,
    pub statics: &'a Statics,
    pub inputs: &'a PlayerInputs,
    pub grid: &'a MonsterGrid,
    pub tick: u64,
}

/// 피해를 받거나 쫓기는 대상입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Player(PlayerId),
    Monster(MonsterId),
    Tower(TowerId),
    Wagon,
}

/// 이번 Tick에 결정된 피해입니다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hit {
    pub target: Target,
    pub amount: u32,
}

impl World {
    /// 모든 피해를 적용하는 유일한 곳입니다.
    pub fn apply_hits(&mut self, hits: &[Hit], t: &Tuning) {
        for hit in hits {
            match hit.target {
                Target::Player(id) => {
                    if let Some(player) = self.players.get_mut(&id) {
                        player.take_damage(hit.amount, t);
                    }
                }
                Target::Monster(id) => {
                    if let Ok(index) = self.monsters.binary_search_by_key(&id, |m| m.id) {
                        let monster = &mut self.monsters[index];
                        monster.hp = monster.hp.saturating_sub(hit.amount);
                    }
                }
                Target::Tower(id) => {
                    if let Some(tower) = self.towers.get_mut(&id) {
                        tower.take_damage(hit.amount);
                    }
                }
                Target::Wagon => {
                    self.wagon.hp = self.wagon.hp.saturating_sub(hit.amount);
                }
            }
        }
        // 몬스터는 모든 피해를 받은 뒤에 한꺼번에 제거합니다. 중간에 제거하면 뒤따르는 피해가 엉뚱한 몬스터에 닿을 수 있습니다.
        self.monsters.retain(|monster| monster.hp > 0);
    }

    /// 거리를 잴 때 쓰는 대상의 모양입니다. 대상이 사라졌으면 `None`입니다.
    pub fn body_of(&self, target: Target, t: &Tuning) -> Option<Body> {
        match target {
            Target::Player(id) => self.players.get(&id).map(|player| Body::Circle {
                center: player.pos,
                radius: t.player.radius,
            }),
            Target::Monster(id) => {
                let index = self.monsters.binary_search_by_key(&id, |m| m.id).ok()?;
                let monster = &self.monsters[index];
                Some(Body::Circle {
                    center: monster.pos,
                    radius: t.monsters.get(monster.kind).radius,
                })
            }
            Target::Tower(id) => self.towers.get(&id).map(|tower| Body::Box(tower.rect)),
            Target::Wagon => Some(Body::Box(self.wagon.rect)),
        }
    }
}
