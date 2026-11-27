//! `Sim`은 방 하나의 게임 상태 전체이며, 규칙 크레이트의 공개 진입점입니다.

use crate::build;
use crate::camp::Camp;
use crate::debrief::{Debrief, Report};
use crate::effect;
use crate::expedition::{self, Outcome, Phase};
use crate::ids::{PlayerId, Team};
use crate::input::PlayerInputs;
use crate::loot::{self, Chest, GroundWeapon, Prompt, Rack, Seed};
use crate::math::Vec2;
use crate::monster::{self, Monster, TileFields};
use crate::nav::Paths;
use crate::player::{self, Interaction, Player, Presence};
use crate::projectile::{self, Projectile};
use crate::resources::{self, GatherNode, Pile};
use crate::spatial::MonsterGrid;
use crate::spawn::SpawnPlan;
use crate::stage::Stage;
use crate::structures::{self, Tower, Wagon};
use crate::tuning::{MapSpec, Start, Tuning};
use crate::weapon;
use crate::world::{Ctx, Layout, World};

/// 방 하나의 게임입니다. 바깥 코드는 `&Sim`으로 상태를 읽고,
/// `add_player`, `remove_player`, `step`으로만 상태를 바꿉니다.
pub struct Sim {
    tuning: Tuning,
    /// 이 방의 씨앗입니다. 원정마다 이 씨앗에서 이어 간 씨앗(`expedition_seed`)으로 무작위를 씁니다. 방이 만들어진 뒤 바뀌지 않습니다.
    seed: Seed,
    /// 방이 만들어진 뒤 실행한 Tick의 수입니다. 줄어들지 않으므로 스냅샷의 Tick 번호는 계속 커집니다.
    tick: u64,
    /// 이 방에서 시작한 원정의 수입니다. 첫 원정을 시작하기 전의 야영지에서는 0이고, 원정을 시작할 때마다 1씩 늘어납니다.
    expedition: u32,
    /// 마지막으로 시작한 원정이 시작된 Tick입니다. 결과의 「걸린 시간」은 원정이 끝난 Tick에서 이 값을 뺀 것입니다.
    expedition_began_at: u64,
    stage: Stage,
    world: World,
    /// 마차와 포탑으로 가는 공유 이동 경로입니다. 세계의 배치가 바뀌면 다음 `sync`가 다시 만듭니다.
    paths: Paths,
    /// 몬스터의 위치를 담은 격자입니다. Tick마다 다시 만듭니다.
    monster_grid: MonsterGrid,
    /// 쫓기는 플레이어가 서 있는 타일을 향하는 이동 경로입니다.
    tile_fields: TileFields,
    /// 밤의 몬스터가 나타나는 자리입니다. 지형으로만 정해지므로 방을 만들 때 한 번 정합니다.
    spawns: SpawnPlan,
    next_player: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinError {
    RoomFull { capacity: usize },
}

impl Sim {
    /// 플레이어가 없는 방을 야영지로 만듭니다. `CampTuning::start`가 `Start::AtOnce`이면 야영지 없이 첫 원정의 첫날을 시작합니다.
    /// 무작위는 `seed`에서 정해지므로, 같은 수치와 씨앗과 입력은 같은 결과를 냅니다.
    /// 수치는 `Tuning::validate`를 통과한 것이어야 합니다. 전제가 어긋난 수치는 개체가 벽을 통과하는 것 같은 오동작을 만들고,
    /// 몬스터가 나타날 자리가 없는 수치에서는 이 함수가 멈춥니다.
    pub fn new(tuning: Tuning, seed: Seed) -> Sim {
        let world = World::new(&tuning, Layout::Camp);
        // 몬스터가 서는 자리는 원정의 첫날의 타일 지도(채집 지점과 시작 감시탑이 있음)에서 정합니다.
        let spawns = SpawnPlan::new(&World::new(&tuning, Layout::Expedition).tiles, &tuning)
            .expect("a validated tuning has a spot where night monsters appear");
        let mut sim = Sim {
            seed,
            expedition: 0,
            expedition_began_at: 0,
            stage: Stage::Camp(Camp::new()),
            paths: Paths::new(&world, &tuning),
            monster_grid: MonsterGrid::new(world.tiles.bounds(), tuning.map.spatial_cell),
            tile_fields: TileFields::new(),
            spawns,
            world,
            tick: 0,
            next_player: 1,
            tuning,
        };
        if sim.tuning.camp.start == Start::AtOnce {
            sim.begin_expedition();
        }
        sim
    }

