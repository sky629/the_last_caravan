//! 게임의 모든 수치를 이 파일 한 곳에 둡니다.
//!
//! 시간은 초, 속도는 초당 타일 수로 적습니다. 그래서 `tick_hz`를 바꿔도 게임의 속도감이 유지됩니다.
//! 초를 Tick으로 바꾸는 곳은 `Tuning::ticks` 하나이고, 게임 상태에는 정수 Tick만 저장합니다.
//! 몬스터와 시설의 수치는 종류별 표(`MonsterTable`, `StructureTable`)에 있습니다.
//!
//! 서버는 `Tuning` 하나를 가지고 있고, 방을 만들 때마다 복사해서 넘깁니다.
//! 수치를 바꾼 `Tuning`은 `validate`로 검사할 수 있습니다.

use std::collections::BTreeSet;

use crate::kind::{MonsterKind, StructureKind};
use crate::math::{Rect, Vec2};
use crate::spawn::SpawnPlan;
use crate::world::{Tile, TileMap, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    /// 서버가 1초에 실행하는 Tick의 수입니다. Tick 속도는 이 값 하나로만 정합니다.
    pub tick_hz: u32,
    /// 한 방에 들어올 수 있는 최대 인원입니다. `Sim::add_player`만 이 값을 읽습니다.
    pub max_players: usize,
    pub player: PlayerTuning,
    pub revive: ReviveTuning,
    pub monsters: MonsterTable,
    pub wagon: WagonTuning,
    pub structures: StructureTable,
    pub village: VillageTuning,
    pub resources: ResourceTuning,
    pub guards: GuardTuning,
    pub cycle: CycleTuning,
    pub nights: NightTuning,
    pub map: MapSpec,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerTuning {
    pub max_hp: u32,
    pub radius: f32,
    pub speed: f32,
    /// 쓰러진 상태에서 기어가는 속도입니다.
    pub crawl_speed: f32,
    pub attack_damage: u32,
    /// 플레이어의 가장자리에서 몬스터의 가장자리까지 잰 근접 공격 거리입니다.
    pub attack_reach: f32,
    /// 조준 방향을 기준으로 한 공격 부채꼴의 절반 각도입니다(도). 부채꼴 안의 적을 모두 공격합니다.
    pub attack_half_angle_deg: f32,
    pub attack_cooldown_secs: f32,
}

impl PlayerTuning {
    /// 공격 부채꼴의 경계 각도에 대한 코사인입니다. 몬스터 방향과 조준 방향의 내적이 이 값 이상이면 부채꼴 안입니다.
    /// 규칙에서 삼각함수를 쓰는 곳은 이 함수 하나입니다.
    pub fn attack_arc_cos(&self) -> f32 {
        self.attack_half_angle_deg.to_radians().cos()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReviveTuning {
    /// 쓰러진 플레이어가 사망 상태가 되기까지 걸리는 시간입니다.
    pub bleed_secs: f32,
    /// 부활에 필요한 시간입니다.
    pub revive_secs: f32,
    /// 부활 직후의 HP입니다. 최대 HP에 대한 백분율입니다.
    pub revived_hp_percent: u32,
    /// 부활시키는 플레이어와 쓰러진 플레이어의 중심 사이 거리 한계입니다.
    pub range: f32,
}

/// 몬스터 한 종류의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterTuning {
    pub max_hp: u32,
    pub radius: f32,
    pub speed: f32,
    pub attack_damage: u32,
    /// 몬스터의 가장자리에서 대상의 가장자리까지 잰 근접 공격 거리입니다.
    pub attack_reach: f32,
    pub attack_cooldown_secs: f32,
    /// 마차로 가는 도중에 이 거리 안에 들어온 플레이어나 감시탑을 쫓습니다.
    /// 몬스터의 중심에서 대상의 가장자리까지 잰 거리입니다.
    pub aggro_range: f32,
    /// 몬스터끼리 겹쳤을 때 서로 밀려나는 최대 속도입니다. 걷는 속도와 더한 값으로 한 Tick에 움직이는 거리가
    /// 반지름보다 짧아야 합니다 (`Tuning::validate`가 검사합니다).
    pub separation_speed: f32,
}

/// 몬스터 종류별 수치표입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterTable {
    pub grunt: MonsterTuning,
}

impl MonsterTable {
    pub fn get(&self, kind: MonsterKind) -> &MonsterTuning {
        match kind {
            MonsterKind::Grunt => &self.grunt,
        }
    }

    /// 모든 종류 가운데 가장 큰 반지름입니다. 이동 경로 격자는 이 크기를 기준으로 막힌 칸을 정합니다.
    pub fn largest_radius(&self) -> f32 {
        MonsterKind::ALL
            .iter()
            .map(|kind| self.get(*kind).radius)
            .fold(0.0, f32::max)
    }

    /// 몬스터의 중심에서 대상의 가장자리까지 잰 공격 거리(반지름 더하기 공격 거리) 가운데 가장 긴 값입니다.
    /// 이동 경로의 도착 칸을 정할 때 씁니다.
    pub fn longest_engage_distance(&self) -> f32 {
        MonsterKind::ALL
            .iter()
            .map(|kind| self.get(*kind).radius + self.get(*kind).attack_reach)
            .fold(0.0, f32::max)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WagonTuning {
    pub max_hp: u32,
    /// 원정이 시작될 때 마차의 보관함에 있는 자원의 양입니다.
    pub start_stash: u32,
}

/// 마차와 그 둘레에 세우는 시설, 곧 마을의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct VillageTuning {
    /// 마차의 가장자리에서 건설할 타일의 중심까지 잰, 건설할 수 있는 거리입니다.
    pub build_range: f32,
    /// 서 있는 감시탑과 짓는 중인 감시탑을 합친 수의 상한입니다.
    pub tower_limit: u32,
}

/// 자원의 수치입니다. 자원은 한 종류이고, 채집 지점에서 플레이어의 손으로, 손에서 마차의 보관함으로 옮겨집니다.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceTuning {
    /// 채집 지점 하나가 낮이 시작될 때마다 가지는 자원의 양입니다.
    pub node_amount: u32,
    /// 플레이어가 한 번에 들 수 있는 자원의 양입니다.
    pub carry_capacity: u32,
    /// 채집은 「일정한 시간마다 자원 일정량」으로 정합니다. 그래서 Tick 속도와 관계없이 자원이 정수로 남습니다.
    pub gather_per_pulse: u32,
    pub gather_pulse_secs: f32,
    /// 플레이어의 가장자리에서 채집 지점의 가장자리까지 잰 채집 거리입니다.
    pub gather_reach: f32,
    /// 플레이어의 가장자리에서 마차의 가장자리까지 잰, 든 자원이 보관함으로 옮겨지는 거리입니다.
    pub deposit_reach: f32,
    /// 플레이어의 가장자리에서 떨어진 자원의 중심까지 잰, 자원을 줍는 거리입니다.
    pub pickup_reach: f32,
}

/// 낮에 채집 지점을 지키는 몬스터의 수치입니다. 이 몬스터들은 낮이 시작될 때마다 놓이고, 밤이 되면 마차를 향합니다.
#[derive(Clone, Debug, PartialEq)]
pub struct GuardTuning {
    pub kind: MonsterKind,
    /// 채집 지점 하나를 지키는 몬스터의 수입니다. 지점의 이웃한 빈 타일 하나에 한 마리씩 섭니다.
    pub per_node: u32,
    /// 지키는 자리의 중심에서 플레이어의 중심까지 잰, 플레이어를 쫓기 시작하는 거리입니다.
    /// 몬스터가 아니라 자리에서 재므로, 몬스터가 쫓다가 자리에서 이 거리를 넘으면 돌아갑니다.
    pub chase_range: f32,
}

/// 시설 한 종류의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureTuning {
    pub max_hp: u32,
    /// 시설의 중심에서 몬스터의 중심까지 잰 사격 거리입니다.
    pub range: f32,
    pub damage: u32,
    pub cooldown_secs: f32,
    /// 수리는 「일정한 시간마다 HP 일정량」으로 정합니다. 그래서 Tick 속도와 관계없이 HP가 정수로 남습니다.
    pub repair_hp_per_pulse: u32,
    pub repair_pulse_secs: f32,
    /// 플레이어의 가장자리에서 시설의 가장자리까지 잰 수리 거리입니다.
    pub repair_range: f32,
    /// 건설을 시작할 때 보관함에서 내는 자원의 양입니다.
    pub build_cost: u32,
    /// 건설 키를 끊지 않고 눌러야 하는 시간입니다.
    pub build_secs: f32,
}

