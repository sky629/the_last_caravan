//! 한 원정과 그 앞의 야영지 동안 변하는 상태(`World`), 한 Tick 동안 규칙이 읽는 값(`Ctx`),
//! 그리고 피해가 상태로 바뀌는 유일한 곳(`World::apply_hits`)입니다.
//!
//! 공격하는 쪽(플레이어, 포탑, 몬스터)은 피해를 직접 적용하지 않고 `Hit`만 만듭니다. `Hit`은 누가 준 피해인지(`Source`)도 담습니다.
//! 모든 공격이 결정된 뒤에 `apply_hits`가 한꺼번에 적용하므로, 개체를 어떤 순서로 방문해도 결과가 같고,
//! 같은 Tick에 죽는 몬스터도 자기 공격은 마칩니다. 몬스터의 HP를 0으로 만든 피해가 그 처치의 주인을 정합니다.
//! 한 개체의 행동이 다른 개체의 현재 상태에 달린 곳(상호작용, 몬스터의 이동)도 같은 방식으로,
//! 먼저 `&World`를 읽기만 하면서 모든 결정을 `Vec`에 모으고 그 뒤에 `&mut World`로 적용합니다.
//! 그래서 한 맵을 읽으면서 같은 맵을 고치려는 대여 충돌이 생기지 않습니다.
//!
//! 막힌 곳을 아는 것은 `World::tiles` 하나입니다. 포탑을 놓고 지우는 일은 `add_tower`, `finish_tower`, `remove_tower` 세 함수만 하고,
//! 무기 상자를 놓고 지우는 일은 `add_chest`, `remove_chest` 두 함수만 합니다. 이 함수들이 타일 지도와 목록을 함께 고치면서 `layout_revision`을 올립니다.
//! 몬스터의 이동 경로(`Paths`, `TileFields`)는 이 번호가 바뀌면 다시 만듭니다.
//! `towers`에 직접 넣거나 빼면 타일 지도와 어긋나므로 하지 않습니다.

mod tiles;

use std::collections::BTreeMap;

use crate::boss::Boss;
use crate::collision::Body;
use crate::effect;
use crate::ids::{
    ChestId, GroundWeaponId, MonsterId, PileId, PlayerId, ProjectileId, Team, TowerId,
};
use crate::input::PlayerInputs;
use crate::kind::{MonsterKind, StructureKind, WeaponKind};
use crate::loot::{Chest, GroundWeapon, Rack};
use crate::math::Vec2;
use crate::monster::{Monster, Order};
use crate::player::Player;
use crate::projectile::{Launch, Projectile};
use crate::resources::{self, GatherNode, Pile};
use crate::spatial::MonsterGrid;
use crate::structures::{Tower, TowerState, Wagon};
use crate::tuning::{BossTuning, Debuff, Tuning};

pub use tiles::Tile;
pub(crate) use tiles::{TileMap, TileUse};

