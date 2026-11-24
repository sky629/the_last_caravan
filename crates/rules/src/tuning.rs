//! 게임의 모든 수치를 이 파일 한 곳에 둡니다 (D21, D24).
//!
//! 시간은 초, 속도는 초당 타일 수로 적습니다. 그래서 `tick_hz`를 바꿔도 게임의 속도감이 유지됩니다.
//! 초를 Tick으로 바꾸는 곳은 `Tuning::ticks` 하나이고, 게임 상태에는 정수 Tick만 저장합니다.
//! 몬스터와 시설의 수치는 종류별 표(`MonsterTable`, `StructureTable`)에 있습니다.
//!
//! 서버는 `Tuning` 하나를 가지고 있고, 방을 만들 때마다 복사해서 넘깁니다.
//! 수치를 바꾼 `Tuning`은 `validate`로 검사할 수 있습니다.

use crate::kind::{MonsterKind, StructureKind};
use crate::math::{Rect, Vec2};

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
    pub waves: WaveTuning,
    pub map: MapSpec,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerTuning {
    pub max_hp: u32,
    pub radius: f32,
    pub speed: f32,
    /// 쓰러진 상태에서 기어가는 속도입니다 (D37).
    pub crawl_speed: f32,
    pub attack_damage: u32,
    /// 플레이어의 가장자리에서 몬스터의 가장자리까지 잰 근접 공격 거리입니다.
    pub attack_reach: f32,
    /// 조준 방향을 기준으로 한 공격 부채꼴의 절반 각도입니다(도). 부채꼴 안의 적을 모두 공격합니다 (D16).
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
    /// 마차로 가는 도중에 이 거리 안에 들어온 플레이어나 감시탑을 쫓습니다 (D17).
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
}

/// 시설 한 종류의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureTuning {
    pub max_hp: u32,
    /// 시설의 중심에서 몬스터의 중심까지 잰 사격 거리입니다.
    pub range: f32,
    pub damage: u32,
    pub cooldown_secs: f32,
    /// 수리는 「일정한 시간마다 HP 일정량」으로 정합니다. 그래서 Tick 속도와 관계없이 HP가 정수로 남습니다 (D48).
    pub repair_hp_per_pulse: u32,
    pub repair_pulse_secs: f32,
    /// 플레이어의 가장자리에서 시설의 가장자리까지 잰 수리 거리입니다.
    pub repair_range: f32,
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

#[derive(Clone, Debug, PartialEq)]
pub struct WaveTuning {
    /// 모든 무리가 등장시키는 몬스터의 종류입니다.
    pub kind: MonsterKind,
    /// 무리마다 몬스터의 수입니다. 길이가 무리의 수입니다 (D18).
    /// 무리는 겹치지 않으므로, 가장 큰 값이 동시에 존재하는 몬스터 수의 상한입니다.
    pub sizes: Vec<u32>,
    /// 원정이 시작된 뒤 첫 무리가 등장하기까지의 시간입니다.
    pub first_delay_secs: f32,
    /// 한 무리 안에서 몬스터가 한 마리씩 등장하는 간격입니다.
    pub spawn_interval_secs: f32,
    /// 등장 지점에서 맵 가장자리를 따라 몬스터를 나란히 세우는 줄의 수입니다.
    pub spawn_lanes: u32,
    /// 줄 사이의 간격입니다.
    pub spawn_lane_spacing: f32,
    /// 한 무리를 모두 처치한 뒤 다음 무리가 등장하기까지의 시간입니다 (D45).
    pub between_secs: f32,
    /// 원정의 결과를 보여 주는 시간입니다. 이 시간이 지나면 새 원정이 시작됩니다 (D20).
    pub restart_secs: f32,
}

impl WaveTuning {
    /// `wave`번 무리(1부터 셉니다)의 몬스터 수입니다. 없는 무리는 0입니다.
    pub fn size_of(&self, wave: u32) -> u32 {
        (wave as usize)
            .checked_sub(1)
            .and_then(|index| self.sizes.get(index))
            .copied()
            .unwrap_or(0)
    }
}

/// 마차나 시설을 놓는 위치입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructurePlacement {
    pub kind: StructureKind,
    pub rect: Rect,
}

/// 고정된 맵 하나입니다 (D18). 좌표는 타일 단위입니다.
/// 장애물은 정수 좌표에 놓아서, 클라이언트가 타일 단위로 나누어 그릴 수 있습니다 (C12).
#[derive(Clone, Debug, PartialEq)]
pub struct MapSpec {
    pub width: f32,
    pub height: f32,
    pub wagon: Rect,
    /// 원정이 시작될 때 놓여 있는 시설입니다. 번호는 이 목록의 순서대로 붙습니다.
    pub structures: Vec<StructurePlacement>,
    pub obstacles: Vec<Rect>,
    /// 플레이어가 나타나는 위치입니다. 입장한 순서대로 하나씩 씁니다.
    pub player_spawns: Vec<Vec2>,
    /// 맵 가장자리의 등장 지점입니다. 무리 N은 `monster_spawns[(N - 1) % 개수]`에서 등장합니다.
    pub monster_spawns: Vec<Vec2>,
    /// 몬스터 이동 경로 격자의 칸 크기입니다.
    pub nav_cell: f32,
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

    pub fn wave_count(&self) -> u32 {
        self.waves.sizes.len() as u32
    }

