//! 테스트에서만 쓰는 상황 구성 도구입니다. 개체를 필요한 자리에 놓은 뒤,
//! 서버가 하는 것과 같은 방식(누르고 있는 입력과 `step`)으로 게임을 진행합니다.

use crate::expedition::Phase;
use crate::ids::{MonsterId, NodeId, PlayerId, TowerId};
use crate::input::{AimDir, MoveDir, PlayerInput, PlayerInputs};
use crate::kind::MonsterKind;
use crate::kind::{StructureKind, WeaponKind};
use crate::math::{Rect, Vec2};
use crate::monster::Monster;
use crate::player::{Life, Player};
use crate::sim::Sim;
use crate::structures::{Tower, TowerState};
use crate::tuning::{GuardTuning, MapSpec, NightPlan, NightTuning, Tuning, WavePlan};
use crate::world::Tile;

pub(crate) struct Scenario {
    pub sim: Sim,
    held: PlayerInputs,
}

impl Scenario {
    /// 장애물과 채집 지점이 없는 20 x 20 맵입니다. 마차는 오른쪽 위 구석에 있어서 테스트하는 동작과 겹치지 않고, 감시탑은 없습니다.
    /// 낮이 끝나지 않으므로 무리가 저절로 나타나지 않고, 몬스터는 테스트가 `monster_at`으로 놓은 것뿐입니다.
    /// 채집 지점을 더해도 낮의 몬스터는 없습니다(`with_cycle`). 낮의 몬스터를 시험하는 테스트가 `guards.per_node`를 직접 정합니다.
    pub fn open_field() -> Scenario {
        Scenario::with_tuning(|_| {})
    }

    /// `open_field`와 같지만 수치를 먼저 바꿉니다.
    pub fn with_tuning(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
        let mut scenario = Scenario::with_cycle(adjust);
        scenario.sim.set_phase(Phase::Day {
            day: 1,
            ticks_left: u32::MAX,
        });
        scenario
    }

    /// 낮과 밤이 정해진 시간에 저절로 흐르는 시나리오입니다. `open_field`의 맵을 쓰고, 밤의 몬스터는 마차에서 12타일 떨어진 곳에 나타납니다.
    /// 낮의 몬스터는 없습니다. 원정의 진행을 확인하는 테스트가 씁니다.
    pub fn with_cycle(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
        let defaults = Tuning::default();
        Scenario::started(
            Tuning {
                map: MapSpec {
                    width: 20.0,
                    height: 20.0,
                    wagon: Rect::new(17.0, 17.0, 19.0, 19.0),
                    obstacles: vec![],
                    nodes: vec![],
                    player_spawns: vec![Vec2::new(10.0, 10.0)],
                    spatial_cell: 2.0,
                },
                nights: NightTuning {
                    spawn_distance: 12.0,
                    ..defaults.nights.clone()
                },
                guards: GuardTuning {
                    per_node: 0,
                    ..defaults.guards.clone()
                },
                ..defaults
            },
            adjust,
        )
    }

    /// 기본 맵과 기본 수치로 흐르는 시나리오입니다. 수치만 `adjust`로 바꿉니다.
    pub fn default_game(adjust: impl FnOnce(&mut Tuning)) -> Scenario {
        Scenario::started(Tuning::default(), adjust)
    }

