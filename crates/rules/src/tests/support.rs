//! 테스트에서만 쓰는 상황 구성 도구입니다. 개체를 필요한 자리에 놓은 뒤,
//! 서버가 하는 것과 같은 방식(누르고 있는 입력과 `step`)으로 게임을 진행합니다.

use crate::expedition::Phase;
use crate::ids::{MonsterId, PlayerId};
use crate::input::{AimDir, MoveDir, PlayerInput, PlayerInputs};
use crate::kind::StructureKind;
use crate::math::{Rect, Vec2};
use crate::monster::Monster;
use crate::player::{Life, Player};
use crate::sim::Sim;
use crate::tuning::{MapSpec, StructurePlacement, Tuning};

pub(crate) struct Scenario {
    pub sim: Sim,
    held: PlayerInputs,
}

impl Scenario {
    /// 장애물이 없는 20 x 20 맵입니다. 마차와 감시탑은 구석에 있어서 테스트하는 동작과 겹치지 않습니다.
    /// 무리가 저절로 나타나지 않으므로, 몬스터는 테스트가 `monster_at`으로 놓은 것뿐입니다.
    pub fn open_field() -> Scenario {
        Scenario::with_tuning(|_| {})
    }

    /// `open_field`와 같지만 수치를 먼저 바꿉니다.
    pub fn with_tuning(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
        let mut scenario = Scenario::with_waves(adjust);
        scenario.sim.set_phase(Phase::Intermission {
            next_wave: 1,
            ticks_left: u32::MAX,
        });
        scenario
    }

    /// 무리가 정해진 시간에 저절로 나타나는 시나리오입니다. 원정의 진행을 확인하는 테스트가 씁니다.
    pub fn with_waves(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
        let mut tuning = Tuning {
            map: MapSpec {
                width: 20.0,
                height: 20.0,
                wagon: Rect::new(17.0, 17.0, 19.0, 19.0),
                structures: vec![StructurePlacement {
                    kind: StructureKind::Watchtower,
                    rect: Rect::new(1.0, 17.0, 2.0, 18.0),
                }],
                obstacles: vec![],
                player_spawns: vec![Vec2::new(10.0, 10.0)],
                monster_spawns: vec![Vec2::new(10.0, 0.6)],
                nav_cell: 0.5,
                spatial_cell: 2.0,
            },
            ..Tuning::default()
        };
        adjust(&mut tuning);
        assert_eq!(tuning.validate(), Ok(()), "the scenario's tuning is valid");
        Scenario {
            sim: Sim::new(tuning),
            held: PlayerInputs::new(),
        }
    }

    pub fn player_at(&mut self, x: f32, y: f32) -> PlayerId {
        let id = self.sim.add_player().expect("the room has space");
        self.player_mut(id).pos = Vec2::new(x, y);
        id
    }

    pub fn monster_at(&mut self, x: f32, y: f32) -> MonsterId {
        self.sim.spawn_monster(Vec2::new(x, y))
    }

    pub fn monster(&self, id: MonsterId) -> Option<&Monster> {
        self.sim.monsters().find(|monster| monster.id == id)
    }

    /// 지금 있는 몬스터를 모두 한꺼번에 없앱니다. 플레이어가 무리를 처치한 상황을 만들 때 씁니다.
    pub fn clear_monsters(&mut self) {
        self.sim.world_mut().monsters.clear();
    }

    pub fn set_wagon_hp(&mut self, hp: u32) {
        self.sim.world_mut().wagon.hp = hp;
    }

    /// 플레이어를 출혈 시간이 가득 찬 쓰러진 상태로 만듭니다.
    pub fn knock_down(&mut self, id: PlayerId) {
        let bleed = self.ticks(self.sim.tuning().revive.bleed_secs);
        self.player_mut(id).life = Life::Downed {
            bleed_left_ticks: bleed,
        };
    }

    pub fn kill(&mut self, id: PlayerId) {
        self.player_mut(id).life = Life::Dead;
    }

    fn player_mut(&mut self, id: PlayerId) -> &mut Player {
        self.sim
            .world_mut()
            .players
            .get_mut(&id)
            .expect("the player exists")
    }

    /// 이 입력을 바꿀 때까지 매 Tick 적용합니다. 키를 계속 누르고 있는 것과 같습니다.
    pub fn hold(&mut self, id: PlayerId, input: PlayerInput) {
        self.held.insert(id, input);
    }

    /// `(x, y)` 방향으로 이동하면서 그 방향을 조준합니다.
    pub fn hold_move(&mut self, id: PlayerId, x: f32, y: f32) {
        self.hold(
            id,
            PlayerInput {
                move_dir: MoveDir::new(x, y),
                aim: AimDir::new(x, y),
                ..PlayerInput::default()
            },
        );
    }

    pub fn run_ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.sim.step(&self.held);
        }
    }

    /// `done`이 참이 될 때까지 한 Tick씩 진행합니다. `max_ticks` 안에 참이 되지 않으면 테스트가 실패합니다.
    /// 이 함수가 돌려주는 값은 걸린 Tick 수입니다.
    pub fn run_until(&mut self, max_ticks: u32, done: impl Fn(&Sim) -> bool) -> u32 {
        for elapsed in 0..max_ticks {
            if done(&self.sim) {
                return elapsed;
            }
            self.sim.step(&self.held);
        }
        assert!(
            done(&self.sim),
            "the condition did not hold within {max_ticks} ticks"
        );
        max_ticks
    }

    /// 이 시나리오의 Tick 속도에서 `secs`초가 몇 Tick인지 돌려줍니다.
    pub fn ticks(&self, secs: f32) -> u32 {
        self.sim.tuning().ticks(secs)
    }

    pub fn player(&self, id: PlayerId) -> &Player {
        self.sim.player(id).expect("the player exists")
    }
}

/// 부동소수점 결과를 비교할 때 쓰는 허용 오차입니다.
pub(crate) fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}