    /// 수치가 규칙의 전제를 지키는지 검사합니다. 어긋난 첫 번째 항목을 설명하는 문장을 돌려줍니다.
    ///
    /// 가장 중요한 전제는 「한 Tick에 움직일 수 있는 거리가 몸의 반지름보다 짧다」입니다.
    /// 충돌 처리는 이동한 뒤에 겹친 만큼 밀어내므로, 한 Tick에 얇은 벽을 건너뛰는 거리를 움직일 수 있으면 개체가 벽을 통과합니다.
    /// 플레이어는 걷는 거리를, 몬스터는 걷는 거리에 겹친 몬스터에게서 밀려나는 거리를 더한 값을 이 전제에 맞춰 검사합니다.
    pub fn validate(&self) -> Result<(), String> {
        if self.tick_hz == 0 {
            return Err("tick_hz must be at least 1".to_string());
        }
        if self.max_players == 0 {
            return Err("max_players must be at least 1".to_string());
        }
        if self.waves.sizes.is_empty() || self.waves.sizes.contains(&0) {
            return Err("every wave needs at least one monster".to_string());
        }
        if self.waves.spawn_lanes == 0 {
            return Err("spawn_lanes must be at least 1".to_string());
        }
        // HP가 0인 개체는 만들어지는 순간 쓰러지거나 제거되므로, 모든 최대 HP는 1 이상이어야 합니다.
        let all_hp = [self.player.max_hp, self.wagon.max_hp]
            .into_iter()
            .chain(MonsterKind::ALL.map(|kind| self.monsters.get(kind).max_hp))
            .chain(StructureKind::ALL.map(|kind| self.structures.get(kind).max_hp));
        if all_hp.into_iter().any(|hp| hp == 0) {
            return Err("every max_hp must be at least 1".to_string());
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
            let farthest = (monster.speed + monster.separation_speed.abs()) * dt;
            if farthest >= monster.radius {
                return Err(format!(
                    "{kind:?} moves up to {farthest} tiles per tick (walking plus separation), which must be shorter than its radius {}",
                    monster.radius
                ));
            }
        }

        let map = &self.map;
        if map.width <= 0.0 || map.height <= 0.0 || map.nav_cell <= 0.0 || map.spatial_cell <= 0.0 {
            return Err("map size and cell sizes must be positive".to_string());
        }
        let bounds = Rect::new(0.0, 0.0, map.width, map.height);
        let mut solids = map.obstacles.clone();
        solids.push(map.wagon);
        solids.extend(map.structures.iter().map(|placed| placed.rect));
        for solid in &solids {
            if !(bounds.contains(solid.min) && bounds.contains(solid.max)) {
                return Err(format!("solid {solid:?} is outside the map"));
            }
        }
        check_spawns(
            "player_spawns",
            &map.player_spawns,
            self.player.radius,
            bounds,
            &solids,
        )?;
        check_spawns(
            "monster_spawns",
            &map.monster_spawns,
            self.monsters.largest_radius(),
            bounds,
            &solids,
        )
    }
}

/// 등장 지점이 있고, 모두 맵 안에서 장애물과 겹치지 않는지 검사합니다.
fn check_spawns(
    name: &str,
    spawns: &[Vec2],
    radius: f32,
    bounds: Rect,
    solids: &[Rect],
) -> Result<(), String> {
    if spawns.is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    for spawn in spawns {
        if !bounds.shrink(radius).contains(*spawn) {
            return Err(format!(
                "{name} point {spawn:?} is too close to the map edge"
            ));
        }
        if solids
            .iter()
            .any(|solid| solid.distance_to(*spawn) < radius)
        {
            return Err(format!("{name} point {spawn:?} overlaps a solid"));
        }
    }
    Ok(())
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning {
            tick_hz: 20,
            max_players: 4,
            player: PlayerTuning {
                max_hp: 100,
                radius: 0.35,
                speed: 4.0,
                crawl_speed: 0.8,
                attack_damage: 25,
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
            wagon: WagonTuning { max_hp: 500 },
            structures: StructureTable {
                watchtower: StructureTuning {
                    max_hp: 300,
                    range: 6.0,
                    damage: 15,
                    cooldown_secs: 0.8,
                    repair_hp_per_pulse: 10,
                    repair_pulse_secs: 0.5,
                    repair_range: 1.0,
                },
            },
            waves: WaveTuning {
                kind: MonsterKind::Grunt,
                sizes: vec![6, 10, 16],
                first_delay_secs: 3.0,
                spawn_interval_secs: 0.3,
                spawn_lanes: 5,
                spawn_lane_spacing: 0.9,
                between_secs: 4.0,
                restart_secs: 5.0,
            },
            map: MapSpec::stage1(),
        }
    }
}

impl MapSpec {
    /// 24 x 24 타일입니다. 한가운데에 마차가 있고, 마차 옆에 감시탑이 하나 있고, 장애물이 다섯 개 있습니다.
    pub fn stage1() -> MapSpec {
        MapSpec {
            width: 24.0,
            height: 24.0,
            wagon: Rect::new(10.0, 11.0, 14.0, 13.0),
            structures: vec![StructurePlacement {
                kind: StructureKind::Watchtower,
                rect: Rect::new(15.0, 11.0, 16.0, 12.0),
            }],
            obstacles: vec![
                Rect::new(4.0, 4.0, 8.0, 5.0),
                Rect::new(17.0, 3.0, 18.0, 8.0),
                Rect::new(5.0, 15.0, 6.0, 20.0),
                Rect::new(15.0, 18.0, 20.0, 19.0),
                Rect::new(11.0, 6.0, 13.0, 7.0),
            ],
            player_spawns: vec![
                Vec2::new(12.0, 9.5),
                Vec2::new(12.0, 14.5),
                Vec2::new(8.5, 12.0),
                Vec2::new(17.0, 12.0),
            ],
            monster_spawns: vec![
                Vec2::new(12.0, 0.6),
                Vec2::new(23.4, 12.0),
                Vec2::new(12.0, 23.4),
                Vec2::new(0.6, 12.0),
            ],
            nav_cell: 0.5,
            spatial_cell: 2.0,
        }
    }
}
