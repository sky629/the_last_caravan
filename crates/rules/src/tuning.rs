//! 게임의 모든 수치를 이 파일 한 곳에 둡니다.
//!
//! 시간은 초, 속도는 초당 타일 수로 적습니다. 그래서 `tick_hz`를 바꿔도 게임의 속도감이 유지됩니다.
//! 초를 Tick으로 바꾸는 곳은 `Tuning::ticks` 하나이고, 게임 상태에는 정수 Tick만 저장합니다.
//! 몬스터, 시설, 무기의 수치는 종류별 표(`MonsterTable`, `StructureTable`, `WeaponTable`)에 있습니다.
//!
//! 서버는 `Tuning` 하나를 가지고 있고, 방을 만들 때마다 복사해서 넘깁니다.
//! 수치를 바꾼 `Tuning`은 `validate`로 검사할 수 있습니다.

use std::collections::BTreeSet;

use crate::kind::{MonsterKind, StructureKind, WeaponKind};
use crate::loadout::Loadout;
use crate::math::{Rect, Vec2};
use crate::spawn::SpawnPlan;
use crate::world::{Layout, Tile, TileMap, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    /// 서버가 1초에 실행하는 Tick의 수입니다. Tick 속도는 이 값 하나로만 정합니다.
    pub tick_hz: u32,
    /// 한 방에 들어올 수 있는 최대 인원입니다. `Sim::add_player`만 이 값을 읽습니다.
    pub max_players: usize,
    pub player: PlayerTuning,
    /// 무기 종류별 수치입니다. 플레이어의 공격 피해량과 쿨다운은 들고 있는 무기의 행에서 읽습니다(`Player::stats`).
    pub weapons: WeaponTable,
    pub sprint: SprintTuning,
    pub revive: ReviveTuning,
    /// 효과(감속, 출혈, 스턴)의 규칙에 공통으로 쓰이는 수치입니다. 효과마다 다른 수치는 무기와 시설의 행(`on_hit`)에 있습니다.
    pub effects: EffectTuning,
    pub monsters: MonsterTable,
    pub wagon: WagonTuning,
    pub structures: StructureTable,
    pub village: VillageTuning,
    pub resources: ResourceTuning,
    pub guards: GuardTuning,
    pub loot: LootTuning,
    pub cycle: CycleTuning,
    /// 야영지와 결과 화면, 원정을 시작하는 방법입니다.
    pub camp: CampTuning,
    pub nights: NightTuning,
    pub map: MapSpec,
}

/// 플레이어의 몸에 관한 수치입니다. 공격에 관한 수치는 무기 표(`WeaponTable`)에 있습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerTuning {
    pub max_hp: u32,
    pub radius: f32,
    pub speed: f32,
    /// 쓰러진 상태에서 기어가는 속도입니다.
    pub crawl_speed: f32,
    /// 플레이어가 입장할 때와 새 원정이 시작될 때 가지는 무기입니다. 플레이어가 방에서 나갈 때는 이 무기와 같은 종류의 무기가 바닥에 떨어지지 않습니다.
    pub start_loadout: Loadout,
}

/// 무기 한 종류의 수치입니다. 무기마다 다른 점은 모두 이 구조체의 값입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponTuning {
    /// 공격이 맞힌 몬스터 하나가 입는 피해량입니다.
    pub damage: u32,
    /// 공격한 뒤 다음 공격까지의 시간입니다. 쿨다운은 플레이어가 하나만 가지므로, 무기를 바꿔도 남은 시간이 이어집니다.
    pub cooldown_secs: f32,
    pub delivery: Delivery,
    /// 맞힌 몬스터마다 거는 효과입니다. 피해와 함께 걸리고, 그 피해로 죽은 몬스터에게는 걸리지 않습니다.
    pub on_hit: Option<Debuff>,
    /// 상자에서 나올 무게입니다. 상자마다 무게에 비례하는 확률로 무기 하나가 나오고, 0이면 상자에서 나오지 않습니다.
    pub chest_weight: u32,
}

/// 맞은 몬스터에게 거는 효과입니다. 무기의 행과 시설의 행이 `on_hit`으로 가지고, 같은 값이 `Hit`에 실려서 몬스터에게 닿습니다.
/// 효과를 추가하려면 변형을 하나 더하고, `effect::MonsterEffects::apply`의 `match`에 갈래를 하나 더합니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Debuff {
    /// `secs`초 동안 몬스터가 스스로 걷는 속도에 `factor`를 곱합니다. `factor`는 0보다 크고 1 이하입니다.
    Slow { factor: f32, secs: f32 },
    /// `secs`초 동안 `EffectTuning::bleed_interval_secs`마다 `damage`의 피해를 줍니다.
    Bleed { damage: u32, secs: f32 },
    /// `secs`초 동안 몬스터가 스스로 이동하지도 공격하지도 않습니다. 끝난 뒤에는 `EffectTuning::stun_immunity_secs` 동안 다시 걸리지 않습니다.
    Stun { secs: f32 },
}

/// 플레이어가 받는 버프입니다. 버프의 세기는 `EffectTuning`에 있고, 길이는 버프를 거는 쪽이 정합니다. `secs`는 0보다 길어야 합니다.
/// 버프를 추가하려면 변형을 하나 더하고, `effect::PlayerBuffs::grant`의 `match`에 갈래를 하나 더합니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Buff {
    /// `secs`초 동안 공격의 쿨다운이 `EffectTuning::haste_cooldown_factor`배가 됩니다.
    Haste { secs: f32 },
    /// `secs`초 동안 이동 속도가 `EffectTuning::swift_speed_factor`배가 됩니다. 달리기의 배율과 곱해집니다.
    Swift { secs: f32 },
}

/// 효과의 규칙에 공통으로 쓰이는 수치입니다. 효과마다 다른 수치는 효과를 거는 쪽의 행(`Debuff`)에 있습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectTuning {
    /// 출혈이 피해를 주는 간격입니다. 출혈이 걸린 지 이 시간이 지나야 첫 피해가 나고, 그 뒤로 이 간격마다 피해가 납니다.
    pub bleed_interval_secs: f32,
    /// 스턴이 끝난 몬스터가 스턴에 다시 걸리지 않는 시간입니다. 스턴을 건 무기가 달라도 같습니다.
    pub stun_immunity_secs: f32,
    /// 공격 속도 버프(`Buff::Haste`)가 쿨다운에 곱하는 값입니다. 0보다 크고 1 이하입니다.
    pub haste_cooldown_factor: f32,
    /// 이동 속도 버프(`Buff::Swift`)가 이동 속도에 곱하는 값입니다. 1 이상입니다.
    pub swift_speed_factor: f32,
}

