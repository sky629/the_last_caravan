//! `Sim`은 방 하나의 게임 상태 전체이며, 규칙 크레이트의 공개 진입점입니다.

use crate::expedition::{self, Advance, Phase};
use crate::ids::{PlayerId, Team};
use crate::input::PlayerInputs;
use crate::math::{Rect, Vec2};
use crate::monster::{self, Monster, PlayerFields};
use crate::nav::{FlowField, NavGrid};
use crate::player::{self, Interaction, Player};
use crate::spatial::MonsterGrid;
use crate::structures::{self, Tower, Wagon};
use crate::tuning::{MapSpec, Tuning};
use crate::world::{Ctx, Statics, World};

/// 방 하나의 게임입니다. 바깥 코드는 `&Sim`으로 상태를 읽고,
/// `add_player`, `remove_player`, `step`으로만 상태를 바꿉니다.
pub struct Sim {
    tuning: Tuning,
    /// 방이 만들어진 뒤 실행한 Tick의 수입니다. 줄어들지 않으므로 스냅샷의 Tick 번호는 계속 커집니다.
    tick: u64,
    /// 이 방의 몇 번째 원정인지입니다. 첫 원정이 1입니다.
    expedition: u32,
    phase: Phase,
    world: World,
    statics: Statics,
    /// 몬스터의 위치를 담은 격자입니다. Tick마다 다시 만듭니다.
    monster_grid: MonsterGrid,
    player_fields: PlayerFields,
    next_player: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinError {
    RoomFull { capacity: usize },
}

impl Sim {
    /// 플레이어가 없는 방을 만들고 첫 원정을 준비합니다.
    pub fn new(tuning: Tuning) -> Sim {
        let map = &tuning.map;
        let bounds = Rect::new(0.0, 0.0, map.width, map.height);
        let mut solids = map.obstacles.clone();
        solids.push(map.wagon);
        solids.extend(map.structures.iter().map(|placed| placed.rect));

        let nav = NavGrid::build(
            bounds,
            &solids,
            tuning.monsters.largest_radius(),
            map.nav_cell,
        );
        let wagon_goals = nav.cells_near(map.wagon, tuning.monsters.longest_engage_distance());
        let wagon_field = FlowField::toward(&nav, &wagon_goals);
        let towers = structures::build_towers(&tuning);
        let tower_fields = towers
            .values()
            .map(|tower| {
                let goals = nav.cells_near(tower.rect, tuning.monsters.longest_engage_distance());
                (tower.id, FlowField::toward(&nav, &goals))
            })
            .collect();

        Sim {
            expedition: 1,
            phase: expedition::opening_phase(&tuning),
            world: World::new(Wagon::new(&tuning), towers),
            statics: Statics {
                bounds,
                solids,
                nav,
                wagon_field,
                tower_fields,
            },
            monster_grid: MonsterGrid::new(bounds, map.spatial_cell),
            player_fields: PlayerFields::new(),
            tick: 0,
            next_player: 1,
            tuning,
        }
    }

    /// 새 플레이어를 최대 HP의 살아 있는 상태로 등장 지점에 추가하고 번호를 돌려줍니다.
    /// 원정이 진행되는 도중에도 곧바로 참가합니다 (D35). 방이 가득 차 있으면 거절합니다.
    pub fn add_player(&mut self) -> Result<PlayerId, JoinError> {
        if self.world.players.len() >= self.tuning.max_players {
            return Err(JoinError::RoomFull {
                capacity: self.tuning.max_players,
            });
        }
        let id = PlayerId(self.next_player);
        self.next_player += 1;
        let spawn = spawn_point(&self.tuning.map, self.world.players.len());
        let team = self.team_for_new_player();
        let player = Player::new(id, team, spawn, &self.tuning);
        self.world.players.insert(id, player);
        Ok(id)
    }

    /// 플레이어를 즉시 제거합니다 (D25). 없는 번호는 무시하므로 같은 퇴장을 두 번 처리해도 안전합니다.
    /// 이 플레이어를 부활시키던 플레이어는 다음 Tick에 대상이 사라진 것을 보고 부활을 멈춥니다.
    pub fn remove_player(&mut self, id: PlayerId) {
        self.world.players.remove(&id);
    }