/// 시설 종류별 수치표입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureTable {
    pub watchtower: StructureTuning,
}

impl StructureTable {
    pub fn get(&self, kind: StructureKind) -> &StructureTuning {
        match kind {
            StructureKind::Watchtower => &self.watchtower,
        }
    }
}

/// 원정의 날이 흐르는 시간입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct CycleTuning {
    /// 낮의 길이입니다. 이 시간이 지나면 밤이 됩니다.
    pub day_secs: f32,
    /// 밤의 무리를 모두 처치한 뒤 다음 날의 낮이 시작되기까지의 시간입니다.
    pub dawn_secs: f32,
    /// 원정의 결과를 보여 주는 시간입니다. 이 시간이 지나면 새 원정이 시작됩니다.
    pub restart_secs: f32,
}

/// 밤 하나입니다. 무리는 목록의 순서대로 나옵니다.
#[derive(Clone, Debug, PartialEq)]
pub struct NightPlan {
    pub waves: Vec<WavePlan>,
}

/// 무리 하나입니다. 무리가 나타나는 방법은 종류와 수뿐이므로, 새 종류의 몬스터가 나오는 밤은 이 목록에 항목을 더해서 만듭니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavePlan {
    pub kind: MonsterKind,
    pub count: u32,
}

/// 밤의 진행과 밤의 몬스터가 나타나는 방법입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct NightTuning {
    /// 날마다 밤 하나입니다. 길이가 원정의 날 수입니다.
    pub plans: Vec<NightPlan>,
    /// 밤이 시작된 뒤 첫 무리가 나타나기까지의 시간입니다.
    pub first_wave_delay_secs: f32,
    /// 한 무리를 모두 처치한 뒤 다음 무리가 나타나기까지의 시간입니다.
    pub between_waves_secs: f32,
    /// 한 무리 안에서 몬스터가 한 마리씩 나타나는 간격입니다.
    pub spawn_interval_secs: f32,
    /// 마차의 가장자리에서 밤의 몬스터가 나타나는 자리까지의 거리입니다. 마을의 범위보다 멀어야 합니다.
    pub spawn_distance: f32,
    /// 한 방향에서 나란히 나타나는 줄의 수와 줄 사이의 간격입니다.
    pub spawn_lanes: u32,
    pub spawn_lane_spacing: f32,
    /// 동시에 살아 있을 수 있는 몬스터의 수입니다. 이 수에 이르면 등장이 멈췄다가 몬스터가 줄면 이어집니다.
    pub max_alive: u32,
}