/// 공격이 맞히는 곳까지 전달되는 방식입니다. 방식마다 그 방식에서만 의미가 있는 수치를 한 구조체로 가집니다.
/// 방식을 추가하려면 변형을 하나 더하고, `weapon::deliver`의 `match`에 갈래를 하나 더합니다. 다른 방식의 코드는 바뀌지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Delivery {
    /// 공격하는 순간 부채꼴 안에 있는 몬스터를 모두 맞힙니다.
    Arc(ArcTuning),
    /// 탄환을 쏩니다. 탄환은 직선으로 날아가며 닿는 순서대로 `pierce`마리를 맞히고 사라집니다.
    Projectile(ProjectileTuning),
    /// 쏜 순간 조준 방향의 선 위에서 처음 닿는 몬스터 한 마리를 맞힙니다. 날아가는 것이 없으므로 같은 Tick에 결과가 정해집니다.
    Hitscan(HitscanTuning),
}

/// 부채꼴로 공격하는 방식의 수치입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcTuning {
    /// 플레이어의 가장자리에서 몬스터의 가장자리까지 잰 거리입니다.
    pub reach: f32,
    /// 조준 방향을 기준으로 한 부채꼴의 절반 각도입니다(도).
    pub half_angle_deg: f32,
}

/// 탄환을 쏘는 방식의 수치입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileTuning {
    /// 초당 타일 수입니다.
    pub speed: f32,
    /// 날아갈 수 있는 거리(타일)입니다. 이 거리를 다 날면 사라집니다.
    pub range: f32,
    /// 탄환의 반지름(타일)입니다. 몬스터의 반지름과 더한 거리 안으로 지나가면 닿은 것입니다.
    /// 플레이어의 반지름을 √2로 나눈 값보다 짧아야 합니다. 그래야 장애물의 모서리에 붙어 서서 쏜 탄환이 쏜 자리에서 멈추지 않습니다.
    pub radius: f32,
    /// 탄환 하나가 맞힐 수 있는 몬스터의 수입니다. 이만큼 맞히면 사라지고, 몬스터마다 같은 피해를 줍니다. 1이면 처음 닿는 한 마리에서 사라집니다.
    /// 1 이상이어야 합니다.
    pub pierce: u32,
}

/// 조준선 위의 몬스터를 곧바로 맞히는 방식의 수치입니다. 선의 굵기는 0이라서 몬스터의 몸 안으로 선이 지나가야 맞습니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitscanTuning {
    /// 플레이어의 중심에서 선이 닿는 거리(타일)입니다. 막힌 타일에 닿으면 거기서 끝납니다.
    pub range: f32,
}

/// 상자에서 나올 무게의 상한입니다. 무게의 합이 `u32`에 들어가므로 상자의 내용을 뽑는 계산이 넘치지 않습니다.
const MAX_CHEST_WEIGHT: u32 = 1000;

/// 부채꼴의 경계 각도에 대한 코사인입니다. 몬스터 방향과 조준 방향의 내적이 이 값 이상이면 부채꼴 안입니다.
/// 규칙에서 삼각함수를 쓰는 곳은 이 함수 하나입니다.
pub(crate) fn arc_cos(half_angle_deg: f32) -> f32 {
    half_angle_deg.to_radians().cos()
}

/// 무기 종류별 수치표입니다. 무기를 추가하는 일은 `WeaponKind`의 변형과 `ALL`, 이 표의 필드와 `get`의 분기, `Default`의 행 하나입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponTable {
    pub sword: WeaponTuning,
    pub bow: WeaponTuning,
    pub greatsword: WeaponTuning,
    pub spear: WeaponTuning,
    pub crossbow: WeaponTuning,
    pub rifle: WeaponTuning,
}

impl WeaponTable {
    pub fn get(&self, kind: WeaponKind) -> &WeaponTuning {
        match kind {
            WeaponKind::Sword => &self.sword,
            WeaponKind::Bow => &self.bow,
            WeaponKind::Greatsword => &self.greatsword,
            WeaponKind::Spear => &self.spear,
            WeaponKind::Crossbow => &self.crossbow,
            WeaponKind::Rifle => &self.rifle,
        }
    }

    /// 모든 무기의 `chest_weight`를 더한 값입니다. 0이면 상자에서 나올 수 있는 무기가 없습니다.
    pub fn total_chest_weight(&self) -> u32 {
        WeaponKind::ALL
            .iter()
            .map(|kind| self.get(*kind).chest_weight)
            .sum()
    }
}

/// 달리기의 수치입니다. 달리기 키를 누르고 움직이는 살아 있는 플레이어는 걷는 속도에 배율을 곱한 속도로 달리고,
/// 달리는 동안 게이지가 줄고 키를 떼고 있으면 게이지가 찹니다.
#[derive(Clone, Debug, PartialEq)]
pub struct SprintTuning {
    /// 달리는 속도는 걷는 속도(`PlayerTuning::speed`)에 이 배율을 곱한 값입니다. 1 이상이어야 합니다.
    pub speed_multiplier: f32,
    /// 가득 찬 게이지로 달릴 수 있는 시간입니다.
    pub sprint_secs: f32,
    /// 빈 게이지가 가득 차는 데 걸리는 시간입니다. 달리기 키를 떼고 있어야 찹니다.
    pub refill_secs: f32,
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
    /// 야영지의 허수아비입니다. 움직이지 않고 공격하지 않으므로 속도와 공격의 수치는 쓰이지 않습니다.
    /// 반지름은 몬스터가 걷는 길의 격자를 정하는 `largest_radius`에 들어가므로 `grunt`보다 크게 하지 않습니다.
    pub scarecrow: MonsterTuning,
}