    /// 새 플레이어를 최대 HP의 살아 있는 상태로 등장 지점에 추가하고 번호를 돌려줍니다.
    /// 등장 지점은 방에 있는 플레이어가 가장 적게 쓰는 지점 중 가장 앞의 것으로 정하므로, 먼저 나간 플레이어의 자리를 새 플레이어가 이어받습니다.
    /// 원정이 진행되는 도중에도 곧바로 참가하고, 야영지에서는 준비하지 않은 상태로, 결과 화면에서는 확인하지 않은 상태로 들어옵니다.
    /// 방이 가득 차 있으면 거절합니다.
    pub fn add_player(&mut self) -> Result<PlayerId, JoinError> {
        if self.world.players.len() >= self.tuning.max_players {
            return Err(JoinError::RoomFull {
                capacity: self.tuning.max_players,
            });
        }
        let id = PlayerId(self.next_player);
        self.next_player += 1;
        let slot = choose_spawn_slot(
            self.tuning.map.player_spawns.len(),
            None,
            self.world.players.values().map(|p| p.spawn_slot),
        );
        let spawn = self.clear_spot_near(spawn_point(&self.tuning.map, slot));
        let team = self.team_for_new_player();
        let player = Player::new(id, team, slot, spawn, &self.tuning);
        self.world.players.insert(id, player);
        if let Stage::Camp(camp) = &mut self.stage {
            camp.settle(&self.world);
        }
        Ok(id)
    }

    /// 플레이어의 몸이 막힌 타일에 걸치지 않고 설 수 있는 자리입니다. `wanted`에 설 수 있으면 `wanted`이고,
    /// 원정 도중에 그 자리에 포탑이 세워졌으면 가장 가까운 빈 타일의 중심입니다.
    fn clear_spot_near(&self, wanted: Vec2) -> Vec2 {
        let radius = self.tuning.player.radius;
        let mut near = Vec::new();
        self.world
            .tiles
            .blocked_rects_near(wanted, radius, &mut near);
        if near.iter().all(|solid| solid.distance_to(wanted) >= radius) {
            return wanted;
        }
        self.world
            .tiles
            .nearest_free(wanted)
            .map_or(wanted, |tile| tile.center())
    }

    /// 플레이어를 즉시 제거합니다. 없는 번호는 무시하므로 같은 퇴장을 두 번 처리해도 안전합니다.
    /// 원정 중이면 이 플레이어를 부활시키던 플레이어는 다음 Tick에 대상이 사라진 것을 보고 부활을 멈추고, 이 플레이어가 들고 있던 자원은 그 자리에 떨어집니다.
    /// 가진 무기도 그 자리에 떨어지지만, 시작 무기와 같은 종류의 무기는 떨어지지 않습니다.
    /// 야영지와 결과 화면에서는 아무것도 떨어지지 않습니다. 이 단계의 것은 다음 단계로 이어지지 않기 때문입니다.
    pub fn remove_player(&mut self, id: PlayerId) {
        match &mut self.stage {
            Stage::Expedition(_) => self.world.remove_player(id, &self.tuning),
            Stage::Camp(camp) => {
                self.world.players.remove(&id);
                camp.settle(&self.world);
            }
            Stage::Debrief(debrief) => {
                debrief.forget(id);
                self.world.players.remove(&id);
            }
        }
    }

    /// 플레이어가 자리에 있는지 알립니다. 규칙은 접속을 알지 못하므로, 접속이 끊기거나 입력이 멈추거나 야영지에서 조작이 없는 것을
    /// 서버가 `Away`로 알려 줍니다. 자리를 비운 플레이어는 몬스터가 노리지 않고, 피해를 받지 않고, 쓰러져 있으면 출혈이 멈추고,
    /// 실패의 판정과 야영지의 「모두 준비」와 결과 화면의 「모두 확인」에서 빠집니다. 없는 번호는 무시합니다.
    /// 야영지에서는 카운트다운을 그 자리에서 다시 정합니다. 준비하지 않은 사람이 돌아오면 세던 시간이 취소됩니다.
    pub fn set_presence(&mut self, id: PlayerId, presence: Presence) {
        let Some(player) = self.world.players.get_mut(&id) else {
            return;
        };
        player.presence = presence;
        if let Stage::Camp(camp) = &mut self.stage {
            camp.settle(&self.world);
        }
    }