impl NightTuning {
    /// `day`번째 밤(1부터 셉니다)의 무리 수입니다. 없는 밤은 0입니다.
    pub fn wave_count(&self, day: u32) -> u32 {
        self.plan(day).map_or(0, |plan| plan.waves.len() as u32)
    }

    /// `day`번째 밤의 `wave`번째 무리입니다(둘 다 1부터 셉니다).
    pub fn wave(&self, day: u32, wave: u32) -> Option<WavePlan> {
        let index = (wave as usize).checked_sub(1)?;
        self.plan(day)?.waves.get(index).copied()
    }

    /// 원정의 첫 무리부터 센 이 무리의 순번입니다(0부터 셉니다). 무리가 나타나는 방향이 이 순번으로 정해집니다.
    pub fn wave_serial(&self, day: u32, wave: u32) -> u32 {
        let earlier: usize = self
            .plans
            .iter()
            .take((day as usize).saturating_sub(1))
            .map(|plan| plan.waves.len())
            .sum();
        earlier as u32 + wave.saturating_sub(1)
    }

    fn plan(&self, day: u32) -> Option<&NightPlan> {
        self.plans.get((day as usize).checked_sub(1)?)
    }
}

/// 고정된 맵 하나입니다. 좌표는 타일 단위입니다.
/// 맵의 크기, 장애물, 마차는 정수 좌표에 놓아서 타일에 꼭 맞습니다(`Tuning::validate`가 검사합니다).
/// 채집 지점은 타일을 가리키므로 처음부터 타일에 맞습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct MapSpec {
    pub width: f32,
    pub height: f32,
    pub wagon: Rect,
    pub obstacles: Vec<Rect>,
    /// 채집 지점이 있는 타일입니다. 지점은 타일 하나를 차지하고 길을 막습니다. 번호는 이 목록의 순서대로 1부터 붙습니다.
    pub nodes: Vec<Tile>,
    /// 플레이어가 나타나는 위치입니다. 플레이어마다 자기 등장 순서의 지점을 쓰고, 입장할 때는 방에 있는 플레이어가 쓰지 않는 가장 앞의 순서를 받습니다.
    pub player_spawns: Vec<Vec2>,
    /// 주변 몬스터를 찾는 격자의 칸 크기입니다.
    pub spatial_cell: f32,
}

