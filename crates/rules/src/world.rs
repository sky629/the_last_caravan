//! 한 원정 동안 변하는 상태(`World`), 한 Tick 동안 규칙이 읽는 값(`Ctx`),
//! 그리고 피해가 상태로 바뀌는 유일한 곳(`World::apply_hits`)입니다.
//!
//! 공격하는 쪽(플레이어, 감시탑, 몬스터)은 피해를 직접 적용하지 않고 `Hit`만 만듭니다.
//! 모든 공격이 결정된 뒤에 `apply_hits`가 한꺼번에 적용하므로, 개체를 어떤 순서로 방문해도 결과가 같고,
//! 같은 Tick에 죽는 몬스터도 자기 공격은 마칩니다.
//! 한 개체의 행동이 다른 개체의 현재 상태에 달린 곳(상호작용, 몬스터의 이동)도 같은 방식으로,
//! 먼저 `&World`를 읽기만 하면서 모든 결정을 `Vec`에 모으고 그 뒤에 `&mut World`로 적용합니다.
//! 그래서 한 맵을 읽으면서 같은 맵을 고치려는 대여 충돌이 생기지 않습니다.
//!
//! 막힌 곳을 아는 것은 `World::tiles` 하나입니다. 감시탑을 놓고 지우는 일은 `add_tower`, `finish_tower`, `remove_tower` 세 함수만 하고,
//! 세 함수가 타일 지도와 감시탑 목록을 함께 고치면서 `layout_revision`을 올립니다.
//! 몬스터의 이동 경로(`Paths`, `TileFields`)는 이 번호가 바뀌면 다시 만듭니다.
//! `towers`에 직접 넣거나 빼면 타일 지도와 어긋나므로 하지 않습니다.

mod tiles;

use std::collections::BTreeMap;

use crate::collision::Body;
use crate::ids::{MonsterId, PlayerId, Team, TowerId};
use crate::input::PlayerInputs;
use crate::kind::{MonsterKind, StructureKind};
use crate::math::Vec2;
use crate::monster::Monster;
use crate::player::Player;
use crate::spatial::MonsterGrid;
use crate::structures::{Tower, TowerState, Wagon};
use crate::tuning::Tuning;

pub use tiles::Tile;
pub(crate) use tiles::{TileMap, TileUse};

/// 원정 동안 변하는 상태 전체입니다. `Sim`이 하나만 가집니다.
/// 순회 순서가 번호 순서로 정해지도록 `HashMap` 대신 `BTreeMap`을 씁니다. 그래야 같은 입력이 항상 같은 결과를 냅니다.
pub(crate) struct World {
    pub players: BTreeMap<PlayerId, Player>,
    /// 번호가 작은 몬스터가 앞에 옵니다. 몬스터는 번호 순서로 추가되고 순서를 바꾸지 않고 제거하므로 이 순서가 유지됩니다.
    /// 수백 마리를 차례로 처리하기에 가장 단순하고 빠른 연속 배열입니다.
    pub monsters: Vec<Monster>,
    /// 서 있는 감시탑과 짓는 중인 감시탑입니다. 번호를 키로 하고, 넣고 빼는 일은 `add_tower`, `finish_tower`, `remove_tower`만 합니다.
    pub towers: BTreeMap<TowerId, Tower>,
    pub wagon: Wagon,
    /// 모든 플레이어가 함께 쓰는 보관함의 자원입니다.
    pub stash: u32,
    /// 읽기만 할 수 있습니다. `TileMap::set`은 이 모듈 밖에서 부를 수 없습니다.
    pub tiles: TileMap,
    /// 감시탑이 생기거나, 서거나, 사라질 때마다 올라가고 줄어들지 않습니다.
    /// 이동 경로는 이 번호가 마지막으로 만들 때와 다르면 낡은 것으로 보고 다시 만듭니다.
    layout_revision: u64,
    next_monster: u32,
    next_tower: u32,
}

impl World {
    /// 원정이 시작될 때의 세계입니다. 플레이어, 몬스터, 감시탑은 없고, 보관함에는 시작 자원이 있습니다.
    pub fn new(t: &Tuning) -> World {
        World {
            players: BTreeMap::new(),
            monsters: Vec::new(),
            towers: BTreeMap::new(),
            wagon: Wagon::new(t),
            stash: t.wagon.start_stash,
            tiles: TileMap::from_map(&t.map),
            layout_revision: 0,
            next_monster: 1,
            next_tower: 1,
        }
    }

