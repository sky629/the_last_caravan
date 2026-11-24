//! 입장을 받아들일 때 한 번 보내는 정보입니다. 접속 중에 바뀌지 않는 값만 담습니다.
//!
//! 클라이언트가 수치표(`Tuning`)를 따로 알지 않고도 고정된 세계를 그리고 스냅샷을 보간할 수 있게 합니다.
//! 서버를 다른 수치로 실행해도 클라이언트가 어긋나지 않습니다.
//!
//! 이 파일은 게임 상태가 아니라 고정된 설정을 전송 형식으로 바꿉니다. 게임 상태를 바꾸는 곳은 `Snapshot::from_sim` 하나입니다.

use caravan_rules::{MonsterKind, Player, Sim, Vec2};
use serde::{Deserialize, Serialize};

use crate::snapshot::WireMonsterKind;
use crate::units::{WirePos, WireRect};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    /// 이 클라이언트의 플레이어 번호입니다.
    pub you: u32,
    /// 이 클라이언트의 진영입니다. 협동 모드에서는 모든 플레이어가 같습니다.
    pub team: u8,
    /// 서버가 1초에 보내는 스냅샷의 수입니다. 보간 지연을 정하는 데 씁니다.
    pub tick_hz: u32,
    /// 한 방의 최대 인원입니다.
    pub capacity: u32,
    pub world: WorldInfo,
}

/// 고정된 세계입니다. 좌표는 모두 타일 단위를 `WirePos`로 줄인 값입니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldInfo {
    /// 맵의 가로와 세로입니다. 왼쪽 아래 모서리가 (0, 0)입니다.
    pub size: WirePos,
    pub obstacles: Vec<WireRect>,
    pub wagon: WireRect,
    pub towers: Vec<TowerSite>,
    /// 플레이어를 그리는 원의 반지름(타일)입니다.
    pub player_radius: f32,
    /// 몬스터 종류마다 하나씩입니다. 새 종류가 생기면 항목이 늘어납니다.
    pub monster_kinds: Vec<MonsterKindInfo>,
}

/// 감시탑이 서 있는 자리입니다. 스냅샷의 `WireTower`는 같은 번호로 이 자리와 짝을 이룹니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TowerSite {
    pub id: u32,
    pub rect: WireRect,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonsterKindInfo {
    pub kind: WireMonsterKind,
    /// 몬스터를 그리는 원의 반지름(타일)입니다.
    pub radius: f32,
    pub max_hp: u32,
}

impl Welcome {
    /// `you`가 방에 들어온 직후의 `sim`에서 입장 안내를 만듭니다.
    pub fn from_sim(sim: &Sim, you: &Player) -> Welcome {
        let tuning = sim.tuning();
        let map = &tuning.map;
        Welcome {
            you: you.id.0,
            team: you.team.0,
            tick_hz: tuning.tick_hz,
            capacity: tuning.max_players as u32,
            world: WorldInfo {
                size: WirePos::from_tiles(Vec2::new(map.width, map.height)),
                obstacles: map
                    .obstacles
                    .iter()
                    .map(|r| WireRect::from_rect(*r))
                    .collect(),
                wagon: WireRect::from_rect(map.wagon),
                towers: sim
                    .towers()
                    .map(|tower| TowerSite {
                        id: tower.id.0,
                        rect: WireRect::from_rect(tower.rect()),
                    })
                    .collect(),
                player_radius: tuning.player.radius,
                monster_kinds: MonsterKind::ALL
                    .iter()
                    .map(|&kind| {
                        let numbers = tuning.monsters.get(kind);
                        MonsterKindInfo {
                            kind: WireMonsterKind::of(kind),
                            radius: numbers.radius,
                            max_hp: numbers.max_hp,
                        }
                    })
                    .collect(),
            },
        }
    }
}