impl Tuning {
    /// 한 Tick이 나타내는 시간(초)입니다.
    pub fn dt(&self) -> f32 {
        1.0 / self.tick_hz as f32
    }

    /// 초를 Tick 수로 바꿉니다. 가장 가까운 정수로 반올림하되, 0초보다 길면 최소 1 Tick입니다.
    /// 소수로 적은 시간(0.8초)이 부동소수점 오차 때문에 한 Tick 더 길어지지 않도록 올림 대신 반올림을 씁니다.
    pub fn ticks(&self, secs: f32) -> u32 {
        let ticks = (secs * self.tick_hz as f32).round() as u32;
        if secs > 0.0 { ticks.max(1) } else { 0 }
    }

    /// 원정의 날 수입니다. 밤의 목록의 길이이므로, 날 수를 적는 곳은 따로 없습니다.
    pub fn day_count(&self) -> u32 {
        self.nights.plans.len() as u32
    }

    /// 수치가 규칙의 전제를 지키는지 검사합니다. 어긋난 첫 번째 항목을 설명하는 문장을 돌려줍니다.
    ///
    /// 속도는 모두 유한하고 0 이상이어야 합니다.
    /// 가장 중요한 전제는 「한 Tick에 움직일 수 있는 거리가 몸의 반지름보다 짧다」입니다.
    /// 충돌 처리는 이동한 뒤에 겹친 만큼 밀어내므로, 한 Tick에 얇은 벽을 건너뛰는 거리를 움직일 수 있으면 개체가 벽을 통과합니다.
    /// 플레이어는 걷는 거리를, 몬스터는 걷는 거리에 겹친 몬스터에게서 밀려나는 거리를 더한 값을 이 전제에 맞춰 검사합니다.
    ///
    /// 맵은 타일에 꼭 맞아야 합니다. 크기, 장애물, 마차가 정수 좌표이고, 몸의 반지름이 타일의 절반보다 짧고,
    /// 채집 지점이 맵 안의 빈 타일이면서 마을의 범위 밖에 있고,
    /// 비어 있는 타일이 모두 마차에 걸어서 이어져야 합니다. 충돌과 몬스터의 이동 경로가 이 전제 위에서 같은 타일 지도를 읽습니다.
    pub fn validate(&self) -> Result<(), String> {
        if self.tick_hz == 0 {
            return Err("tick_hz must be at least 1".to_string());
        }
        if self.max_players == 0 {
            return Err("max_players must be at least 1".to_string());
        }
        if self.nights.plans.is_empty() {
            return Err("there must be at least one night".to_string());
        }
        for plan in &self.nights.plans {
            if plan.waves.is_empty() || plan.waves.iter().any(|wave| wave.count == 0) {
                return Err(
                    "every night needs at least one wave, and every wave at least one monster"
                        .to_string(),
                );
            }
        }
        if self.nights.spawn_lanes == 0 {
            return Err("spawn_lanes must be at least 1".to_string());
        }
        if self.nights.max_alive == 0 {
            return Err("max_alive must be at least 1".to_string());
        }
        // HP가 0인 개체는 만들어지는 순간 쓰러지거나 제거되므로, 모든 최대 HP는 1 이상이어야 합니다.
        let all_hp = [self.player.max_hp, self.wagon.max_hp]
            .into_iter()
            .chain(MonsterKind::ALL.map(|kind| self.monsters.get(kind).max_hp))
            .chain(StructureKind::ALL.map(|kind| self.structures.get(kind).max_hp));
        if all_hp.into_iter().any(|hp| hp == 0) {
            return Err("every max_hp must be at least 1".to_string());
        }

        // 음수 속도는 개체를 목표의 반대쪽으로 걷게 하고, NaN은 아래의 모든 비교를 거짓으로 만들어 검사를 통과하므로 먼저 거절합니다.
        check_not_negative("player speed", self.player.speed)?;
        check_not_negative("player crawl_speed", self.player.crawl_speed)?;
        for kind in MonsterKind::ALL {
            let monster = self.monsters.get(kind);
            check_not_negative(&format!("{kind:?} speed"), monster.speed)?;
            check_not_negative(
                &format!("{kind:?} separation_speed"),
                monster.separation_speed,
            )?;
        }

        let dt = self.dt();
        let fastest_player = self.player.speed.max(self.player.crawl_speed);
        if fastest_player * dt >= self.player.radius {
            return Err(format!(
                "a player moves {} tiles per tick, which must be shorter than its radius {}",
                fastest_player * dt,
                self.player.radius
            ));
        }
        for kind in MonsterKind::ALL {
            let monster = self.monsters.get(kind);
            // 몬스터는 같은 Tick에 걸으면서 밀려날 수 있으므로, 움직일 수 있는 거리의 최댓값은 두 거리의 합입니다.
            let farthest = (monster.speed + monster.separation_speed) * dt;
            if farthest >= monster.radius {
                return Err(format!(
                    "{kind:?} moves up to {farthest} tiles per tick (walking plus separation), which must be shorter than its radius {}",
                    monster.radius
                ));
            }
        }

        if self.village.tower_limit == 0 {
            return Err("tower_limit must be at least 1".to_string());
        }
        check_not_negative("village build_range", self.village.build_range)?;
        for (name, value) in [
            ("day_secs", self.cycle.day_secs),
            ("dawn_secs", self.cycle.dawn_secs),
            ("restart_secs", self.cycle.restart_secs),
            ("first_wave_delay_secs", self.nights.first_wave_delay_secs),
            ("between_waves_secs", self.nights.between_waves_secs),
            ("spawn_interval_secs", self.nights.spawn_interval_secs),
            ("spawn_lane_spacing", self.nights.spawn_lane_spacing),
        ] {
            check_not_negative(name, value)?;
        }
        let beyond_the_village = self.nights.spawn_distance > self.village.build_range + 1.0;
        if !beyond_the_village {
            return Err(format!(
                "spawn_distance {} must be longer than the build range {} plus one tile",
                self.nights.spawn_distance, self.village.build_range
            ));
        }
        for kind in StructureKind::ALL {
            let build_secs = self.structures.get(kind).build_secs;
            check_not_negative(&format!("{kind:?} build_secs"), build_secs)?;
        }
        let resources = &self.resources;
        if resources.carry_capacity == 0 || resources.gather_per_pulse == 0 {
            return Err("carry_capacity and gather_per_pulse must be at least 1".to_string());
        }
        for (name, value) in [
            ("gather_pulse_secs", resources.gather_pulse_secs),
            ("gather_reach", resources.gather_reach),
            ("deposit_reach", resources.deposit_reach),
            ("pickup_reach", resources.pickup_reach),
        ] {
            check_not_negative(name, value)?;
        }

        // 몸의 반지름이 0.5보다 작아야 비어 있는 타일의 중심을 따라 걷는 몸이 이웃한 막힌 타일에 걸리지 않습니다.
        let radii = [self.player.radius]
            .into_iter()
            .chain(MonsterKind::ALL.map(|kind| self.monsters.get(kind).radius));
        if radii
            .into_iter()
            .any(|radius| radius.is_nan() || radius >= 0.5)
        {
            return Err("every body radius must be shorter than half a tile".to_string());
        }

        let map = &self.map;
        if map.spatial_cell <= 0.0 {
            return Err("spatial_cell must be positive".to_string());
        }
        let tiled =
            |value: f32| value >= 1.0 && value <= f32::from(u16::MAX) && value.fract() == 0.0;
        if !tiled(map.width) || !tiled(map.height) {
            return Err("the map size must be a whole number of tiles".to_string());
        }
        let bounds = Rect::new(0.0, 0.0, map.width, map.height);
        for solid in map.obstacles.iter().chain([&map.wagon]) {
            if !solid.is_tile_aligned() {
                return Err(format!("solid {solid:?} does not fit the tile grid"));
            }
            if !(bounds.contains(solid.min) && bounds.contains(solid.max)) {
                return Err(format!("solid {solid:?} is outside the map"));
            }
        }

        let mut taken: BTreeSet<Tile> = BTreeSet::new();
        for node in &map.nodes {
            let center = node.center();
            if !bounds.contains(center) {
                return Err(format!("gather node {node:?} is outside the map"));
            }
            let on_a_solid = map
                .obstacles
                .iter()
                .chain([&map.wagon])
                .any(|solid| solid.contains(center));
            if on_a_solid || !taken.insert(*node) {
                return Err(format!(
                    "gather node {node:?} overlaps a solid or another node"
                ));
            }
            if map.wagon.distance_to(center) <= self.village.build_range {
                return Err(format!(
                    "gather node {node:?} is inside the village; it must be farther than the build range {} from the wagon",
                    self.village.build_range
                ));
            }
        }
        check_not_negative("guards chase_range", self.guards.chase_range)?;
        if !map.nodes.is_empty() && resources.node_amount == 0 {
            return Err("node_amount must be at least 1 when the map has gather nodes".to_string());
        }

        // 아래 검사는 규칙이 실제로 쓰는 타일 지도를 읽습니다.
        let tiles = World::new(self).tiles;
        check_spawns(
            "player_spawns",
            &map.player_spawns,
            self.player.radius,
            bounds,
            &tiles,
        )?;
        let wagon_tiles = tiles.engage_tiles(map.wagon, self.monsters.longest_engage_distance());
        if !tiles.is_fully_connected(&wagon_tiles, None) {
            return Err("every free tile must be connected to the wagon".to_string());
        }
        SpawnPlan::new(&tiles, self)?;
        Ok(())
    }
}