    fn started(mut tuning: Tuning, adjust: impl FnOnce(&mut Tuning)) -> Scenario {
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

    /// 테스트가 `tower_at`으로 처음 놓은 감시탑입니다.
    pub fn tower(&self) -> &Tower {
        self.sim.towers().next().expect("the test placed a tower")
    }

    /// 타일 (`x`, `y`)에 서 있는 감시탑을 하나 놓습니다. 건설의 조건은 거치지 않으므로, 다른 규칙을 시험할 상황을 만들 때만 씁니다.
    pub fn tower_at(&mut self, x: u16, y: u16) -> TowerId {
        let kind = StructureKind::Watchtower;
        let state = Tower::standing_state(kind, self.sim.tuning());
        self.sim.world_mut().add_tower(kind, Tile::new(x, y), state)
    }

    /// 보관함의 자원을 `amount`로 맞춥니다.
    pub fn set_stash(&mut self, amount: u32) {
        self.sim.world_mut().stash = amount;
    }

    /// 채집 지점의 남은 자원을 `amount`로 맞춥니다.
    pub fn set_node_amount(&mut self, node: NodeId, amount: u32) {
        let found = self
            .sim
            .world_mut()
            .nodes
            .iter_mut()
            .find(|candidate| candidate.id == node)
            .expect("the node exists");
        found.amount = amount;
    }

    /// 플레이어가 들고 있는 자원을 `amount`로 맞춥니다. 쓰러진 플레이어에게는 아무 일도 하지 않습니다.
    pub fn set_carried(&mut self, id: PlayerId, amount: u32) {
        if let Life::Alive { carried, .. } = &mut self.player_mut(id).life {
            *carried = amount;
        }
    }

    /// `(x, y)`에 자원 `amount`가 떨어진 더미를 놓습니다.
    pub fn pile_at(&mut self, x: f32, y: f32, amount: u32) {
        self.sim.world_mut().add_pile(Vec2::new(x, y), amount);
    }

    /// 처음 놓은 감시탑의 HP를 바꿉니다. 쏘는 동안의 쿨다운과 마지막 사격은 그대로 둡니다.
    pub fn set_tower_hp(&mut self, hp: u32) {
        let tower = self
            .sim
            .world_mut()
            .towers
            .values_mut()
            .next()
            .expect("the test placed a tower");
        if let TowerState::Standing { hp: current, .. } = &mut tower.state {
            *current = hp;
        }
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

    /// 플레이어의 달리기 게이지를 모두 비웁니다. 쓰러졌거나 사망한 플레이어에게는 아무 일도 하지 않습니다.
    pub fn drain_stamina(&mut self, id: PlayerId) {
        let t = self.sim.tuning().clone();
        if let Life::Alive { stamina, .. } = &mut self.player_mut(id).life {
            while stamina.can_spend(&t) {
                stamina.spend(&t);
            }
        }
    }

    pub fn kill(&mut self, id: PlayerId) {
        self.player_mut(id).life = Life::Dead;
    }

    /// 플레이어가 드는 무기를 `kind`로 바꿉니다. 무기는 생명 상태와 따로 있으므로 쓰러졌거나 사망한 플레이어도 바뀝니다.
    /// 테스트가 「활을 든 사수」라는 상황을 만들 때 씁니다.
    pub fn give_weapon(&mut self, id: PlayerId, kind: WeaponKind) {
        self.player_mut(id).weapon = kind;
    }

    /// 지금 누르고 있는 입력은 그대로 두고, 무기 전환 키(Q)를 한 Tick 동안 눌렀다 뗍니다. 이 함수가 한 Tick을 진행합니다.
    ///
    /// `hold`는 입력을 매 Tick 되풀이하므로 `hold`로는 「한 번 누름」을 만들 수 없습니다. 서버가 하는 일과 같이 이 Tick의 입력에만 참을 넣습니다.
    pub fn tap_switch_key(&mut self, id: PlayerId) {
        let mut inputs = self.held.clone();
        inputs.entry(id).or_default().switch_weapon = true;
        self.sim.step(&inputs);
    }

    /// 날아가는 중인 탄환의 수입니다. 이번 Tick에 끝난 탄환은 세지 않습니다.
    pub fn flying(&self) -> usize {
        self.sim
            .projectiles()
            .filter(|projectile| projectile.is_flying())
            .count()
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

    /// 상호작용 키(F)를 누른 채로 서 있습니다.
    pub fn hold_interact(&mut self, id: PlayerId) {
        self.hold(
            id,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
    }

    /// 건설 키를 누른 채로 `(x, y)` 방향을 조준합니다. 건설할 타일은 서 있는 타일에서 그 방향으로 이웃한 타일입니다.
    pub fn hold_build(&mut self, id: PlayerId, x: f32, y: f32) {
        self.hold(
            id,
            PlayerInput {
                build: true,
                aim: AimDir::new(x, y),
                ..PlayerInput::default()
            },
        );
    }

    /// `(x, y)` 방향으로 이동하면서 그 방향을 조준하고, 달리기 키(Shift)를 누르고 있습니다.
    pub fn hold_sprint(&mut self, id: PlayerId, x: f32, y: f32) {
        self.hold(
            id,
            PlayerInput {
                move_dir: MoveDir::new(x, y),
                aim: AimDir::new(x, y),
                sprint: true,
                ..PlayerInput::default()
            },
        );
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
    pub fn run_until(&mut self, max_ticks: u32, mut done: impl FnMut(&Sim) -> bool) -> u32 {
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

/// 자원이 있는 모든 곳의 합입니다. 보관함, 플레이어가 든 자원, 떨어진 자원, 짓는 중인 감시탑에 낸 자원, 서 있는 감시탑을 짓는 데 쓴 자원을 더합니다.
/// 서 있는 감시탑은 하나당 건설 비용이 들었다고 셉니다(파괴된 감시탑이 없고 수리가 없는 상황에서만 맞습니다. 수리에 쓴 자원은 사라집니다).
pub(crate) fn resource_total(sim: &Sim) -> u32 {
    let t = sim.tuning();
    let carried: u32 = sim.players().map(Player::carried).sum();
    let piles: u32 = sim.piles().map(|pile| pile.amount).sum();
    let in_towers: u32 = sim
        .towers()
        .map(|tower| match tower.state {
            TowerState::Building { paid, .. } => paid,
            TowerState::Standing { .. } => t.structures.get(tower.kind).build_cost,
        })
        .sum();
    sim.stash() + carried + piles + in_towers
}

/// 채집 지점에서 지금까지 옮겨진 자원의 양입니다. 채집 지점이 가득 찼을 때 줄어든 만큼입니다.
pub(crate) fn gathered_total(sim: &Sim) -> u32 {
    let full = sim.tuning().resources.node_amount;
    sim.nodes().map(|node| full - node.amount).sum()
}

/// 부동소수점 결과를 비교할 때 쓰는 허용 오차입니다.
pub(crate) fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// 밤마다 무리의 몬스터 수를 적은 목록에서 밤의 구성을 만듭니다. 모든 몬스터는 `Grunt`입니다.
pub(crate) fn nights(counts: &[&[u32]]) -> Vec<NightPlan> {
    counts
        .iter()
        .map(|waves| NightPlan {
            waves: waves
                .iter()
                .map(|&count| WavePlan {
                    kind: MonsterKind::Grunt,
                    count,
                })
                .collect(),
        })
        .collect()
}
