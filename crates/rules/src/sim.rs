//! `Sim`은 방 하나의 게임 상태 전체이며, 규칙 크레이트의 공개 진입점입니다.

use crate::ids::{PlayerId, Team};
use crate::input::PlayerInputs;
use crate::math::{Rect, Vec2};
use crate::player::{self, Player};
use crate::tuning::Tuning;
use crate::world::{Ctx, Statics, World};

/// 방 하나의 게임입니다. 바깥 코드는 `&Sim`으로 상태를 읽고,
/// `add_player`, `remove_player`, `step`으로만 상태를 바꿉니다.
pub struct Sim {
    tuning: Tuning,
    /// 방이 만들어진 뒤 실행한 Tick의 수입니다. 줄어들지 않으므로 스냅샷의 Tick 번호는 계속 커집니다.
    tick: u64,
    world: World,
    statics: Statics,
    next_player: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinError {
    RoomFull { capacity: usize },
}

impl Sim {
    /// 플레이어가 없는 방을 만듭니다.
    pub fn new(tuning: Tuning) -> Sim {
        let map = &tuning.map;
        let bounds = Rect::new(0.0, 0.0, map.width, map.height);
        let mut solids = map.obstacles.clone();
        solids.push(map.wagon);
        solids.extend(map.structures.iter().map(|placed| placed.rect));
        Sim {
            tuning,
            tick: 0,
            world: World {
                players: Default::default(),
            },
            statics: Statics { bounds, solids },
            next_player: 1,
        }
    }

    /// 새 플레이어를 최대 HP의 살아 있는 상태로 등장 지점에 추가하고 번호를 돌려줍니다.
    /// 방이 가득 차 있으면 거절합니다.
    pub fn add_player(&mut self) -> Result<PlayerId, JoinError> {
        if self.world.players.len() >= self.tuning.max_players {
            return Err(JoinError::RoomFull {
                capacity: self.tuning.max_players,
            });
        }
        let id = PlayerId(self.next_player);
        self.next_player += 1;
        let spawn = self.player_spawn(self.world.players.len());
        let team = self.team_for_new_player();
        let player = Player::new(id, team, spawn, &self.tuning);
        self.world.players.insert(id, player);
        Ok(id)
    }

    /// 플레이어를 즉시 제거합니다. 없는 번호는 무시하므로 같은 퇴장을 두 번 처리해도 안전합니다.
    pub fn remove_player(&mut self, id: PlayerId) {
        self.world.players.remove(&id);
    }

    /// 게임을 Tick 하나만큼 진행합니다. `inputs`는 이번 Tick의 플레이어별 입력이고,
    /// 목록에 없는 플레이어는 아무것도 누르지 않은 것으로 봅니다.
    pub fn step(&mut self, inputs: &PlayerInputs) {
        self.tick += 1;
        let ctx = Ctx {
            t: &self.tuning,
            statics: &self.statics,
            inputs,
        };
        player::move_players(&mut self.world, &ctx);
    }

    /// 진영을 정하는 곳은 이 함수 하나입니다. 협동 모드는 모두를 같은 편으로 둡니다.
    /// 팀 대전 모드는 이 함수만 바꾸면 됩니다.
    fn team_for_new_player(&self) -> Team {
        Team::CARAVAN
    }

    /// `slot`번째로 입장한 플레이어가 나타날 위치입니다. 등장 지점을 돌아가며 씁니다.
    fn player_spawn(&self, slot: usize) -> Vec2 {
        let spawns = &self.tuning.map.player_spawns;
        spawns
            .get(slot % spawns.len().max(1))
            .copied()
            .unwrap_or(Vec2::new(
                self.tuning.map.width * 0.5,
                self.tuning.map.height * 0.5,
            ))
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn tuning(&self) -> &Tuning {
        &self.tuning
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
}

#[cfg(test)]
impl Sim {
    /// 테스트가 개체를 원하는 자리에 놓을 때만 씁니다.
    pub(crate) fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}