/// 속도는 유한하고 0 이상이어야 합니다.
fn check_not_negative(name: &str, value: f32) -> Result<(), String> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(format!(
            "{name} must be a finite number that is not negative, but it is {value}"
        ))
    }
}

/// 등장 지점이 있고, 모두 맵 안에서 막힌 타일과 겹치지 않는지 검사합니다.
fn check_spawns(
    name: &str,
    spawns: &[Vec2],
    radius: f32,
    bounds: Rect,
    tiles: &TileMap,
) -> Result<(), String> {
    if spawns.is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    let mut near = Vec::new();
    for spawn in spawns {
        if !bounds.shrink(radius).contains(*spawn) {
            return Err(format!(
                "{name} point {spawn:?} is too close to the map edge"
            ));
        }
        tiles.blocked_rects_near(*spawn, radius, &mut near);
        if near.iter().any(|solid| solid.distance_to(*spawn) < radius) {
            return Err(format!("{name} point {spawn:?} overlaps a solid"));
        }
    }
    Ok(())
}

impl Default for Tuning {
    fn default() -> Tuning {
        let grunts = |count| WavePlan {
            kind: MonsterKind::Grunt,
            count,
        };
        Tuning {
            tick_hz: 20,
            max_players: 4,
            player: PlayerTuning {
                max_hp: 100,
                radius: 0.35,
                speed: 4.0,
                crawl_speed: 0.8,
                attack_damage: 100,
                attack_reach: 1.0,
                attack_half_angle_deg: 60.0,
                attack_cooldown_secs: 0.5,
            },
            revive: ReviveTuning {
                bleed_secs: 30.0,
                revive_secs: 8.0,
                revived_hp_percent: 30,
                range: 1.5,
            },
            monsters: MonsterTable {
                grunt: MonsterTuning {
                    max_hp: 60,
                    radius: 0.4,
                    speed: 2.2,
                    attack_damage: 10,
                    attack_reach: 0.5,
                    attack_cooldown_secs: 1.0,
                    aggro_range: 5.0,
                    separation_speed: 1.0,
                },
            },
            wagon: WagonTuning {
                max_hp: 500,
                start_stash: 10,
            },
            structures: StructureTable {
                watchtower: StructureTuning {
                    max_hp: 300,
                    range: 6.0,
                    damage: 15,
                    cooldown_secs: 0.8,
                    repair_hp_per_pulse: 10,
                    repair_pulse_secs: 0.5,
                    repair_range: 1.0,
                    build_cost: 10,
                    build_secs: 3.0,
                },
            },
            village: VillageTuning {
                build_range: 10.0,
                tower_limit: 40,
            },
            guards: GuardTuning {
                kind: MonsterKind::Grunt,
                per_node: 2,
                chase_range: 6.0,
            },
            resources: ResourceTuning {
                node_amount: 15,
                carry_capacity: 10,
                gather_per_pulse: 1,
                gather_pulse_secs: 0.5,
                gather_reach: 1.0,
                deposit_reach: 1.5,
                pickup_reach: 0.8,
            },
            cycle: CycleTuning {
                day_secs: 90.0,
                dawn_secs: 4.0,
                restart_secs: 5.0,
            },
            nights: NightTuning {
                plans: vec![
                    NightPlan {
                        waves: vec![grunts(8), grunts(12)],
                    },
                    NightPlan {
                        waves: vec![grunts(20), grunts(30), grunts(40)],
                    },
                    NightPlan {
                        waves: vec![grunts(60), grunts(90), grunts(150)],
                    },
                ],
                first_wave_delay_secs: 3.0,
                between_waves_secs: 4.0,
                spawn_interval_secs: 0.3,
                spawn_distance: 18.0,
                spawn_lanes: 5,
                spawn_lane_spacing: 0.9,
                max_alive: 300,
            },
            map: MapSpec::frontier(),
        }
    }
}