    /// 야영지에서 플레이어의 준비를 끕니다. 야영지가 아니거나 준비하지 않은 플레이어이면 아무 일도 하지 않습니다.
    /// 야영지에서 조작이 없어 자리를 비운 사람은 돌아올 때 준비하지 않은 상태여야 하므로, 서버가 그 사람을 `Away`로 알린 뒤에 부릅니다.
    pub fn withdraw_ready(&mut self, id: PlayerId) {
        if let Stage::Camp(camp) = &mut self.stage {
            camp.withdraw(id, &self.world);
        }
    }

    /// 게임을 Tick 하나만큼 진행합니다. `inputs`는 이번 Tick의 플레이어별 입력이고,
    /// 목록에 없는 플레이어는 아무것도 누르지 않은 것으로 봅니다. 단계마다 하는 일이 다릅니다.
    pub fn step(&mut self, inputs: &PlayerInputs) {
        self.tick += 1;
        match &self.stage {
            Stage::Camp(_) => self.practice(inputs),
            Stage::Expedition(_) => self.simulate(inputs),
            Stage::Debrief(_) => self.wait_for_confirmations(inputs),
        }
    }

    /// 야영지의 Tick입니다. 준비와 카운트다운을 먼저 정하고, 원정이 시작되지 않으면 플레이어가 움직이고 무기를 써 봅니다.
    /// 야영지에서는 아무것도 피해를 받지 않습니다. 공격이 허수아비에 닿은 피해는 화면에 보여 주려고 적기만 합니다.
    /// 몬스터(허수아비)와 포탑은 행동하지 않고, 건설은 거절됩니다.
    fn practice(&mut self, inputs: &PlayerInputs) {
        let Stage::Camp(camp) = &mut self.stage else {
            return;
        };
        if camp.advance(&self.world, inputs, &self.tuning) {
            self.begin_expedition();
            return;
        }
        self.monster_grid
            .rebuild(&self.world.monsters, &self.tuning.monsters);
        let ctx = Ctx {
            t: &self.tuning,
            inputs,
            grid: &self.monster_grid,
            tick: self.tick,
            practice: true,
        };
        let mut hits = Vec::new();
        player::update_interactions(&mut self.world, &ctx);
        player::update_sprint(&mut self.world, &ctx);
        player::move_players(&mut self.world, &ctx);
        weapon::use_weapons(&mut self.world, &ctx, &mut hits);
        projectile::advance_projectiles(&mut self.world, &ctx, &mut hits);
        // 허수아비는 피해를 받지 않으므로 출혈이 낸 피해는 버립니다. 효과가 걸리고 시간이 흐르는 것은 원정과 같습니다.
        effect::advance_effects(&mut self.world, &ctx, &mut Vec::new());
        effect::apply_debuffs(&mut self.world, &hits, &self.tuning);
        if let Stage::Camp(camp) = &mut self.stage {
            camp.note_strikes(&hits, self.tick);
        }
    }

    /// 결과 화면의 Tick입니다. 세계는 멈춰 있고, 확인 키와 남은 시간만 봅니다.
    /// 날던 탄환은 그 자리에 멈추고, 끝난 탄환은 스냅샷에 한 번 실렸으므로 지웁니다.
    fn wait_for_confirmations(&mut self, inputs: &PlayerInputs) {
        projectile::remove_landed(&mut self.world);
        let Stage::Debrief(debrief) = &mut self.stage else {
            return;
        };
        if debrief.advance(&self.world, inputs) {
            match self.tuning.camp.start {
                Start::WhenReady => self.enter_camp(),
                Start::AtOnce => self.begin_expedition(),
            }
        }
    }