impl MonsterTable {
    pub fn get(&self, kind: MonsterKind) -> &MonsterTuning {
        match kind {
            MonsterKind::Grunt => &self.grunt,
            MonsterKind::Scarecrow => &self.scarecrow,
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
    /// 마차 곁에 있는 살아 있는 플레이어가 HP를 회복하는 속도입니다. 거리는 플레이어의 가장자리에서 마차의 가장자리까지 잽니다.
    pub recovery: HealTuning,
    /// 플레이어가 F를 눌러서 마차의 HP를 수리하는 속도와 비용입니다.
    pub repair: RepairTuning,
}

/// HP를 회복시키는 규칙의 수치입니다.
/// 회복은 「일정한 시간마다 HP 일정량」으로 정합니다. 그래서 Tick 속도와 관계없이 HP가 정수로 남습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct HealTuning {
    pub hp_per_pulse: u32,
    pub pulse_secs: f32,
    /// 회복시키는 쪽의 가장자리에서 회복되는 쪽의 가장자리까지 잰, 회복이 닿는 거리입니다.
    pub range: f32,
}

/// 수리의 수치입니다. 수리는 HP를 회복시키는 규칙(`heal`)에, 한 번마다 보관함의 자원을 내는 비용이 더해진 것입니다.
/// 마차 곁의 회복처럼 자원을 내지 않는 회복은 `HealTuning`만 가지므로, 비용이 없다는 사실이 타입에 드러납니다.
#[derive(Clone, Debug, PartialEq)]
pub struct RepairTuning {
    pub heal: HealTuning,
    /// 수리의 한 번마다 보관함에서 내는 자원의 양입니다. 그 한 번이 회복시키는 HP가 `hp_per_pulse`보다 적어도 같은 양을 냅니다.
    /// 보관함이 낼 수 없으면 그 한 번은 회복시키지 않습니다. 1 이상이어야 합니다.
    pub cost_per_pulse: u32,
}

/// 마차와 그 둘레에 세우는 시설, 곧 마을의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct VillageTuning {
    /// 마차의 가장자리에서 건설할 타일의 중심까지 잰, 건설할 수 있는 거리입니다.
    pub build_range: f32,
    /// 서 있는 감시탑과 짓는 중인 감시탑을 합친 수의 상한입니다.
    pub tower_limit: u32,
    /// 원정이 시작될 때 이미 서 있는 감시탑의 타일입니다. 기본 수치에서는 비어 있고, 부하 시험(`--stress`)과 테스트가
    /// 감시탑이 많은 상황을 만들 때 씁니다. 건설과 같은 조건(마을 안의 빈 타일, 상한, 마차로 가는 길)을 지켜야 합니다.
    pub start_towers: Vec<Tile>,
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

/// 낮에 채집 지점과 무기 상자를 지키는 몬스터의 수치입니다. 이 몬스터들은 낮이 시작될 때마다 놓이고, 밤이 되면 마차를 향합니다.
#[derive(Clone, Debug, PartialEq)]
pub struct GuardTuning {
    pub kind: MonsterKind,
    /// 채집 지점 하나를 지키는 몬스터의 수입니다. 지점의 이웃한 빈 타일 하나에 한 마리씩 섭니다.
    pub per_node: u32,
    /// 무기 상자 하나를 지키는 몬스터의 수입니다. 상자가 놓인 자리의 이웃한 빈 타일 하나에 한 마리씩 섭니다.
    pub per_chest: u32,
    /// 지키는 자리의 중심에서 플레이어의 중심까지 잰, 플레이어를 쫓기 시작하는 거리입니다.
    /// 몬스터가 아니라 자리에서 재므로, 몬스터가 쫓다가 자리에서 이 거리를 넘으면 돌아갑니다.
    pub chase_range: f32,
}

/// 무기 상자와 바닥의 무기의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct LootTuning {
    /// 낮이 시작될 때마다 놓이는 상자의 수입니다. `MapSpec::chest_sites`의 수 이하입니다. 상자는 후보 자리 가운데 서로 다른 곳에 놓입니다.
    pub chests_per_day: u32,
    /// 상자를 열려면 F를 끊지 않고 눌러야 하는 시간입니다. 누르는 동안 피해를 받으면 처음부터 다시 눌러야 합니다.
    pub open_secs: f32,
    /// 플레이어의 가장자리에서 상자의 가장자리까지 잰, 상자를 열 수 있는 거리입니다.
    pub open_reach: f32,
    /// 플레이어의 가장자리에서 바닥의 무기까지 잰, 무기를 주울 수 있는 거리입니다.
    /// 상자를 여는 플레이어가 선 자리에서 그대로 무기를 주울 수 있도록, 상자가 놓이는 맵에서는 `open_reach`에 타일 절반의 대각선을 더한 값 이상입니다.
    pub take_reach: f32,
}

/// 시설 한 종류의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureTuning {
    pub max_hp: u32,
    /// 시설의 중심에서 몬스터의 중심까지 잰 사격 거리입니다.
    pub range: f32,
    pub damage: u32,
    pub cooldown_secs: f32,
    /// 맞힌 몬스터마다 거는 효과입니다. 감시탑은 걸지 않습니다.
    pub on_hit: Option<Debuff>,
    /// 플레이어가 F를 눌러서 시설의 HP를 수리하는 속도와 비용입니다. 거리는 플레이어의 가장자리에서 시설의 가장자리까지 잽니다.
    pub repair: RepairTuning,
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
}

/// 원정을 시작하는 방법입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// 방은 야영지에서 시작하고, 결과를 본 뒤 야영지로 돌아갑니다.
    WhenReady,
    /// 야영지 없이 방이 만들어지자마자, 그리고 결과 화면이 끝날 때마다 곧바로 원정을 시작합니다.
    /// 사람의 조작 없이 원정이 진행되어야 하는 부하 시험(`--stress`)만 씁니다.
    AtOnce,
}

/// 야영지와 결과 화면의 수치입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct CampTuning {
    pub start: Start,
    /// 모두가 준비한 뒤 원정이 시작되기까지의 시간입니다.
    pub countdown_secs: f32,
    /// 결과 화면을 보여 주는 가장 긴 시간입니다. 모두가 확인하면 더 일찍 끝납니다.
    pub debrief_secs: f32,
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

/// 무리의 크기가 방의 인원에 따라 달라지는지입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveScaling {
    /// `NightTuning::plans`의 수는 `full_party`명이 있을 때의 크기입니다. 무리의 크기는 `올림(수 × 인원 / full_party)`이고 1 이상입니다.
    /// 인원은 그 밤이 시작될 때 방에 있는 플레이어의 수입니다(`Phase::Night::party`). `full_party`는 방의 정원(`max_players`) 이상이어야 하므로, 무리는 수치표보다 커지지 않습니다.
    ByParty { full_party: u32 },
    /// 인원과 관계없이 `plans`의 수 그대로입니다. 몬스터의 수를 정해서 재는 부하 시험(`--stress`)이 씁니다.
    Fixed,
}