/// 원정과 야영지 동안 변하는 상태 전체입니다. `Sim`이 하나만 가집니다.
/// 순회 순서가 번호 순서로 정해지도록 `HashMap` 대신 `BTreeMap`을 씁니다. 그래야 같은 입력이 항상 같은 결과를 냅니다.
pub(crate) struct World {
    pub players: BTreeMap<PlayerId, Player>,
    /// 번호가 작은 몬스터가 앞에 옵니다. 몬스터는 번호 순서로 추가되고 순서를 바꾸지 않고 제거하므로 이 순서가 유지됩니다.
    /// 수백 마리를 차례로 처리하기에 가장 단순하고 빠른 연속 배열입니다.
    pub monsters: Vec<Monster>,
    /// 날아가는 탄환과 이번 Tick에 끝난 탄환입니다. 번호가 작은 탄환이 앞에 옵니다.
    /// 탄환은 번호 순서로 추가되고 순서를 바꾸지 않고 제거하므로 이 순서가 유지됩니다. 스냅샷과 클라이언트가 번호 순서에 기댑니다.
    pub projectiles: Vec<Projectile>,
    /// 서 있는 포탑과 짓는 중인 포탑입니다. 번호를 키로 하고, 넣고 빼는 일은 `add_tower`, `finish_tower`, `remove_tower`만 합니다.
    pub towers: BTreeMap<TowerId, Tower>,
    pub wagon: Wagon,
    /// 맵에 적힌 채집 지점입니다. 번호가 작은 지점이 앞에 오고, 위치는 원정 동안 바뀌지 않으며 남은 양만 변합니다.
    pub nodes: Vec<GatherNode>,
    /// 땅에 떨어진 자원입니다. 번호가 작은 더미가 앞에 옵니다.
    pub piles: Vec<Pile>,
    /// 모든 플레이어가 함께 쓰는 보관함의 자원입니다.
    pub stash: u32,
    /// 낮에 놓인 무기 상자입니다. 번호가 작은 상자가 앞에 옵니다. 넣고 빼는 일은 `add_chest`, `remove_chest`만 하고, 타일 지도가 함께 바뀝니다.
    pub chests: Vec<Chest>,
    /// 바닥에 놓인 무기입니다. 번호가 작은 무기가 앞에 옵니다. 원정이 끝날 때까지 남습니다.
    pub ground_weapons: Vec<GroundWeapon>,
    /// 야영지의 무기 진열대입니다. 야영지에만 있고 `WeaponKind::ALL`의 순서입니다.
    pub racks: Vec<Rack>,
    /// 이번 원정에서 포탑이 마지막 피해를 주어 처치한 몬스터의 수입니다. 새 원정이나 야영지가 시작되면 0으로 돌아갑니다(`restart`).
    pub tower_kills: u32,
    /// 싸우는 중인 보스의 상태입니다. 보스의 몸은 `monsters`에 있고, 이 값의 `id`가 그 몬스터를 가리킵니다.
    /// 보스가 나타나는 `spawn_boss`가 둘을 함께 만들고, `apply_hits`가 보스를 제거하는 순간 이 값도 함께 비웁니다. 그래서 몸이 없는 상태와 상태가 없는 몸은 생기지 않습니다.
    pub boss: Option<Boss>,
    /// 읽기만 할 수 있습니다. `TileMap::set`은 이 모듈 밖에서 부를 수 없습니다.
    pub tiles: TileMap,
    /// 포탑이 생기거나, 서거나, 사라질 때마다 올라가고 줄어들지 않습니다.
    /// 이동 경로는 이 번호가 마지막으로 만들 때와 다르면 낡은 것으로 보고 다시 만듭니다.
    layout_revision: u64,
    next_monster: u32,
    next_projectile: u32,
    next_tower: u32,
    next_pile: u32,
    next_chest: u32,
    next_ground_weapon: u32,
}

/// 세계를 어떤 모습으로 놓는지입니다. 두 모습은 같은 지도를 씁니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    /// 야영지입니다. 채집 지점, 상자, 포탑이 없고, 허수아비와 무기 진열대가 있습니다. 보관함은 비어 있습니다.
    Camp,
    /// 원정의 첫날이 시작되기 직전입니다. 채집 지점이 가득 차 있고, 포탑은 `village.start_towers`에 적힌 것뿐이고, 보관함에는 시작 자원이 있습니다.
    Expedition,
}

/// 원정이 시작될 때의 지면입니다. 장애물과 마차에 채집 지점을 더한 타일 지도와, 가득 찬 채집 지점입니다.
fn ground(t: &Tuning) -> (TileMap, Vec<GatherNode>) {
    let nodes = resources::nodes_of(t);
    let mut tiles = TileMap::from_map(&t.map);
    for node in &nodes {
        tiles.set(node.tile, TileUse::Node(node.id));
    }
    (tiles, nodes)
}

impl World {
    /// 플레이어가 없는 세계를 `layout`의 모습으로 만듭니다.
    pub fn new(t: &Tuning, layout: Layout) -> World {
        let mut world = World {
            players: BTreeMap::new(),
            monsters: Vec::new(),
            projectiles: Vec::new(),
            towers: BTreeMap::new(),
            wagon: Wagon::new(t),
            nodes: Vec::new(),
            piles: Vec::new(),
            stash: 0,
            chests: Vec::new(),
            ground_weapons: Vec::new(),
            racks: Vec::new(),
            tower_kills: 0,
            boss: None,
            tiles: TileMap::from_map(&t.map),
            layout_revision: 0,
            next_monster: 1,
            next_projectile: 1,
            next_tower: 1,
            next_pile: 1,
            next_chest: 1,
            next_ground_weapon: 1,
        };
        world.restart(t, layout);
        world
    }