impl MapSpec {
    /// 64 x 64 타일입니다. 한가운데에 마차가 있고, 마차 둘레의 마을에 장애물이 다섯 개, 그 바깥에 여덟 개 있습니다.
    /// 바깥의 장애물은 밤의 몬스터가 나타나는 여덟 방향의 자리를 피해서 놓습니다.
    /// 채집 지점 여덟 곳은 마차에서 24타일쯤 떨어진 곳에, 밤의 몬스터가 나타나는 방향과 방향의 사이마다 하나씩 놓습니다.
    pub fn frontier() -> MapSpec {
        MapSpec {
            width: 64.0,
            height: 64.0,
            wagon: Rect::new(30.0, 31.0, 34.0, 33.0),
            obstacles: vec![
                Rect::new(24.0, 24.0, 28.0, 25.0),
                Rect::new(37.0, 23.0, 38.0, 28.0),
                Rect::new(25.0, 35.0, 26.0, 40.0),
                Rect::new(35.0, 38.0, 40.0, 39.0),
                Rect::new(31.0, 26.0, 33.0, 27.0),
                Rect::new(16.0, 20.0, 19.0, 21.0),
                Rect::new(45.0, 18.0, 46.0, 21.0),
                Rect::new(50.0, 44.0, 52.0, 46.0),
                Rect::new(14.0, 44.0, 15.0, 48.0),
                Rect::new(24.0, 52.0, 28.0, 53.0),
                Rect::new(36.0, 8.0, 40.0, 9.0),
                Rect::new(57.0, 36.0, 58.0, 40.0),
                Rect::new(7.0, 24.0, 8.0, 28.0),
            ],
            nodes: vec![
                Tile::new(54, 41),
                Tile::new(41, 54),
                Tile::new(22, 54),
                Tile::new(9, 41),
                Tile::new(9, 22),
                Tile::new(22, 9),
                Tile::new(41, 9),
                Tile::new(54, 22),
            ],
            player_spawns: vec![
                Vec2::new(32.0, 29.5),
                Vec2::new(32.0, 34.5),
                Vec2::new(28.5, 32.0),
                Vec2::new(37.0, 32.0),
            ],
            spatial_cell: 2.0,
        }
    }
}