/// 밤의 진행과 밤의 몬스터가 나타나는 방법입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct NightTuning {
    /// 날마다 밤 하나입니다. 길이가 원정의 날 수입니다. 무리의 수는 `scaling`이 정하는 기준 인원일 때의 크기입니다.
    pub plans: Vec<NightPlan>,
    /// `plans`의 수를 방의 인원에 따라 바꾸는 방법입니다.
    pub scaling: WaveScaling,
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

    /// `day`번째 밤의 `wave`번째 무리에 나올 몬스터의 수입니다. 방에 플레이어가 `party`명 있을 때의 크기이고, 없는 무리는 0입니다.
    /// 인원에 따라 달라지는 값은 이 함수 한 곳에서 정합니다. 있는 무리는 인원이 몇 명이어도 한 마리 이상입니다.
    pub fn wave_size(&self, day: u32, wave: u32, party: u32) -> u32 {
        let Some(plan) = self.wave(day, wave) else {
            return 0;
        };
        match self.scaling {
            WaveScaling::Fixed => plan.count,
            WaveScaling::ByParty { full_party } => {
                let scaled =
                    (u64::from(plan.count) * u64::from(party)).div_ceil(u64::from(full_party));
                u32::try_from(scaled.max(1)).unwrap_or(u32::MAX)
            }
        }
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
/// 채집 지점과 상자의 후보 자리는 타일을 가리키므로 처음부터 타일에 맞습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct MapSpec {
    pub width: f32,
    pub height: f32,
    pub wagon: Rect,
    pub obstacles: Vec<Rect>,
    /// 채집 지점이 있는 타일입니다. 지점은 타일 하나를 차지하고 길을 막습니다. 번호는 이 목록의 순서대로 1부터 붙습니다.
    pub nodes: Vec<Tile>,
    /// 무기 상자가 놓일 수 있는 후보 타일입니다. 낮이 시작될 때마다 이 가운데 `loot.chests_per_day`곳에 상자가 놓입니다.
    /// 상자는 채집 지점과 같이 타일 하나를 차지하고 길을 막습니다.
    pub chest_sites: Vec<Tile>,
    /// 플레이어가 나타나는 위치입니다. 플레이어마다 자기 등장 순서의 지점을 쓰고, 입장할 때는 방에 있는 플레이어가 쓰지 않는 가장 앞의 순서를 받습니다.
    pub player_spawns: Vec<Vec2>,
    /// 주변 몬스터를 찾는 격자의 칸 크기입니다.
    pub spatial_cell: f32,
    /// 야영지에 놓이는 허수아비와 무기 진열대의 자리입니다.
    pub camp: CampLayout,
}

/// 야영지의 배치입니다. 야영지는 원정과 같은 지도이고, 채집 지점과 상자 대신 이것들이 놓입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct CampLayout {
    /// 허수아비가 서는 자리입니다. 한 줄로 세우면 석궁이 여러 마리를 꿰뚫는 것을 시험할 수 있습니다.
    pub scarecrows: Vec<Vec2>,
    /// 첫째 진열대의 자리입니다. 진열대는 무기 종류마다 하나이고, `WeaponKind::ALL`의 순서로 `rack_step`씩 떨어져 놓입니다.
    pub first_rack: Vec2,
    pub rack_step: Vec2,
}