    /// 플레이어만 남기고 세계를 `layout`의 모습으로 다시 놓습니다. 몬스터(허수아비 포함), 탄환, 포탑, 떨어진 자원, 무기 상자, 바닥의 무기, 진열대를 모두 치우고,
    /// 포탑의 처치 수를 0으로 되돌리고, 마차를 최대 HP로 되돌리고, 타일 지도를 다시 만듭니다. 야영지에서 생긴 것은 원정으로 이어지지 않고, 원정에서 생긴 것은 야영지로 이어지지 않습니다.
    /// 번호를 세는 값은 되돌리지 않으므로 이전에 쓴 번호가 다시 쓰이지 않습니다. `layout_revision`이 올라가므로 이동 경로는 다음 `sync`에서 다시 만들어집니다.
    pub fn restart(&mut self, t: &Tuning, layout: Layout) {
        self.monsters.clear();
        self.projectiles.clear();
        self.towers.clear();
        self.piles.clear();
        self.chests.clear();
        self.ground_weapons.clear();
        self.racks.clear();
        self.tower_kills = 0;
        self.boss = None;
        self.wagon = Wagon::new(t);
        self.layout_revision += 1;
        match layout {
            Layout::Expedition => {
                (self.tiles, self.nodes) = ground(t);
                self.stash = t.wagon.start_stash;
                self.place_start_towers(t);
            }
            Layout::Camp => {
                self.tiles = TileMap::from_map(&t.map);
                self.nodes = Vec::new();
                self.stash = 0;
                for spot in &t.map.camp.scarecrows {
                    self.spawn_monster(MonsterKind::Scarecrow, Order::Stand, *spot, t);
                }
                self.racks = t
                    .map
                    .camp
                    .racks()
                    .map(|(kind, pos)| Rack { kind, pos })
                    .collect();
            }
        }
    }

    /// 원정이 시작될 때 이미 서 있는 감시탑을 최대 HP로 놓습니다.
    fn place_start_towers(&mut self, t: &Tuning) {
        let kind = StructureKind::Watchtower;
        for tile in &t.village.start_towers {
            self.add_tower(kind, *tile, Tower::standing_state(kind, t));
        }
    }

    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    /// 타일 `tile`에 포탑을 하나 놓습니다. 타일은 비어 있어야 합니다(호출하는 쪽이 `build::check_site`로 확인합니다).
    /// 짓는 중인 포탑(`TowerState::Building`)과 서 있는 포탑을 같은 함수로 놓습니다.
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

    /// 짓는 중인 포탑을 최대 HP의 서 있는 포탑으로 바꿉니다. 타일 지도는 그대로이고 몬스터의 목표가 늘어납니다.
    pub fn finish_tower(&mut self, id: TowerId, t: &Tuning) {
        if let Some(tower) = self.towers.get_mut(&id) {
            tower.state = Tower::standing_state(tower.kind, t);
            self.layout_revision += 1;
        }
    }

    /// 포탑을 지우고 타일을 비웁니다. 파괴된 포탑과 취소된 건설이 모두 이 함수로 사라집니다.
    /// 짓던 포탑이면 낸 자원이 `paid`에 들어 있으므로 부르는 쪽이 보관함에 돌려줍니다.
    pub fn remove_tower(&mut self, id: TowerId) -> Option<Tower> {
        let tower = self.towers.remove(&id)?;
        self.tiles.set(tower.tile, TileUse::Free);
        self.layout_revision += 1;
        Some(tower)
    }

    /// 플레이어를 방에서 내보냅니다. 든 자원은 그 자리에 떨어집니다. 가진 무기도 그 자리에 떨어지지만,
    /// 시작 무기(`PlayerTuning::start_loadout`)에 있는 종류의 무기는 떨어지지 않습니다. 입장하는 플레이어가 공짜로 받는 무기이기 때문입니다.
    /// 없는 번호는 무시합니다.
    pub fn remove_player(&mut self, id: PlayerId, t: &Tuning) {
        if let Some(player) = self.players.remove(&id) {
            self.add_pile(player.pos, player.carried());
            for kind in player.loadout.weapons() {
                self.set_down_weapon(player.pos, kind, t);
            }
        }
    }

    /// 플레이어가 내놓은 무기를 `pos`의 바닥에 놓습니다. 바닥에 놓일 수 없는 종류(`PlayerTuning::may_lie_on_ground`)는 놓지 않고 사라지게 합니다.
    pub(crate) fn set_down_weapon(&mut self, pos: Vec2, kind: WeaponKind, t: &Tuning) {
        if t.player.may_lie_on_ground(kind) {
            self.drop_weapon(pos, kind);
        }
    }