    /// 새 원정을 위해 몬스터, 감시탑, 마차, 보관함, 타일 지도를 처음 상태로 되돌립니다. 플레이어는 그대로입니다.
    /// 번호를 세는 값은 되돌리지 않으므로 이전 원정의 번호가 다시 쓰이지 않습니다.
    pub fn restart(&mut self, t: &Tuning) {
        self.monsters.clear();
        self.towers.clear();
        self.wagon = Wagon::new(t);
        self.stash = t.wagon.start_stash;
        self.tiles = TileMap::from_map(&t.map);
        self.layout_revision += 1;
    }

    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    /// 타일 `tile`에 감시탑을 하나 놓습니다. 타일은 비어 있어야 합니다(호출하는 쪽이 `build::check_site`로 확인합니다).
    /// 짓는 중인 감시탑(`TowerState::Building`)과 서 있는 감시탑을 같은 함수로 놓습니다.
    pub fn add_tower(&mut self, kind: StructureKind, tile: Tile, state: TowerState) -> TowerId {
        let id = TowerId(self.next_tower);
        self.next_tower += 1;
        self.tiles.set(tile, TileUse::Tower(id));
        self.towers.insert(
            id,
            Tower {
                id,
                kind,
                team: Team::CARAVAN,
                tile,
                state,
            },
        );
        self.layout_revision += 1;
        id
    }

    /// 짓는 중인 감시탑을 최대 HP의 서 있는 감시탑으로 바꿉니다. 타일 지도는 그대로이고 몬스터의 목표가 늘어납니다.
    pub fn finish_tower(&mut self, id: TowerId, t: &Tuning) {
        if let Some(tower) = self.towers.get_mut(&id) {
            tower.state = Tower::standing_state(tower.kind, t);
            self.layout_revision += 1;
        }
    }

    /// 감시탑을 지우고 타일을 비웁니다. 파괴된 감시탑과 취소된 건설이 모두 이 함수로 사라집니다.
    /// 짓던 감시탑이면 낸 자원이 `paid`에 들어 있으므로 부르는 쪽이 보관함에 돌려줍니다.
    pub fn remove_tower(&mut self, id: TowerId) -> Option<Tower> {
        let tower = self.towers.remove(&id)?;
        self.tiles.set(tower.tile, TileUse::Free);
        self.layout_revision += 1;
        Some(tower)
    }

    /// 새 몬스터를 만들어 몬스터 목록의 끝에 추가합니다. 번호는 새로 받고 다시 사용하지 않습니다.
    pub fn spawn_monster(&mut self, kind: MonsterKind, pos: Vec2, t: &Tuning) -> MonsterId {
        let id = MonsterId(self.next_monster);
        self.next_monster += 1;
        self.monsters.push(Monster::new(id, kind, pos, t));
        id
    }
}

/// 한 Tick 동안 규칙이 읽기만 하는 값을 묶은 것입니다. 타일 지도는 `World`에 있으므로 `world.tiles`를 직접 읽습니다.
/// 같은 함수가 `world.players`를 가변으로 빌리면서 `world.tiles`를 공유로 빌릴 수 있는 것은 두 값이 서로 다른 필드이기 때문입니다.
pub(crate) struct Ctx<'a> {
    pub t: &'a Tuning,
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
        // HP가 0이 된 감시탑은 같은 Tick에 사라지고, 그 타일은 다시 지나갈 수 있고 건설할 수 있는 빈 타일이 됩니다.
        let destroyed: Vec<TowerId> = self
            .towers
            .values()
            .filter(|tower| tower.is_standing() && tower.hp() == 0)
            .map(|tower| tower.id)
            .collect();
        for id in destroyed {
            self.remove_tower(id);
        }
        // 몬스터는 모든 피해를 받은 뒤에 한꺼번에 제거합니다. 중간에 제거하면 뒤따르는 피해가 엉뚱한 몬스터에 닿을 수 있습니다.
        self.monsters.retain(|monster| monster.hp > 0);
    }

    /// 몬스터가 쫓거나 공격하는 대상의 모양입니다. 대상이 사라졌으면 `None`입니다.
    pub fn body_of(&self, target: Target, t: &Tuning) -> Option<Body> {
        match target {
            Target::Player(id) => self.players.get(&id).map(|player| Body::Circle {
                center: player.pos,
                radius: t.player.radius,
            }),
            // 몬스터는 다른 몬스터를 쫓지 않으므로, 몬스터의 모양은 아직 필요하지 않습니다.
            Target::Monster(_) => None,
            Target::Tower(id) => self.towers.get(&id).map(|tower| Body::Box(tower.rect())),
            Target::Wagon => Some(Body::Box(self.wagon.rect)),
        }
    }
}