impl CampLayout {
    /// 진열대마다 진열하는 무기와 자리입니다. 무기 종류를 더하면 진열대도 저절로 하나 늘어납니다.
    pub fn racks(&self) -> impl Iterator<Item = (WeaponKind, Vec2)> + '_ {
        WeaponKind::ALL
            .iter()
            .enumerate()
            .map(|(index, kind)| (*kind, self.first_rack + self.rack_step * index as f32))
    }
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

    /// 마을 안에서 감시탑 `count`개를 놓을 자리를 마차에 가까운 순서로 돌려줍니다. 가로와 세로 모두 짝수인 타일만 고르므로
    /// 감시탑 사이에 빈 타일이 한 칸씩 남고, 마차로 가는 길이 막히지 않습니다. 장애물, 마차, 플레이어의 등장 지점과 겹치는 타일은 건너뜁니다.
    /// 자리가 모자라면 있는 만큼만 돌려줍니다. 부하 시험과 테스트가 감시탑이 상한만큼 서 있는 상황을 만들 때 씁니다.
    pub fn spread_tower_sites(&self, count: u32) -> Vec<Tile> {
        let map = &self.map;
        let center = map.wagon.center();
        let mut sites: Vec<(f32, Tile)> = Vec::new();
        for x in (0..map.width as u16).step_by(2) {
            for y in (0..map.height as u16).step_by(2) {
                let tile = Tile::new(x, y);
                let on_a_solid = map
                    .obstacles
                    .iter()
                    .chain([&map.wagon])
                    .any(|solid| solid.contains(tile.center()));
                let on_a_spawn = map
                    .player_spawns
                    .iter()
                    .any(|spawn| tile.rect().distance_to(*spawn) < self.player.radius);
                let inside_the_village =
                    map.wagon.distance_to(tile.center()) <= self.village.build_range;
                if inside_the_village && !on_a_solid && !on_a_spawn {
                    sites.push((tile.center().distance(center), tile));
                }
            }
        }
        sites.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        sites
            .into_iter()
            .take(count as usize)
            .map(|(_, tile)| tile)
            .collect()
    }

    /// 원정의 날 수입니다. 밤의 목록의 길이이므로, 날 수를 적는 곳은 따로 없습니다.
    pub fn day_count(&self) -> u32 {
        self.nights.plans.len() as u32
    }

    /// 수치가 규칙의 전제를 지키는지 검사합니다. 어긋난 첫 번째 항목을 설명하는 문장을 돌려줍니다.
    ///
    /// 속도는 모두 유한하고 0 이상이어야 합니다.
    /// 가장 중요한 전제는 「한 번에 움직이는 거리가 몸의 반지름보다 짧다」입니다.
    /// 충돌 처리는 이동한 뒤에 겹친 만큼 밀어내므로, 얇은 벽을 건너뛰는 거리를 한 번에 움직일 수 있으면 개체가 벽을 통과합니다.
    /// 플레이어는 한 Tick의 이동이 반지름 이상이면 반지름보다 짧은 걸음으로 나누어 옮기므로(`player::move_players`) 속도에 상한이 없고, 걸음의 수를 정하는 반지름만 0보다 커야 합니다.
    /// 몬스터는 걷는 거리에 겹친 몬스터에게서 밀려나는 거리를 더한 값이 이 전제를 지키는지 검사합니다.
    ///
    /// 맵은 타일에 꼭 맞아야 합니다. 크기, 장애물, 마차가 정수 좌표이고, 몸의 반지름이 타일의 절반보다 짧고,
    /// 채집 지점이 맵 안의 빈 타일이면서 마을의 범위 밖에 있고,
    /// 비어 있는 타일이 모두 마차에 걸어서 이어져야 합니다. 무기 상자의 후보 자리는 `validate_chest_sites`가 따로 검사합니다. 충돌과 몬스터의 이동 경로가 이 전제 위에서 같은 타일 지도를 읽습니다.
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
        // 기준 인원이 정원보다 작으면, 꽉 찬 방의 무리가 수치표보다 커집니다.
        if let WaveScaling::ByParty { full_party } = self.nights.scaling
            && (full_party as usize) < self.max_players
        {
            return Err(format!(
                "the full_party {full_party} of the wave scaling must be at least max_players {}",
                self.max_players
            ));
        }
        if self.nights.spawn_lanes == 0 {
            return Err("spawn_lanes must be at least 1".to_string());
        }
        if self.nights.max_alive == 0 {
            return Err("max_alive must be at least 1".to_string());
        }
        // 허수아비는 맞아도 HP가 줄지 않는 야영지에서만 놓입니다. 원정에 나오면 처치할 수 없는 몬스터가 됩니다.
        let expedition_kinds = self
            .nights
            .plans
            .iter()
            .flat_map(|plan| plan.waves.iter().map(|wave| wave.kind))
            .chain([self.guards.kind]);
        for kind in expedition_kinds {
            if kind == MonsterKind::Scarecrow {
                return Err(
                    "the scarecrow stands only in the camp, not in waves or as a guard".to_string(),
                );
            }
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
        check_positive("player radius", self.player.radius)?;
        for kind in MonsterKind::ALL {
            let monster = self.monsters.get(kind);
            check_not_negative(&format!("{kind:?} speed"), monster.speed)?;
            check_not_negative(
                &format!("{kind:?} separation_speed"),
                monster.separation_speed,
            )?;
        }
        self.validate_weapons()?;
        self.validate_effects()?;

        let sprint = &self.sprint;
        if !(sprint.speed_multiplier.is_finite() && sprint.speed_multiplier >= 1.0) {
            return Err(format!(
                "sprint speed_multiplier must be a finite number of at least 1, but it is {}",
                sprint.speed_multiplier
            ));
        }
        for (name, secs) in [
            ("sprint_secs", sprint.sprint_secs),
            ("refill_secs", sprint.refill_secs),
        ] {
            if !(secs.is_finite() && secs > 0.0) {
                return Err(format!(
                    "sprint {name} must be a finite number longer than 0, but it is {secs}"
                ));
            }
        }
        if self
            .ticks(sprint.sprint_secs)
            .checked_mul(self.ticks(sprint.refill_secs))
            .is_none()
        {
            return Err(format!(
                "the sprint gauge ({} s times {} s in ticks) is too big to count",
                sprint.sprint_secs, sprint.refill_secs
            ));
        }

        let dt = self.dt();
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
            ("camp countdown_secs", self.camp.countdown_secs),
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
        check_heal("wagon recovery", &self.wagon.recovery)?;
        check_repair("wagon repair", &self.wagon.repair)?;
        for kind in StructureKind::ALL {
            check_repair(
                &format!("{kind:?} repair"),
                &self.structures.get(kind).repair,
            )?;
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
        if self.village.start_towers.len() > self.village.tower_limit as usize {
            return Err(format!(
                "{} start towers are more than the tower limit {}",
                self.village.start_towers.len(),
                self.village.tower_limit
            ));
        }
        let mut occupied: BTreeSet<Tile> = BTreeSet::new();
        for tower in &self.village.start_towers {
            let center = tower.center();
            let on_a_solid = map
                .obstacles
                .iter()
                .chain([&map.wagon])
                .any(|solid| solid.contains(center));
            let inside_the_village = map.wagon.distance_to(center) <= self.village.build_range;
            if !bounds.contains(center)
                || on_a_solid
                || !inside_the_village
                || !occupied.insert(*tower)
            {
                return Err(format!(
                    "start tower {tower:?} must be a free tile of the village that no other start tower takes"
                ));
            }
        }
        check_positive("camp debrief_secs", self.camp.debrief_secs)?;
        check_not_negative("guards chase_range", self.guards.chase_range)?;
        if !map.nodes.is_empty() && resources.node_amount == 0 {
            return Err("node_amount must be at least 1 when the map has gather nodes".to_string());
        }

        // 아래 검사는 규칙이 원정의 첫날에 실제로 쓰는 타일 지도를 읽습니다.
        let tiles = World::new(self, Layout::Expedition).tiles;
        check_spawns(
            "player_spawns",
            &map.player_spawns,
            self.player.radius,
            bounds,
            &tiles,
        )?;
        let wagon_tiles = tiles.engage_tiles(map.wagon, self.monsters.longest_engage_distance());
        if !tiles.is_fully_connected(&wagon_tiles, &[]) {
            return Err("every free tile must be connected to the wagon".to_string());
        }
        self.validate_chest_sites(&tiles, &wagon_tiles)?;
        SpawnPlan::new(&tiles, self)?;
        self.validate_camp(bounds)?;
        Ok(())
    }

    /// 야영지의 배치를 검사합니다. 허수아비와 진열대는 맵 안에 있고 장애물과 마차에 겹치지 않아야 합니다.
    /// 규칙이 야영지에서 실제로 쓰는 타일 지도를 읽습니다. 야영지에는 채집 지점과 상자가 없으므로 그 타일은 보지 않습니다.
    fn validate_camp(&self, bounds: Rect) -> Result<(), String> {
        let tiles = World::new(self, Layout::Camp).tiles;
        let radius = self.monsters.scarecrow.radius;
        let mut near = Vec::new();
        let spots = self
            .map
            .camp
            .scarecrows
            .iter()
            .map(|spot| ("scarecrow", *spot))
            .chain(self.map.camp.racks().map(|(_, spot)| ("weapon rack", spot)));
        for (name, spot) in spots {
            tiles.blocked_rects_near(spot, radius, &mut near);
            let clear = bounds.shrink(radius).contains(spot)
                && near.iter().all(|solid| solid.distance_to(spot) >= radius);
            if !clear {
                return Err(format!(
                    "the camp {name} at {spot:?} must stand inside the map, clear of obstacles and the wagon"
                ));
            }
        }
        Ok(())
    }

    /// 무기 상자의 수치와 후보 자리를 검사합니다. `validate`가 타일 지도를 만든 뒤에 부릅니다.
    ///
    /// - 후보 자리는 맵 안의 빈 타일이고, 서로 겹치지 않고, 마을의 범위 밖에 있습니다. 마을 안에 상자가 놓이면 싸우지 않고 열 수 있기 때문입니다.
    /// - 후보 자리가 모두 막혀도 비어 있는 타일이 모두 마차에 이어집니다. 어느 날 어느 자리에 상자가 놓여도 길이 끊기지 않습니다.
    /// - 후보 자리마다 마을의 범위 밖에 상하좌우로 이웃한 빈 타일이 하나 이상 있고, 그 타일은 다른 후보 자리가 아닙니다.
    ///   플레이어가 그 타일에 서서 상자를 열 수 있고, 감시탑이 그 타일을 막을 수 없으므로 상자가 사라진 뒤의 타일도 길에 이어집니다.
    /// - 상자는 후보 자리의 수보다 많이 놓이지 않고, 상자가 놓이는 맵이면 상자에서 나오는 무기의 비중을 더한 값이 1 이상입니다.
    /// - 상자가 놓이는 맵이면 무기를 줍는 거리가 상자를 여는 거리에 타일 절반의 대각선을 더한 값 이상입니다. 무기는 상자가 있던 타일의 중심에 놓이고,
    ///   상자를 열 수 있는 가장 먼 자리는 상자의 모서리에서 대각선으로 떨어진 자리이므로, 이 값이면 연 플레이어가 어느 방향에서 열었든 걷지 않고 무기를 줍습니다.
    fn validate_chest_sites(&self, tiles: &TileMap, wagon_tiles: &[Tile]) -> Result<(), String> {
        let loot = &self.loot;
        for (name, value) in [
            ("loot open_secs", loot.open_secs),
            ("loot open_reach", loot.open_reach),
            ("loot take_reach", loot.take_reach),
        ] {
            check_not_negative(name, value)?;
        }
        let sites = &self.map.chest_sites;
        if loot.chests_per_day as usize > sites.len() {
            return Err(format!(
                "{} chests per day need at least that many chest sites, but the map has {}",
                loot.chests_per_day,
                sites.len()
            ));
        }
        let outside_village =
            |tile: Tile| self.map.wagon.distance_to(tile.center()) > self.village.build_range;
        let mut taken: BTreeSet<Tile> = BTreeSet::new();
        for site in sites {
            if !tiles.is_free(*site) || !taken.insert(*site) {
                return Err(format!(
                    "chest site {site:?} must be a free tile of the map that no other site or gather node takes"
                ));
            }
            if !outside_village(*site) {
                return Err(format!(
                    "chest site {site:?} is inside the village; it must be farther than the build range {} from the wagon",
                    self.village.build_range
                ));
            }
        }
        if !tiles.is_fully_connected(wagon_tiles, sites) {
            return Err(
                "every free tile must stay connected to the wagon when every chest site is blocked"
                    .to_string(),
            );
        }
        for site in sites {
            let stand = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .filter_map(|(dx, dy)| site.step(dx, dy))
                .any(|next| tiles.is_free(next) && !taken.contains(&next) && outside_village(next));
            if !stand {
                return Err(format!(
                    "chest site {site:?} needs a free side tile outside the village to open it from"
                ));
            }
        }
        let to_the_middle_of_the_tile = loot.open_reach + std::f32::consts::FRAC_1_SQRT_2;
        if loot.chests_per_day > 0 && loot.take_reach < to_the_middle_of_the_tile {
            return Err(format!(
                "loot take_reach {} is shorter than open_reach {} plus half the tile diagonal ({to_the_middle_of_the_tile}), so a player who opens a chest from its far corner could not take the weapon without stepping",
                loot.take_reach, loot.open_reach
            ));
        }
        if loot.chests_per_day > 0 && self.weapons.total_chest_weight() == 0 {
            return Err(
                "some weapon needs a chest_weight of at least 1 when chests are placed".to_string(),
            );
        }
        Ok(())
    }

    /// 효과의 수치를 검사합니다. 공통 수치(`EffectTuning`)와, 무기와 시설의 행이 가진 효과(`on_hit`)를 모두 봅니다.
    ///
    /// - 시간은 유한하고 0보다 길어야 합니다. 0초 효과는 아무것도 하지 않고, 스턴이 끝난 뒤의 면역이 0초이면 스턴을 끝없이 이을 수 있습니다.
    /// - 감속의 비율은 0보다 크고 1 이하입니다. 0이면 스턴이고, 1보다 크면 빨라집니다.
    /// - 출혈의 피해는 1 이상이고, 출혈이 계속되는 시간은 간격 이상입니다. 그보다 짧으면 한 번도 피해가 나지 않습니다.
    /// - 공격 속도 버프의 비율은 0보다 크고 1 이하입니다. 0이면 쿨다운이 없어지고, 1보다 크면 버프가 공격을 느리게 합니다.
    /// - 이동 속도 버프의 비율은 1 이상입니다. 1보다 작으면 버프가 이동을 느리게 하고, 음수이면 뒤로 걷게 합니다.
    fn validate_effects(&self) -> Result<(), String> {
        let effects = &self.effects;
        check_positive("effects bleed_interval_secs", effects.bleed_interval_secs)?;
        check_positive("effects stun_immunity_secs", effects.stun_immunity_secs)?;
        let haste = effects.haste_cooldown_factor;
        if !(haste.is_finite() && haste > 0.0 && haste <= 1.0) {
            return Err(format!(
                "effects haste_cooldown_factor must be above 0 and at most 1, but it is {haste}"
            ));
        }
        let swift = effects.swift_speed_factor;
        if !(swift.is_finite() && swift >= 1.0) {
            return Err(format!(
                "effects swift_speed_factor must be a finite number of at least 1, but it is {swift}"
            ));
        }
        let rows = WeaponKind::ALL
            .map(|kind| (format!("{kind:?}"), self.weapons.get(kind).on_hit))
            .into_iter()
            .chain(
                StructureKind::ALL
                    .map(|kind| (format!("{kind:?}"), self.structures.get(kind).on_hit)),
            );
        for (name, on_hit) in rows {
            match on_hit {
                None => {}
                Some(Debuff::Slow { factor, secs }) => {
                    check_positive(&format!("{name} slow secs"), secs)?;
                    if !(factor.is_finite() && factor > 0.0 && factor <= 1.0) {
                        return Err(format!(
                            "{name} slow factor must be above 0 and at most 1, but it is {factor}"
                        ));
                    }
                }
                Some(Debuff::Bleed { damage, secs }) => {
                    check_positive(&format!("{name} bleed secs"), secs)?;
                    if damage == 0 {
                        return Err(format!("{name} bleed damage must be at least 1"));
                    }
                    if secs < effects.bleed_interval_secs {
                        return Err(format!(
                            "{name} bleed secs {secs} must be at least the bleed interval {}",
                            effects.bleed_interval_secs
                        ));
                    }
                }
                Some(Debuff::Stun { secs }) => {
                    check_positive(&format!("{name} stun secs"), secs)?;
                }
            }
        }
        Ok(())
    }

    /// 모든 무기의 수치를 검사합니다. `validate`가 속도를 검사한 직후에 부릅니다.
    ///
    /// - 상자에서 나올 무게는 상한(`MAX_CHEST_WEIGHT`) 이하입니다.
    /// - 쿨다운과 부채꼴의 닿는 거리는 유한하고 0 이상입니다. 쿨다운이 음수나 NaN이면 Tick 수로 바꾸는 계산이 깨집니다.
    /// - 부채꼴의 절반 각도는 0도 이상 180도 이하입니다.
    /// - 탄환의 속도와 사거리는 유한하고 0보다 큽니다. 0이면 탄환이 사라지지 않아서 스냅샷이 끝없이 커집니다.
    /// - 탄환이 맞힐 수 있는 몬스터의 수는 1 이상입니다. 0이면 탄환이 아무도 맞히지 못합니다.
    /// - 곧바로 맞히는 무기의 사거리는 유한하고 0보다 큽니다. 0이면 아무것도 맞히지 못합니다.
    /// - 탄환의 반지름은 유한하고 0 이상이며, √2를 곱한 값이 플레이어의 반지름보다 짧습니다. 탄환은 플레이어의 중심에서 나가므로,
    ///   벽이나 장애물의 모서리에 붙어 선 플레이어가 쏘는 탄환이 처음부터 막힌 곳에 닿으면 안 되기 때문입니다.
    ///
    /// 탄환의 속도에는 상한이 없습니다. 이동(`collision`)은 한 Tick의 거리가 반지름보다 짧아야 하지만, 탄환의 명중 판정은 이동하는 선분 전체를 검사합니다.
    ///
    /// `match`에 `_` 갈래가 없으므로 전달 방식을 추가하면 컴파일러가 이 검사를 더하라고 알려 줍니다.
    fn validate_weapons(&self) -> Result<(), String> {
        for kind in WeaponKind::ALL {
            let weapon = self.weapons.get(kind);
            check_not_negative(&format!("{kind:?} cooldown_secs"), weapon.cooldown_secs)?;
            if weapon.chest_weight > MAX_CHEST_WEIGHT {
                return Err(format!(
                    "{kind:?} chest_weight {} is above the limit {MAX_CHEST_WEIGHT}",
                    weapon.chest_weight
                ));
            }
            match weapon.delivery {
                Delivery::Arc(arc) => {
                    check_not_negative(&format!("{kind:?} reach"), arc.reach)?;
                    if !(0.0..=180.0).contains(&arc.half_angle_deg) {
                        return Err(format!(
                            "{kind:?} half_angle_deg must be between 0 and 180 degrees, but it is {}",
                            arc.half_angle_deg
                        ));
                    }
                }
                Delivery::Projectile(shot) => {
                    check_positive(&format!("{kind:?} projectile speed"), shot.speed)?;
                    check_positive(&format!("{kind:?} projectile range"), shot.range)?;
                    check_not_negative(&format!("{kind:?} projectile radius"), shot.radius)?;
                    if shot.pierce == 0 {
                        return Err(format!(
                            "{kind:?} projectile pierce must be at least 1, so that the projectile can hit a monster"
                        ));
                    }
                    // 탄환은 막힌 타일을 반지름만큼 키운 각진 사각형에 닿으면 멈춥니다. 그 사각형의 모서리는 타일의 모서리에서
                    // 반지름의 √2배까지 나와 있으므로, 타일의 모서리에 대각선으로 붙어 선 플레이어의 중심이 그 안에 들어가지 않아야 합니다.
                    if shot.radius * std::f32::consts::SQRT_2 >= self.player.radius {
                        return Err(format!(
                            "{kind:?} projectile radius {} times the square root of 2 must be shorter than the player radius {}",
                            shot.radius, self.player.radius
                        ));
                    }
                }
                Delivery::Hitscan(shot) => {
                    check_positive(&format!("{kind:?} hitscan range"), shot.range)?;
                }
            }
        }
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

/// 유한하고 0보다 커야 합니다.
fn check_positive(name: &str, value: f32) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!(
            "{name} must be a finite number longer than 0, but it is {value}"
        ))
    }
}

