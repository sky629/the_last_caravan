//! 입장을 받아들일 때 한 번 보내는 정보입니다. 접속 중에 바뀌지 않는 값만 담습니다.
//!
//! 클라이언트가 수치표(`Tuning`)를 따로 알지 않고도 고정된 세계를 그리고 스냅샷을 보간할 수 있게 합니다.
//! 서버를 다른 수치로 실행해도 클라이언트가 어긋나지 않습니다.
//!
//! 이 파일은 게임 상태가 아니라 고정된 설정을 전송 형식으로 바꿉니다. 게임 상태를 바꾸는 곳은 `Snapshot::from_sim` 하나입니다.

use caravan_rules::{Delivery, MonsterKind, Player, Sim, Vec2, WeaponKind};
use serde::{Deserialize, Serialize};

use crate::snapshot::{WireMonsterKind, WireWeaponKind};
use crate::units::{WirePos, WireRect, WireTile};

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
    /// 원정의 날 수입니다. 「날 2 / 3」 같은 표시에 씁니다.
    pub day_count: u32,
    /// 서 있는 감시탑과 짓는 중인 감시탑을 합친 수의 상한입니다. 감시탑의 위치는 스냅샷에 실립니다.
    pub tower_limit: u32,
    /// 플레이어가 한 번에 들 수 있는 자원의 양입니다. 「들고 있는 양 / 상한」 표시에 씁니다.
    pub carry_capacity: u32,
    /// 채집 지점이 낮마다 가득 찼을 때 가지는 자원의 양입니다. 지점의 막대를 그리는 기준입니다.
    pub node_amount: u32,
    /// 채집 지점의 위치입니다. 번호가 작은 지점부터이고, 원정 동안 바뀌지 않습니다. 남은 양은 스냅샷에 실립니다.
    pub nodes: Vec<NodeInfo>,
    /// 플레이어를 그리는 원의 반지름(타일)입니다.
    pub player_radius: f32,
    /// 몬스터 종류마다 하나씩입니다. 새 종류가 생기면 항목이 늘어납니다.
    pub monster_kinds: Vec<MonsterKindInfo>,
    /// 무기 종류마다 하나씩입니다. 클라이언트가 공격 효과를 수치표의 값으로 그리게 하려고 보냅니다. 새 무기가 생기면 항목이 늘어납니다.
    pub weapons: Vec<WeaponInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u32,
    pub tile: WireTile,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonsterKindInfo {
    pub kind: WireMonsterKind,
    /// 몬스터를 그리는 원의 반지름(타일)입니다.
    pub radius: f32,
    pub max_hp: u32,
}

/// 무기 하나가 화면에 그려지는 데 필요한 것입니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeaponInfo {
    pub kind: WireWeaponKind,
    pub delivery: WireDelivery,
}

/// 공격이 전달되는 방식 가운데 그리는 데 필요한 값입니다. 규칙의 `Delivery`와 따로 정의해서, 규칙의 수치 구조가 전송 형식이 되지 않게 합니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireDelivery {
    /// 플레이어의 가장자리에서 `reach`까지, 조준 방향의 양쪽으로 `half_angle_deg`도의 부채꼴입니다.
    Arc { reach: f32, half_angle_deg: f32 },
    /// 탄환입니다. 탄환은 스냅샷의 탄환 목록으로 그리므로 수치가 없습니다.
    Projectile,
    /// 조준선 위의 몬스터를 곧바로 맞힙니다. 선이 끝난 자리는 스냅샷의 마지막 공격(`WireAttack::trace_end`)에 있으므로 수치가 없습니다.
    Hitscan,
}

impl WireDelivery {
    /// `match`가 모든 방식을 다루므로, 규칙에 전달 방식을 추가하면 여기서 컴파일 오류가 납니다.
    fn of(delivery: Delivery) -> WireDelivery {
        match delivery {
            Delivery::Arc(arc) => WireDelivery::Arc {
                reach: arc.reach,
                half_angle_deg: arc.half_angle_deg,
            },
            Delivery::Projectile(_) => WireDelivery::Projectile,
            Delivery::Hitscan(_) => WireDelivery::Hitscan,
        }
    }
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
                day_count: tuning.day_count(),
                tower_limit: tuning.village.tower_limit,
                carry_capacity: tuning.resources.carry_capacity,
                node_amount: tuning.resources.node_amount,
                nodes: sim
                    .nodes()
                    .map(|node| NodeInfo {
                        id: node.id.0,
                        tile: WireTile::from_tile(node.tile),
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
                weapons: WeaponKind::ALL
                    .iter()
                    .map(|&kind| WeaponInfo {
                        kind: WireWeaponKind::of(kind),
                        delivery: WireDelivery::of(tuning.weapons.get(kind).delivery),
                    })
                    .collect(),
            },
        }
    }
}