    /// 타일 `tile`에 무기 상자를 놓습니다. 타일은 비어 있어야 합니다(후보 자리는 `Tuning::validate`가 확인하고, 낮의 몬스터는 상자의 둘레에만 섭니다).
    /// 상자는 채집 지점처럼 길을 막으므로 타일 지도를 고치고 `layout_revision`을 올립니다.
    pub fn add_chest(&mut self, tile: Tile, contents: WeaponKind) -> ChestId {
        let id = ChestId(self.next_chest);
        self.next_chest += 1;
        self.tiles.set(tile, TileUse::Chest(id));
        self.chests.push(Chest { id, tile, contents });
        self.layout_revision += 1;
        id
    }

    /// 상자를 치우고 타일을 비웁니다. 밤이 되어 사라지는 상자가 이 함수로 사라집니다. 없는 번호이면 `None`입니다.
    pub fn remove_chest(&mut self, id: ChestId) -> Option<Chest> {
        let index = self.chests.iter().position(|chest| chest.id == id)?;
        let chest = self.chests.remove(index);
        self.tiles.set(chest.tile, TileUse::Free);
        self.layout_revision += 1;
        Some(chest)
    }

    /// `pos`에 무기를 놓습니다. 번호는 새로 받고 다시 사용하지 않습니다.
    pub fn drop_weapon(&mut self, pos: Vec2, kind: WeaponKind) -> GroundWeaponId {
        let id = GroundWeaponId(self.next_ground_weapon);
        self.next_ground_weapon += 1;
        self.ground_weapons.push(GroundWeapon { id, pos, kind });
        id
    }

    /// `pos`에 자원 `amount`가 떨어진 더미를 놓습니다. 양이 0이면 놓지 않습니다. 번호는 새로 받고 다시 사용하지 않습니다.
    pub fn add_pile(&mut self, pos: Vec2, amount: u32) {
        if amount == 0 {
            return;
        }
        let id = PileId(self.next_pile);
        self.next_pile += 1;
        self.piles.push(Pile { id, pos, amount });
    }

    /// 새 몬스터를 만들어 몬스터 목록의 끝에 추가합니다. 번호는 새로 받고 다시 사용하지 않습니다.
    pub fn spawn_monster(
        &mut self,
        kind: MonsterKind,
        order: Order,
        pos: Vec2,
        t: &Tuning,
    ) -> MonsterId {
        let id = MonsterId(self.next_monster);
        self.next_monster += 1;
        self.monsters.push(Monster::new(id, kind, order, pos, t));
        id
    }

    /// 방에 플레이어가 `party`명 있는 밤의 보스를 `pos`에 만듭니다. 몸(`MonsterKind::Boss`)과 상태(`World::boss`)를 함께 만드는 유일한 곳입니다.
    /// 몸의 HP는 인원에 맞춘 최대 HP입니다. 이미 보스가 있으면 만들지 않고 그 보스의 번호를 돌려줍니다.
    pub fn spawn_boss(&mut self, pos: Vec2, party: u32, cfg: &BossTuning, t: &Tuning) -> MonsterId {
        if let Some(boss) = &self.boss {
            return boss.id;
        }
        let id = self.spawn_monster(MonsterKind::Boss, Order::Boss, pos, t);
        let boss = Boss::new(id, party, cfg, t);
        if let Some(body) = self.monsters.last_mut() {
            body.hp = boss.max_hp;
        }
        self.boss = Some(boss);
        id
    }

    /// 탄환을 만들어 탄환 목록의 끝에 추가합니다. 번호는 새로 받고 다시 사용하지 않습니다.
    pub(crate) fn launch(&mut self, launch: Launch) -> ProjectileId {
        let id = ProjectileId(self.next_projectile);
        self.next_projectile += 1;
        self.projectiles.push(Projectile::new(id, launch));
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
    /// 야영지의 Tick이면 참입니다. 건설 요청(`build::start_builds`)과 무기 버리기(`loot::drops_this_tick`)를 받지 않습니다.
    pub practice: bool,
}

/// 피해를 받거나 쫓기는 대상입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Player(PlayerId),
    Monster(MonsterId),
    Tower(TowerId),
    Wagon,
}

/// 피해를 준 쪽입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// 플레이어의 공격입니다. 탄환은 쏜 플레이어의 것입니다(`Projectile::shooter`).
    Player(PlayerId),
    Tower,
    /// 몬스터의 공격입니다. 몬스터는 몬스터를 맞히지 않으므로 처치를 세는 일이 없습니다.
    Monster,
}