/// 회복의 간격은 유한하고 0보다 길어야 하고, 거리는 유한하고 0 이상이어야 합니다.
/// 마차 곁의 회복은 Tick 번호가 간격의 배수일 때 일어나므로, 간격이 0이면 한 번도 일어나지 않습니다.
fn check_heal(name: &str, heal: &HealTuning) -> Result<(), String> {
    if !(heal.pulse_secs.is_finite() && heal.pulse_secs > 0.0) {
        return Err(format!(
            "{name} pulse_secs must be a finite number longer than 0, but it is {}",
            heal.pulse_secs
        ));
    }
    check_not_negative(&format!("{name} range"), heal.range)
}

/// 수리는 회복의 조건을 모두 지켜야 하고, 비용이 1 이상이어야 합니다. 비용이 0이면 수리가 자원을 쓰지 않습니다.
fn check_repair(name: &str, repair: &RepairTuning) -> Result<(), String> {
    check_heal(name, &repair.heal)?;
    if repair.cost_per_pulse == 0 {
        return Err(format!("{name} cost_per_pulse must be at least 1"));
    }
    Ok(())
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
                start_loadout: Loadout::new(WeaponKind::Sword, None),
            },
            weapons: WeaponTable {
                sword: WeaponTuning {
                    damage: 25,
                    cooldown_secs: 0.5,
                    delivery: Delivery::Arc(ArcTuning {
                        reach: 1.0,
                        half_angle_deg: 60.0,
                    }),
                    on_hit: Some(Debuff::Bleed {
                        damage: 5,
                        secs: 3.0,
                    }),
                    chest_weight: 0,
                },
                bow: WeaponTuning {
                    damage: 15,
                    cooldown_secs: 0.4,
                    delivery: Delivery::Projectile(ProjectileTuning {
                        speed: 14.0,
                        range: 9.0,
                        radius: 0.15,
                        pierce: 1,
                    }),
                    on_hit: Some(Debuff::Slow {
                        factor: 0.5,
                        secs: 2.0,
                    }),
                    chest_weight: 1,
                },
                greatsword: WeaponTuning {
                    damage: 45,
                    cooldown_secs: 1.0,
                    delivery: Delivery::Arc(ArcTuning {
                        reach: 1.3,
                        half_angle_deg: 90.0,
                    }),
                    on_hit: Some(Debuff::Stun { secs: 0.8 }),
                    chest_weight: 1,
                },
                spear: WeaponTuning {
                    damage: 30,
                    cooldown_secs: 0.6,
                    delivery: Delivery::Arc(ArcTuning {
                        reach: 2.2,
                        half_angle_deg: 15.0,
                    }),
                    on_hit: Some(Debuff::Bleed {
                        damage: 6,
                        secs: 3.0,
                    }),
                    chest_weight: 1,
                },
                crossbow: WeaponTuning {
                    damage: 20,
                    cooldown_secs: 0.9,
                    delivery: Delivery::Projectile(ProjectileTuning {
                        speed: 18.0,
                        range: 11.0,
                        radius: 0.15,
                        pierce: 4,
                    }),
                    on_hit: Some(Debuff::Slow {
                        factor: 0.5,
                        secs: 2.0,
                    }),
                    chest_weight: 1,
                },
                rifle: WeaponTuning {
                    damage: 60,
                    cooldown_secs: 1.5,
                    delivery: Delivery::Hitscan(HitscanTuning { range: 12.0 }),
                    on_hit: Some(Debuff::Stun { secs: 0.8 }),
                    chest_weight: 1,
                },
            },
            sprint: SprintTuning {
                speed_multiplier: 1.6,
                sprint_secs: 4.0,
                refill_secs: 10.0,
            },
            revive: ReviveTuning {
                bleed_secs: 30.0,
                revive_secs: 5.0,
                revived_hp_percent: 30,
                range: 1.5,
            },
            effects: EffectTuning {
                bleed_interval_secs: 1.0,
                stun_immunity_secs: 2.0,
                haste_cooldown_factor: 0.7,
                swift_speed_factor: 1.3,
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
                scarecrow: MonsterTuning {
                    max_hp: 60,
                    radius: 0.4,
                    speed: 0.0,
                    attack_damage: 0,
                    attack_reach: 0.0,
                    attack_cooldown_secs: 1.0,
                    aggro_range: 0.0,
                    separation_speed: 0.0,
                },
            },
            wagon: WagonTuning {
                max_hp: 500,
                start_stash: 10,
                recovery: HealTuning {
                    hp_per_pulse: 1,
                    pulse_secs: 0.2,
                    range: 3.0,
                },
                // 1초에 20 HP이고, 마차를 최대 HP까지 모두 수리하면 자원 17이 듭니다.
                repair: RepairTuning {
                    heal: HealTuning {
                        hp_per_pulse: 30,
                        pulse_secs: 1.5,
                        range: 1.0,
                    },
                    cost_per_pulse: 1,
                },
            },
            structures: StructureTable {
                watchtower: StructureTuning {
                    max_hp: 300,
                    range: 6.0,
                    damage: 15,
                    cooldown_secs: 0.8,
                    on_hit: None,
                    // 1초에 20 HP이고, 감시탑을 최대 HP까지 모두 수리하면 건설 비용(10)과 같은 자원이 듭니다.
                    repair: RepairTuning {
                        heal: HealTuning {
                            hp_per_pulse: 30,
                            pulse_secs: 1.5,
                            range: 1.0,
                        },
                        cost_per_pulse: 1,
                    },
                    build_cost: 10,
                    build_secs: 3.0,
                },
            },
            village: VillageTuning {
                build_range: 10.0,
                tower_limit: 40,
                start_towers: Vec::new(),
            },
            guards: GuardTuning {
                kind: MonsterKind::Grunt,
                per_node: 2,
                per_chest: 3,
                chase_range: 6.0,
            },
            loot: LootTuning {
                chests_per_day: 3,
                open_secs: 1.5,
                open_reach: 1.0,
                take_reach: 1.75,
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
            },
            camp: CampTuning {
                start: Start::WhenReady,
                countdown_secs: 3.0,
                debrief_secs: 60.0,
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
                // 위의 무리는 네 명이 모였을 때의 크기입니다.
                scaling: WaveScaling::ByParty { full_party: 4 },
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
    /// 무기 상자의 후보 자리 여덟 곳은 마차에서 동서남북으로 15타일쯤, 대각선 방향으로 24타일쯤 떨어진 곳에 있습니다.
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
            chest_sites: vec![
                Tile::new(47, 32),
                Tile::new(32, 47),
                Tile::new(17, 32),
                Tile::new(32, 17),
                Tile::new(49, 49),
                Tile::new(15, 49),
                Tile::new(15, 15),
                Tile::new(49, 15),
            ],
            player_spawns: vec![
                Vec2::new(32.0, 29.5),
                Vec2::new(32.0, 34.5),
                Vec2::new(28.5, 32.0),
                Vec2::new(37.0, 32.0),
            ],
            spatial_cell: 2.0,
            // 허수아비는 마차의 동쪽에 한 줄로, 진열대는 마차의 북쪽에 한 줄로 섭니다.
            camp: CampLayout {
                scarecrows: vec![
                    Vec2::new(40.5, 32.0),
                    Vec2::new(41.5, 32.0),
                    Vec2::new(42.5, 32.0),
                ],
                first_rack: Vec2::new(29.0, 36.5),
                rack_step: Vec2::new(1.2, 0.0),
            },
        }
    }
}