    /// 원정의 Tick입니다. 모든 공격은 같은 상태를 보고 결정한 뒤 한꺼번에 적용합니다. 그래서 같은 Tick에 죽는 몬스터도 자기 공격은 마칩니다.
    fn simulate(&mut self, inputs: &PlayerInputs) {
        let expedition_seed = self.expedition_seed();
        let Stage::Expedition(phase) = &mut self.stage else {
            return;
        };
        expedition::advance_phase(
            phase,
            &mut self.world,
            &self.spawns,
            expedition_seed,
            &self.tuning,
        );

        self.paths.sync(&self.world, &self.tuning);
        self.monster_grid
            .rebuild(&self.world.monsters, &self.tuning.monsters);
        let ctx = Ctx {
            t: &self.tuning,
            inputs,
            grid: &self.monster_grid,
            tick: self.tick,
            practice: false,
        };
        let mut hits = Vec::new();
        let mut afflictions = Vec::new();
        build::cycle_kinds(&mut self.world, &ctx);
        player::update_interactions(&mut self.world, &ctx);
        structures::advance_construction(&mut self.world, &ctx);
        self.paths.sync(&self.world, &self.tuning);
        player::update_sprint(&mut self.world, &ctx);
        player::move_players(&mut self.world, &ctx);
        resources::deposit_and_pickup(&mut self.world, &ctx);
        weapon::use_weapons(&mut self.world, &ctx, &mut hits);
        projectile::advance_projectiles(&mut self.world, &ctx, &mut hits);
        structures::towers_act(&mut self.world, &ctx, &mut hits, &mut afflictions);
        monster::monsters_act(
            &mut self.world,
            &ctx,
            &self.paths,
            &mut self.tile_fields,
            &mut hits,
        );
        effect::advance_effects(&mut self.world, &ctx, &mut hits);
        self.world.apply_hits(&hits, &self.tuning);
        effect::apply_afflictions(&mut self.world, &afflictions, &self.tuning);
        player::recover_near_wagon(&mut self.world, &ctx);
        player::advance_life_timers(&mut self.world, &self.tuning);
        loot::finish_openings(&mut self.world, &self.tuning);
        structures::hold_auras(&mut self.world, &self.tuning);
        let Stage::Expedition(phase) = &mut self.stage else {
            return;
        };
        if let Some(outcome) = expedition::settle_outcome(phase, &self.world, &self.tuning) {
            let day = expedition::day_of(*phase);
            self.conclude(outcome, day);
        }
    }

    /// 새 원정을 시작합니다. 이전 원정과 야영지에서 이어받는 것은 방에 있는 플레이어뿐입니다.
    /// 모든 플레이어는 시작 무기와 최대 HP의 살아 있는 상태로 등장 지점에 돌아가고, 사망 상태도 초기화됩니다.
    /// 원정의 씨앗은 방의 씨앗과 원정의 번호로만 정해지므로, 야영지에 머문 시간은 원정이 뽑는 것을 바꾸지 않습니다.
    pub(crate) fn begin_expedition(&mut self) {
        self.expedition += 1;
        self.expedition_began_at = self.tick;
        self.world.restart(&self.tuning, Layout::Expedition);
        self.return_players_to_spawns();
        let expedition_seed = self.expedition_seed();
        expedition::begin_day(
            &mut self.world,
            &self.spawns,
            expedition_seed,
            1,
            &self.tuning,
        );
        self.stage = Stage::Expedition(expedition::opening_phase(&self.tuning));
    }

    /// 원정을 끝내고 결과 화면을 엽니다. 세계는 끝난 모습 그대로 멈춥니다. 이때 방에 있는 플레이어의 기록이 결과로 복사됩니다.
    fn conclude(&mut self, outcome: Outcome, day: u32) {
        let report = Report {
            outcome,
            day,
            elapsed_ticks: self.tick - self.expedition_began_at,
            tower_kills: self.world.tower_kills,
            rows: self
                .world
                .players
                .values()
                .map(|player| (player.id, player.record))
                .collect(),
        };
        self.stage = Stage::Debrief(Debrief::new(report, &self.tuning));
    }

    /// 결과 화면이면 끝난 원정의 결과입니다.
    pub fn debrief(&self) -> Option<&Debrief> {
        match &self.stage {
            Stage::Debrief(debrief) => Some(debrief),
            Stage::Camp(_) | Stage::Expedition(_) => None,
        }
    }