    /// 게임을 Tick 하나만큼 진행합니다. `inputs`는 이번 Tick의 플레이어별 입력이고,
    /// 목록에 없는 플레이어는 아무것도 누르지 않은 것으로 봅니다.
    ///
    /// 모든 공격은 같은 상태를 보고 결정한 뒤 한꺼번에 적용합니다. 그래서 같은 Tick에 죽는 몬스터도 자기 공격은 마칩니다.
    pub fn step(&mut self, inputs: &PlayerInputs) {
        self.tick += 1;
        match expedition::advance_phase(&mut self.phase, &mut self.world, &self.tuning) {
            Advance::Frozen => return,
            Advance::StartNewExpedition => {
                self.reset_expedition();
                return;
            }
            Advance::Simulate => {}
        }

        self.monster_grid.rebuild(&self.world.monsters);
        let ctx = Ctx {
            t: &self.tuning,
            statics: &self.statics,
            inputs,
            grid: &self.monster_grid,
            tick: self.tick,
        };
        let mut hits = Vec::new();
        player::update_interactions(&mut self.world, &ctx);
        player::move_players(&mut self.world, &ctx);
        player::player_attacks(&mut self.world, &ctx, &mut hits);
        structures::towers_fire(&mut self.world, &ctx, &mut hits);
        monster::monsters_act(&mut self.world, &ctx, &mut self.player_fields, &mut hits);
        self.world.apply_hits(&hits, &self.tuning);
        player::advance_life_timers(&mut self.world, &self.tuning);
        expedition::settle_outcome(&mut self.phase, &self.world, &self.tuning);
    }

    /// 새 원정을 시작합니다 (D20, D34). 이전 원정에서 이어받는 것은 방에 있는 플레이어뿐입니다.
    /// 모든 플레이어는 최대 HP의 살아 있는 상태로 등장 지점에 돌아가고, 사망 상태도 초기화됩니다.
    fn reset_expedition(&mut self) {
        self.expedition += 1;
        self.phase = expedition::opening_phase(&self.tuning);
        self.world.monsters.clear();
        self.world.wagon = Wagon::new(&self.tuning);
        self.world.towers = structures::build_towers(&self.tuning);
        for (slot, player) in self.world.players.values_mut().enumerate() {
            player.reset(spawn_point(&self.tuning.map, slot), &self.tuning);
        }
    }

    /// 진영을 정하는 곳은 이 함수 하나입니다. 협동 모드는 모두를 같은 편으로 둡니다.
    /// 팀 대전 모드는 이 함수만 바꾸면 됩니다.
    fn team_for_new_player(&self) -> Team {
        Team::CARAVAN
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn tuning(&self) -> &Tuning {
        &self.tuning
    }

    /// 이 방의 몇 번째 원정인지입니다. 첫 원정이 1입니다.
    pub fn expedition_number(&self) -> u32 {
        self.expedition
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// 번호가 작은 플레이어부터 차례로 돌려줍니다.
    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.world.players.values()
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.world.players.get(&id)
    }

    pub fn player_count(&self) -> usize {
        self.world.players.len()
    }

    /// 번호가 작은 몬스터부터 차례로 돌려줍니다.
    pub fn monsters(&self) -> impl Iterator<Item = &Monster> {
        self.world.monsters.iter()
    }

    /// 번호가 작은 시설부터 차례로 돌려줍니다.
    pub fn towers(&self) -> impl Iterator<Item = &Tower> {
        self.world.towers.values()
    }

    pub fn wagon(&self) -> &Wagon {
        &self.world.wagon
    }

    /// `downed`를 부활시키는 플레이어가 있으면 부활의 진행도(0.0 이상 1.0 이하)를 돌려줍니다.
    /// 진행도는 부활시키는 플레이어의 `Interaction`에서 계산하며, 쓰러진 플레이어는 아무 값도 저장하지 않습니다.
    pub fn revive_progress(&self, downed: PlayerId) -> Option<f32> {
        let required = self.tuning.ticks(self.tuning.revive.revive_secs);
        self.world
            .players
            .values()
            .find_map(|player| match player.interaction() {
                Some(Interaction::Reviving {
                    target,
                    elapsed_ticks,
                }) if *target == downed => Some(*elapsed_ticks as f32 / required as f32),
                _ => None,
            })
    }
}

/// `slot`번째 플레이어가 나타날 위치입니다. 등장 지점을 돌아가며 씁니다.
fn spawn_point(map: &MapSpec, slot: usize) -> Vec2 {
    map.player_spawns
        .get(slot % map.player_spawns.len().max(1))
        .copied()
        .unwrap_or(Vec2::new(map.width * 0.5, map.height * 0.5))
}

#[cfg(test)]
impl Sim {
    /// 테스트가 개체를 원하는 자리에 놓을 때만 씁니다.
    pub(crate) fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub(crate) fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
    }

    pub(crate) fn spawn_monster(&mut self, pos: Vec2) -> crate::ids::MonsterId {
        let kind = self.tuning.waves.kind;
        self.world.spawn_monster(kind, pos, &self.tuning)
    }
}