/// 이번 Tick에 결정된 피해입니다. 맞은 몬스터에게 효과도 함께 거는 공격이면 `debuff`가 있습니다.
/// 출혈의 피해도 `Hit`이고 `debuff`는 `None`입니다. 포탑의 사격도 같은 필드로 효과를 겁니다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hit {
    pub target: Target,
    pub amount: u32,
    pub by: Source,
    /// 몬스터에게 거는 효과입니다. 대상이 몬스터가 아니면 항상 `None`입니다.
    pub debuff: Option<Debuff>,
}

/// 이번 Tick에 피해 없이 몬스터에게 걸기로 정한 효과입니다. 효과만 거는 포탑이 만듭니다.
/// 피해가 없으므로 `Hit`이 아니고, 그래서 피해의 적용, 처치의 주인, 야영지의 맞은 기록, 결과 기록에 나타나지 않습니다.
/// `World::apply_hits`가 몬스터를 제거한 뒤에 `effect::apply_afflictions`가 겁니다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Affliction {
    pub target: MonsterId,
    pub debuff: Debuff,
    pub by: Source,
}

/// 이번 Tick에 플레이어를 밀어내기로 정한 기록입니다. 플레이어는 `ticks` Tick 동안 Tick마다 `step`씩 밀려납니다.
/// 밀어내는 공격이 만들고, 피해를 적용한 뒤에 `player::apply_shoves`가 그 플레이어가 아직 살아 있으면 밀려나는 상태(`Interaction::Knocked`)로 바꿉니다.
/// 피해가 없으므로 `Hit`이 아닙니다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shove {
    pub player: PlayerId,
    pub step: Vec2,
    pub ticks: u32,
}

impl World {
    /// 모든 피해를 적용하는 유일한 곳입니다. 몬스터의 HP를 0으로 만든 피해가 있으면 그 피해를 준 쪽이 처치를 가져갑니다.
    /// 같은 Tick에 여럿이 같은 몬스터를 맞히면 `hits`의 순서(플레이어의 공격, 탄환, 포탑, 보스, 몬스터, 출혈)에서 HP를 0으로 만든 피해가 주인입니다.
    /// 포탑이 몬스터에게 준 피해(포탑이 건 출혈 포함)는 그 몬스터의 종류가 받는 비율(`Resist::tower_damage_percent`)만 들어갑니다.
    /// 피해를 모두 적용하고 죽은 몬스터를 제거한 뒤에, 살아남은 몬스터에게 `Hit`이 싣고 온 효과를 겁니다.
    pub fn apply_hits(&mut self, hits: &[Hit], t: &Tuning) {
        for hit in hits {
            match hit.target {
                Target::Player(id) => {
                    if let Some(player) = self.players.get_mut(&id) {
                        // 쓰러지는 플레이어가 들고 있던 자원은 그 자리에 떨어집니다.
                        let dropped = player.take_damage(hit.amount, t);
                        let pos = player.pos;
                        self.add_pile(pos, dropped);
                    }
                }
                Target::Monster(id) => {
                    if let Ok(index) = self.monsters.binary_search_by_key(&id, |m| m.id) {
                        let monster = &mut self.monsters[index];
                        let was_alive = monster.hp > 0;
                        let dealt = t
                            .monsters
                            .get(monster.kind)
                            .resist
                            .damage_from(hit.by, hit.amount);
                        monster.hp = monster.hp.saturating_sub(dealt);
                        // 보스는 가장 많이 때린 플레이어를 노리므로, 플레이어가 준 피해를 기억합니다. 포탑의 피해는 기억하지 않습니다.
                        if let (Some(boss), Source::Player(by)) = (&mut self.boss, hit.by)
                            && boss.id == id
                        {
                            boss.record(by, dealt);
                        }
                        if was_alive && monster.hp == 0 {
                            self.credit_kill(hit.by);
                        }
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
        // HP가 0이 된 포탑은 같은 Tick에 사라지고, 그 타일은 다시 지나갈 수 있고 건설할 수 있는 빈 타일이 됩니다.
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
        // 보스의 몸이 제거되면 보스의 상태도 함께 사라집니다.
        if let Some(boss) = &self.boss
            && self
                .monsters
                .binary_search_by_key(&boss.id, |m| m.id)
                .is_err()
        {
            self.boss = None;
        }
        effect::apply_debuffs(self, hits, t);
    }

    /// 몬스터를 쓰러뜨린 처치를 `by`의 것으로 셉니다. 쏜 플레이어가 이미 방을 떠났으면 셀 사람이 없습니다.
    fn credit_kill(&mut self, by: Source) {
        match by {
            Source::Player(id) => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.record.kills += 1;
                }
            }
            Source::Tower => self.tower_kills += 1,
            Source::Monster => {}
        }
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