    /// 야영지로 돌아갑니다. 모든 플레이어는 시작 무기와 최대 HP로 등장 지점에 섭니다.
    fn enter_camp(&mut self) {
        self.world.restart(&self.tuning, Layout::Camp);
        self.return_players_to_spawns();
        self.stage = Stage::Camp(Camp::new());
    }

    /// 모든 플레이어를 시작 무기와 최대 HP의 살아 있는 상태로 자기 등장 지점에 돌려놓습니다.
    fn return_players_to_spawns(&mut self) {
        // 나간 플레이어가 있으면 한 지점에 둘이 몰릴 수 있으므로, 돌려놓을 때마다 지점을 다시 정합니다.
        // 겹치지 않는 플레이어는 자기 지점에 그대로 돌아갑니다.
        let mut assigned: Vec<usize> = Vec::with_capacity(self.world.players.len());
        for player in self.world.players.values_mut() {
            let slot = choose_spawn_slot(
                self.tuning.map.player_spawns.len(),
                Some(player.spawn_slot),
                assigned.iter().copied(),
            );
            assigned.push(slot);
            player.reset(slot, spawn_point(&self.tuning.map, slot), &self.tuning);
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

    /// 이 방에서 시작한 원정의 수입니다. 원정 중에는 몇 번째 원정인지이고, 첫 원정이 1입니다. 첫 원정 전의 야영지에서는 0입니다.
    pub fn expedition_number(&self) -> u32 {
        self.expedition
    }

    /// 방이 만들어질 때 받은 씨앗입니다.
    pub fn seed(&self) -> Seed {
        self.seed
    }

    /// 지금 원정의 씨앗입니다.
    pub fn expedition_seed(&self) -> Seed {
        seed_of_expedition(self.seed, self.expedition)
    }

    /// 방이 지금 무엇을 하는지입니다.
    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    /// 원정 중이면 원정의 단계입니다. 야영지와 결과 화면에서는 없습니다.
    pub fn phase(&self) -> Option<Phase> {
        match &self.stage {
            Stage::Expedition(phase) => Some(*phase),
            Stage::Camp(_) | Stage::Debrief(_) => None,
        }
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

    /// 번호가 작은 탄환부터 차례로 돌려줍니다. 이번 Tick에 끝난 탄환도 포함합니다(`Projectile::flight`).
    /// 끝난 탄환을 포함하는 까닭은 클라이언트가 탄환이 끝난 자리까지 그릴 수 있게 하려는 것입니다.
    pub fn projectiles(&self) -> impl Iterator<Item = &Projectile> {
        self.world.projectiles.iter()
    }

    /// 번호가 작은 시설부터 차례로 돌려줍니다. 짓는 중인 포탑도 포함합니다.
    pub fn towers(&self) -> impl Iterator<Item = &Tower> {
        self.world.towers.values()
    }

    pub fn wagon(&self) -> &Wagon {
        &self.world.wagon
    }

    /// 번호가 작은 채집 지점부터 차례로 돌려줍니다.
    pub fn nodes(&self) -> impl Iterator<Item = &GatherNode> {
        self.world.nodes.iter()
    }

    /// 지금 놓여 있는 무기 상자를 번호가 작은 상자부터 차례로 돌려줍니다. 낮에만 있습니다.
    pub fn chests(&self) -> impl Iterator<Item = &Chest> {
        self.world.chests.iter()
    }

    /// 바닥에 놓인 무기를 번호가 작은 무기부터 차례로 돌려줍니다.
    pub fn ground_weapons(&self) -> impl Iterator<Item = &GroundWeapon> {
        self.world.ground_weapons.iter()
    }

    /// 야영지의 무기 진열대입니다. 야영지에만 있습니다.
    pub fn racks(&self) -> impl Iterator<Item = &Rack> {
        self.world.racks.iter()
    }

    /// `id`가 지금 F를 누르면 하게 되는 일이 화면에 안내할 만한 것이면 그 안내입니다. 부활, 수리, 채집은 안내하지 않습니다.
    /// `F`의 우선순위를 고르는 함수(`player::choose_interaction`)가 정하므로 서버가 실제로 하는 일과 어긋나지 않고, 따로 저장하는 값이 없습니다.
    pub fn prompt(&self, id: PlayerId) -> Option<Prompt> {
        let player = self.player(id)?;
        player::prompt(&self.world, player, &self.tuning)
    }

    /// 땅에 떨어진 자원을 번호가 작은 더미부터 차례로 돌려줍니다.
    pub fn piles(&self) -> impl Iterator<Item = &Pile> {
        self.world.piles.iter()
    }

    /// 마차의 보관함에 있는 자원입니다. 모든 플레이어가 함께 씁니다.
    pub fn stash(&self) -> u32 {
        self.world.stash
    }

    /// `id`가 수리하는 중인데 보관함의 자원이 수리의 비용보다 적어서 다음 수리가 회복시키지 못하면 참입니다.
    /// 이벤트가 아니라 상태이고, 수리하는 대상(`Interaction`)과 보관함에서 그때그때 구하므로 따로 저장하는 값이 없고 둘과 어긋날 수 없습니다.
    pub fn lacks_stash_to_repair(&self, id: PlayerId) -> bool {
        let Some(Interaction::Repairing { target, .. }) =
            self.player(id).and_then(Player::interaction)
        else {
            return false;
        };
        player::repair_spec(&self.world, *target, &self.tuning)
            .is_some_and(|spec| self.world.stash < spec.cost_per_pulse)
    }

    /// `opener`가 상자를 열고 있으면 열기의 진행도(0.0 이상 1.0 이하)를 돌려줍니다.
    /// 진행도는 여는 플레이어의 `Interaction`에서 계산하며, 상자는 아무 값도 저장하지 않습니다.
    pub fn open_progress(&self, opener: PlayerId) -> Option<f32> {
        let required = self.tuning.ticks(self.tuning.loot.open_secs);
        match self.world.players.get(&opener)?.interaction() {
            Some(Interaction::Opening { elapsed_ticks, .. }) => {
                Some((*elapsed_ticks as f32 / required as f32).min(1.0))
            }
            _ => None,
        }
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

/// 등장 지점 `point_count`개 가운데 `used`가 가장 적게 쓰고 있는 지점의 순서를 고릅니다.
/// 같은 횟수이면 `preferred`를 먼저, 그다음 앞선 순서를 고릅니다. 그래서 플레이어가 지점보다 많아도 지점마다 고르게 나뉩니다.
fn choose_spawn_slot(
    point_count: usize,
    preferred: Option<usize>,
    used: impl Iterator<Item = usize>,
) -> usize {
    let point_count = point_count.max(1);
    let mut uses = vec![0usize; point_count];
    for slot in used {
        uses[slot % point_count] += 1;
    }
    let fewest = uses.iter().copied().min().unwrap_or(0);
    preferred
        .map(|slot| slot % point_count)
        .filter(|slot| uses[*slot] == fewest)
        .or_else(|| uses.iter().position(|&n| n == fewest))
        .unwrap_or(0)
}

/// 등장 순서가 `slot`인 플레이어가 나타날 위치입니다. 순서가 등장 지점의 수를 넘으면 처음부터 다시 돌아가며 씁니다.
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

    /// 원정 중인 방의 단계를 바꿉니다. 원정 중이 아니면 테스트가 잘못 만든 것이므로 멈춥니다.
    pub(crate) fn set_phase(&mut self, phase: Phase) {
        assert!(
            matches!(self.stage, Stage::Expedition(_)),
            "set_phase needs an expedition"
        );
        self.stage = Stage::Expedition(phase);
    }

    /// 원정을 `outcome`으로 끝냅니다. 원정의 끝을 시험하는 테스트가 씁니다.
    pub(crate) fn end_expedition(&mut self, outcome: Outcome) {
        let day = self.phase().map_or(1, expedition::day_of);
        self.conclude(outcome, day);
    }

    pub(crate) fn spawn_monster(&mut self, pos: Vec2) -> crate::ids::MonsterId {
        let kind = crate::kind::MonsterKind::Grunt;
        self.world
            .spawn_monster(kind, crate::monster::Order::Raid, pos, &self.tuning)
    }
}

/// 방의 `expedition`번째 원정의 씨앗입니다. 방의 씨앗에서 원정의 번호로 이어 간 씨앗이라서, 원정마다 다르고 같은 방의 같은 원정은 언제나 같습니다.
fn seed_of_expedition(room: Seed, expedition: u32) -> Seed {
    room.derive(u64::from(expedition))
}
