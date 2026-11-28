//! 규칙의 상태를 전송 형식으로 바꾸는 곳입니다. 게임 상태를 규칙에서 전송 형식으로 바꾸는 함수는 `Snapshot::from_sim` 하나입니다.
//!
//! 전송 형식의 타입은 규칙의 타입과 따로 정의합니다. 규칙의 내부 표현이 바뀌어도 전송 형식은 이 파일에서만 고칩니다.
//! 값은 클라이언트가 수치표를 읽지 않고 곧바로 그릴 수 있게 정합니다.
//! 시간은 Tick이 아니라 초로, 진행도는 0.0에서 1.0 사이의 실수로, 위치와 방향은 `units`의 정수로 보냅니다.
//! 번호는 규칙의 번호 타입이 아니라 `u32`입니다. 규칙의 타입은 전송 형식에 나타나지 않습니다.
//!
//! 몬스터는 수백 마리일 수 있으므로 `WireMonster` 하나가 postcard로 9바이트 안팎이 되게 정했습니다
//! (번호 2, 종류와 효과 1, 위치 4, 방향 1, HP 1). postcard는 필드 이름을 기록하지 않으므로 필드 이름은 크기에 영향이 없습니다.

use caravan_rules::{
    Act, Aim, Boss, BuildRefusal, CannotBuild, Chest, GatherNode, GroundWeapon, Interaction, Life,
    Loadout, Monster, MonsterKind, NightStep, Outcome, Phase, Pile, Player, PlayerBuffs,
    Projectile, Prompt, RepairTarget, Sim, Stage, StructureKind, Tower, TowerState, Tuning,
    WeaponKind,
};
use serde::{Deserialize, Serialize};

use crate::monster_status::{WireMonsterEffects, WireMonsterStatus};
use crate::units::{WireAngle, WirePos, WireTile};

/// Tick 하나가 끝난 뒤의 방 전체 상태입니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 서버의 Tick 번호입니다. 같은 방의 모든 클라이언트는 같은 Tick에 같은 상태를 받습니다.
    pub tick: u64,
    /// 이 방의 몇 번째 원정인지입니다. 첫 원정이 1이고, 첫 원정을 시작하기 전의 야영지에서는 0입니다.
    pub expedition: u32,
    pub phase: WirePhase,
    pub wagon: WireWagon,
    /// 마차의 보관함에 있는 자원입니다. 모든 플레이어가 함께 씁니다.
    pub stash: u32,
    /// 짓는 중인 포탑을 포함하고, 번호가 작은 포탑부터입니다. 파괴된 포탑은 없습니다.
    pub towers: Vec<WireTower>,
    /// 번호가 작은 채집 지점부터입니다. 위치는 `Welcome`에 있고 여기에는 남은 양만 있습니다.
    pub nodes: Vec<WireNode>,
    /// 땅에 떨어진 자원입니다. 번호가 작은 더미부터이고, 다 주워 간 더미는 없습니다.
    pub piles: Vec<WirePile>,
    /// 번호가 작은 플레이어부터입니다.
    pub players: Vec<WirePlayer>,
    /// 번호가 작은 몬스터부터입니다.
    pub monsters: Vec<WireMonster>,
    /// 낮에 놓여 있는 무기 상자입니다. 번호가 작은 상자부터이고, 밤이 되면 모두 사라집니다. 상자 안의 무기는 싣지 않습니다.
    pub chests: Vec<WireChest>,
    /// 바닥에 놓인 무기입니다. 번호가 작은 무기부터이고, 주워 간 무기는 없습니다.
    pub ground_weapons: Vec<WireGroundWeapon>,
    /// 날아가는 탄환과 이번 Tick에 끝난 탄환입니다. 번호가 작은 탄환부터입니다. 끝난 탄환이 한 번 실리므로 클라이언트가 탄환이 닿은 자리까지 그립니다.
    pub projectiles: Vec<WireProjectile>,
}

/// 방의 단계입니다. 야영지, 원정의 낮과 밤과 새벽, 결과 화면 가운데 하나이고, 단계마다 그 단계에서 표시할 값만 가집니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WirePhase {
    /// `day`번째 날의 낮이고, `secs_left`초 뒤에 밤이 됩니다.
    Day { day: u32, secs_left: f32 },
    /// `day`번째 날의 밤이고, 그 밤의 무리는 `wave_count`개입니다.
    Night {
        day: u32,
        wave_count: u32,
        step: WireNightStep,
    },
    /// 밤의 무리를 모두 처치했고, `secs_left`초 뒤에 `next_day`번째 날의 낮이 시작됩니다.
    Dawn { next_day: u32, secs_left: f32 },
    /// 야영지입니다. 진열대의 위치는 `Welcome`에 있습니다.
    Camp {
        /// 준비한 플레이어의 번호입니다. 번호가 작은 플레이어부터입니다.
        ready: Vec<u32>,
        /// 모두가 준비했으면, 원정이 시작되기까지 남은 초입니다.
        starting_in: Option<f32>,
        /// 허수아비마다 마지막으로 맞은 때와 그 Tick의 피해입니다. 번호가 작은 허수아비부터입니다.
        struck: Vec<WireStruck>,
    },
    /// 원정의 결과를 보여 주는 중입니다.
    Debrief {
        outcome: WireOutcome,
        /// 이 시간이 지나면 야영지로 돌아갑니다.
        secs_left: f32,
        /// 확인한 플레이어의 번호입니다. 번호가 작은 플레이어부터입니다.
        confirmed: Vec<u32>,
    },
}

/// 허수아비가 마지막으로 맞은 때입니다. 클라이언트는 `tick`이 최근일 때만 피해의 숫자를 띄웁니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireStruck {
    pub monster: u32,
    pub tick: u64,
    pub amount: u32,
}

/// 밤 안에서의 진행입니다. 무리의 번호는 그 밤 안에서 1부터 셉니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireNightStep {
    /// `next_wave`번 무리가 나타나기까지 `secs_left`초 남았습니다.
    Waiting { next_wave: u32, secs_left: f32 },
    /// `wave`번 무리가 나타나는 중이거나 싸우는 중입니다.
    Fighting { wave: u32 },
    /// 마지막 무리를 모두 처치했고, `secs_left`초 뒤에 보스가 나타납니다.
    Warning { secs_left: f32 },
    /// 보스와 싸우는 중입니다. 보스의 몸은 몬스터 목록에 있고, 여기에는 몸에 없는 것만 있습니다.
    Boss(WireBoss),
}

/// 보스에 관한 것 가운데 몬스터 목록의 몸에 없는 것입니다. 보스의 몸은 목록에서 종류(`WireMonsterKind::Boss`)로 찾습니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireBoss {
    /// 이 밤의 인원에 맞춘 최대 HP입니다. 몸의 현재 HP와 함께 HP 막대의 비율을 정합니다.
    pub max_hp: u32,
    /// 보스가 노리는 대상입니다.
    pub aim: WireAim,
    pub act: WireBossAct,
}

/// 보스가 노리는 대상입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireAim {
    Player(u32),
    Wagon,
}

/// 보스가 지금 하는 일입니다. 영역의 크기는 `Welcome`의 `BossInfo`에 있고 여기에는 자리와 진행도만 있습니다.
/// 진행도는 0.0에서 1.0이고, 1.0을 실은 스냅샷의 다음 Tick에 피해가 납니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireBossAct {
    /// 걷는 중입니다.
    Walking,
    /// 내려찍기를 예고하는 중입니다. `center`가 원의 중심입니다.
    Slam { center: WirePos, progress: f32 },
}

impl WireBoss {
    fn of(boss: &Boss) -> WireBoss {
        WireBoss {
            max_hp: boss.max_hp,
            aim: match boss.aim {
                Aim::Player(id) => WireAim::Player(id.0),
                Aim::Wagon => WireAim::Wagon,
            },
            act: match &boss.act {
                Act::Walking { .. } | Act::Winding { .. } | Act::Charging(_) => {
                    WireBossAct::Walking
                }
                Act::Slamming(telegraph) => WireBossAct::Slam {
                    center: WirePos::from_tiles(telegraph.zone.center()),
                    progress: clamp_progress(telegraph.progress()),
                },
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireOutcome {
    Victory,
    Defeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWagon {
    pub hp: u32,
    pub max_hp: u32,
}

/// 채집 지점 하나의 남은 자원입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireNode {
    pub id: u32,
    pub amount: u32,
}

/// 땅에 떨어진 자원 한 더미입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePile {
    pub id: u32,
    pub pos: WirePos,
    pub amount: u32,
}

/// 무기 상자 하나입니다. 어떤 무기가 들어 있는지는 열어서 바닥에 놓일 때까지 보내지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireChest {
    pub id: u32,
    pub tile: WireTile,
}

/// 바닥에 놓인 무기 하나입니다. 다른 플레이어가 줍고 맞바꾸면 `kind`가 바뀝니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireGroundWeapon {
    pub id: u32,
    pub pos: WirePos,
    pub kind: WireWeaponKind,
}

/// 포탑 하나입니다. 최대 HP와 범위는 종류마다 정해져 있으므로 싣지 않고, 클라이언트가 `Welcome`의 종류표(`StructureKindInfo`)에서 읽습니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireTower {
    pub id: u32,
    pub tile: WireTile,
    pub kind: WireStructureKind,
    pub state: WireTowerState,
}

/// 포탑의 종류입니다. `match`가 모든 종류를 다루므로, 규칙에 종류를 추가하면 `of`에서 컴파일 오류가 납니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireStructureKind {
    Watchtower,
    SlowTower,
    StunTower,
    HasteTower,
    SwiftTower,
}

impl WireStructureKind {
    pub fn of(kind: StructureKind) -> WireStructureKind {
        match kind {
            StructureKind::Watchtower => WireStructureKind::Watchtower,
            StructureKind::SlowTower => WireStructureKind::SlowTower,
            StructureKind::StunTower => WireStructureKind::StunTower,
            StructureKind::HasteTower => WireStructureKind::HasteTower,
            StructureKind::SwiftTower => WireStructureKind::SwiftTower,
        }
    }
}

/// 짓는 중인 포탑에는 HP와 사격 대상이 없고, 서 있는 포탑에는 건설의 진행도가 없습니다.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireTowerState {
    Building {
        /// 건설의 진행도(0.0에서 1.0)입니다.
        progress: f32,
    },
    Standing {
        hp: u32,
        last_shot: Option<WireShot>,
    },
}

/// 포탑의 마지막 사격입니다. 감시탑이 쏜 몬스터와 스턴 포탑이 스턴을 건 몬스터가 같은 값에 담깁니다.
/// 클라이언트는 `tick`이 최근일 때만 `target`을 향한 사격 선을 그립니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireShot {
    /// 맞은 몬스터의 번호입니다. 그 몬스터가 이미 사라졌을 수 있습니다.
    pub target: u32,
    pub tick: u64,
}

/// 플레이어가 마지막으로 한 공격입니다. `weapon`은 공격한 무기이고, 지금 든 무기와 다를 수 있습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireAttack {
    /// 공격한 서버 Tick입니다.
    pub tick: u64,
    pub weapon: WireWeaponKind,
    /// 곧바로 맞히는 무기이면 사격 선이 끝난 자리입니다. 클라이언트는 공격한 플레이어에서 이 자리까지 선을 긋습니다.
    /// 시작점은 보내지 않으므로 선이 없는 공격은 1바이트, 선이 있는 공격은 5바이트입니다.
    pub trace_end: Option<WirePos>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WirePlayer {
    pub id: u32,
    pub team: u8,
    pub pos: WirePos,
    pub facing: WireAngle,
    pub life: WireLife,
    /// 서버가 마지막으로 처리한 입력의 순서 번호입니다. 클라이언트 예측이 어디까지 확인되었는지 알려 줍니다.
    pub last_input_seq: u32,
    /// 마지막으로 한 공격입니다. 클라이언트는 공격한 Tick을 스냅샷의 `tick`과 비교해서, 공격한 무기의 효과를 그립니다.
    pub last_attack: Option<WireAttack>,
    /// 가진 무기입니다. 쓰러졌거나 사망한 플레이어도 무기를 가지고 있습니다.
    pub loadout: WireLoadout,
    /// F를 누르면 하게 되는 일 가운데 화면에 안내할 것입니다. 모든 클라이언트가 받고, 화면은 자기 것만 표시합니다.
    pub prompt: Option<WirePrompt>,
    /// 건설 키(B)가 지을 포탑의 종류입니다. 모든 클라이언트가 받고, 화면은 자기 것만 표시합니다.
    pub build_kind: WireStructureKind,
}

/// F를 누르면 하게 되는 일입니다. 부활, 수리, 채집은 안내하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WirePrompt {
    /// F를 누르고 있으면 가까운 상자를 엽니다.
    OpenChest,
    /// F를 한 번 누르면 가까운 바닥의 무기를 줍습니다.
    TakeWeapon(WireWeaponKind),
}

impl WirePrompt {
    /// `match`가 모든 안내를 다루므로, 규칙에 안내를 추가하면 여기서 컴파일 오류가 납니다.
    fn of(prompt: Prompt) -> WirePrompt {
        match prompt {
            Prompt::OpenChest => WirePrompt::OpenChest,
            Prompt::TakeWeapon(kind) => WirePrompt::TakeWeapon(WireWeaponKind::of(kind)),
        }
    }
}

/// 두 칸의 무기입니다. 든 무기는 언제나 있고, 다른 칸은 비어 있을 수 있습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireLoadout {
    pub held: WireWeaponKind,
    pub spare: Option<WireWeaponKind>,
}

impl WireLoadout {
    fn of(loadout: &Loadout) -> WireLoadout {
        WireLoadout {
            held: WireWeaponKind::of(loadout.held()),
            spare: loadout.spare().map(WireWeaponKind::of),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireWeaponKind {
    Sword,
    Bow,
    Greatsword,
    Spear,
    Crossbow,
    Rifle,
}

impl WireWeaponKind {
    /// `match`가 모든 종류를 다루므로, 규칙에 무기 종류를 추가하면 여기서 컴파일 오류가 납니다.
    pub fn of(kind: WeaponKind) -> WireWeaponKind {
        match kind {
            WeaponKind::Sword => WireWeaponKind::Sword,
            WeaponKind::Bow => WireWeaponKind::Bow,
            WeaponKind::Greatsword => WireWeaponKind::Greatsword,
            WeaponKind::Spear => WireWeaponKind::Spear,
            WeaponKind::Crossbow => WireWeaponKind::Crossbow,
            WeaponKind::Rifle => WireWeaponKind::Rifle,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireLife {
    Alive {
        hp: u32,
        max_hp: u32,
        /// 들고 있는 자원입니다. 상한은 `Welcome`의 `carry_capacity`입니다.
        carried: u32,
        /// 달리기 게이지가 가득 찬 값에 대한 백분율입니다(0에서 100). 모든 클라이언트가 받고, 화면은 자기 것만 표시합니다.
        stamina: u8,
        /// 상호작용 키나 건설 키가 지금 하고 있는 일입니다.
        interaction: Option<WireInteraction>,
        /// 건설 키를 누르고 있지만 세울 수 없을 때의 이유입니다. 모든 클라이언트가 받고, 화면은 자기 것만 표시합니다.
        build_refusal: Option<WireBuildRefusal>,
        /// 받은 버프입니다. 모든 클라이언트가 받고, 화면은 자기 것만 표시합니다. 쓰러지면 사라지므로 `Alive`에만 있습니다.
        buffs: WireBuffs,
    },
    Downed {
        /// 남은 출혈 시간(초)입니다. 부활이 진행되는 동안에는 줄지 않습니다.
        bleed_secs_left: f32,
        /// 동료가 부활시키는 중이면 진행도(0.0에서 1.0)입니다.
        revive_progress: Option<f32>,
    },
    Dead,
}

/// 플레이어가 받은 버프입니다. 남은 시간은 싣지 않습니다. 플레이어는 최대 넷이므로 bool 둘로 보내도 스냅샷은 플레이어마다 2바이트, 모두 합쳐 8바이트만 커집니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireBuffs {
    pub haste: bool,
    pub swift: bool,
}

impl WireBuffs {
    /// 버프가 하나도 없습니다.
    pub const NONE: WireBuffs = WireBuffs {
        haste: false,
        swift: false,
    };

    fn of(buffs: &PlayerBuffs) -> WireBuffs {
        WireBuffs {
            haste: buffs.has_haste(),
            swift: buffs.has_swift(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WireInteraction {
    Reviving {
        /// 부활시키는 대상의 플레이어 번호입니다.
        target: u32,
        /// 부활의 진행도(0.0에서 1.0)입니다.
        progress: f32,
    },
    Repairing {
        target: WireRepairTarget,
        /// 보관함의 자원이 수리의 비용보다 적어서 다음 수리가 회복시키지 못합니다. 이벤트가 아니라 상태이고, 자원이 생기면 곧바로 거짓이 됩니다.
        stash_short: bool,
    },
    Building {
        /// 짓고 있는 포탑의 번호입니다. 진행도는 스냅샷의 그 포탑에 있습니다.
        tower: u32,
    },
    Gathering {
        /// 채집하는 지점의 번호입니다. 남은 양은 스냅샷의 그 지점에 있습니다.
        node: u32,
    },
    Opening {
        /// 여는 상자의 번호입니다.
        chest: u32,
        /// 열기의 진행도(0.0에서 1.0)입니다. 진행도는 상자가 아니라 여는 플레이어에게 있습니다.
        progress: f32,
    },
    /// 공격에 맞아서 밀려나는 중입니다. 위치는 서버가 Tick마다 조금씩 옮기므로, 클라이언트가 보간하면 미끄러지듯 움직입니다.
    Knocked,
}

/// 수리하는 대상입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireRepairTarget {
    Tower {
        /// 수리하는 포탑의 번호입니다.
        tower: u32,
    },
    Wagon,
}

/// 건설이 거절된 이유와 놓으려던 타일입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireBuildRefusal {
    pub tile: WireTile,
    pub why: WireCannotBuild,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireCannotBuild {
    NotEnoughStash,
    TowerLimit,
    OutsideVillage,
    TileTaken,
    WouldBlockPath,
    InCamp,
}

impl WireCannotBuild {
    /// `match`가 모든 이유를 다루므로, 규칙에 거절의 이유를 추가하면 여기서 컴파일 오류가 납니다.
    pub fn of(why: CannotBuild) -> WireCannotBuild {
        match why {
            CannotBuild::NotEnoughStash => WireCannotBuild::NotEnoughStash,
            CannotBuild::TowerLimit => WireCannotBuild::TowerLimit,
            CannotBuild::OutsideVillage => WireCannotBuild::OutsideVillage,
            CannotBuild::TileTaken => WireCannotBuild::TileTaken,
            CannotBuild::WouldBlockPath => WireCannotBuild::WouldBlockPath,
            CannotBuild::InCamp => WireCannotBuild::InCamp,
        }
    }
}

impl WireBuildRefusal {
    fn of(refusal: &BuildRefusal) -> WireBuildRefusal {
        WireBuildRefusal {
            tile: WireTile::from_tile(refusal.tile),
            why: WireCannotBuild::of(refusal.why),
        }
    }
}

/// 몬스터 하나입니다. 몬스터는 수백 마리일 수 있으므로 필드를 이 다섯 개로 한정하고, 종류와 효과는 1바이트 하나에 함께 담습니다(`WireMonsterStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireMonster {
    pub id: u32,
    pub status: WireMonsterStatus,
    pub pos: WirePos,
    pub facing: WireAngle,
    /// 현재 HP입니다. 최대 HP는 `Welcome`의 몬스터 종류표에 있습니다.
    pub hp: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireMonsterKind {
    Grunt,
    Scarecrow,
    Boss,
}

impl WireMonsterKind {
    /// `match`가 모든 종류를 다루므로, 규칙에 몬스터 종류를 추가하면 여기서 컴파일 오류가 납니다.
    pub fn of(kind: MonsterKind) -> WireMonsterKind {
        match kind {
            MonsterKind::Grunt => WireMonsterKind::Grunt,
            MonsterKind::Scarecrow => WireMonsterKind::Scarecrow,
            MonsterKind::Boss => WireMonsterKind::Boss,
        }
    }

    /// 전송할 때 쓰는 종류의 순서입니다. `WireMonsterStatus`의 아래 4비트가 되므로 `MONSTER_KIND_CAPACITY`보다 작아야 합니다.
    /// `match`가 모든 종류를 다루므로 종류를 추가하면 컴파일러가 순서를 정하라고 알려 줍니다.
    pub(crate) fn index(self) -> u8 {
        match self {
            WireMonsterKind::Grunt => 0,
            WireMonsterKind::Scarecrow => 1,
            WireMonsterKind::Boss => 2,
        }
    }

    /// 순서가 `index`인 종류입니다. 규칙의 모든 종류(`MonsterKind::ALL`)에서 찾으므로 종류의 목록을 따로 두지 않습니다.
    pub(crate) fn from_index(index: u8) -> Option<WireMonsterKind> {
        MonsterKind::ALL
            .into_iter()
            .map(WireMonsterKind::of)
            .find(|kind| kind.index() == index)
    }
}

/// 탄환 하나입니다. 날아가는 방향으로 짧은 선을 그리는 데 필요한 값만 보냅니다. 피해량과 반지름은 보내지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireProjectile {
    pub id: u32,
    pub pos: WirePos,
    /// 날아가는 방향입니다.
    pub facing: WireAngle,
}

impl Snapshot {
    pub fn from_sim(sim: &Sim) -> Snapshot {
        let tuning = sim.tuning();
        Snapshot {
            tick: sim.tick(),
            expedition: sim.expedition_number(),
            phase: wire_stage(sim.stage(), tuning, sim.boss()),
            wagon: WireWagon {
                hp: sim.wagon().hp,
                max_hp: tuning.wagon.max_hp,
            },
            stash: sim.stash(),
            towers: sim
                .towers()
                .map(|tower| wire_tower(tower, tuning))
                .collect(),
            nodes: sim.nodes().map(wire_node).collect(),
            piles: sim.piles().map(wire_pile).collect(),
            players: sim
                .players()
                .map(|player| wire_player(sim, player))
                .collect(),
            monsters: sim.monsters().map(wire_monster).collect(),
            chests: sim.chests().map(wire_chest).collect(),
            ground_weapons: sim.ground_weapons().map(wire_ground_weapon).collect(),
            projectiles: sim.projectiles().map(wire_projectile).collect(),
        }
    }
}

/// Tick 수를 초로 바꿉니다. 나눗셈으로 구하므로 3초 분량의 Tick은 정확히 3.0입니다.
fn secs(ticks: u32, tuning: &Tuning) -> f32 {
    ticks as f32 / tuning.tick_hz as f32
}

fn wire_outcome(outcome: Outcome) -> WireOutcome {
    match outcome {
        Outcome::Victory => WireOutcome::Victory,
        Outcome::Defeat => WireOutcome::Defeat,
    }
}

fn wire_stage(stage: &Stage, tuning: &Tuning, boss: Option<&Boss>) -> WirePhase {
    match stage {
        Stage::Camp(camp) => WirePhase::Camp {
            ready: camp.ready().map(|id| id.0).collect(),
            starting_in: camp.countdown().map(|ticks| secs(ticks, tuning)),
            struck: camp
                .struck()
                .map(|(monster, struck)| WireStruck {
                    monster: monster.0,
                    tick: struck.tick,
                    amount: struck.amount,
                })
                .collect(),
        },
        Stage::Expedition(phase) => wire_phase(*phase, tuning, boss),
        Stage::Debrief(debrief) => WirePhase::Debrief {
            outcome: wire_outcome(debrief.outcome()),
            secs_left: secs(debrief.ticks_left(), tuning),
            confirmed: debrief.confirmed().map(|id| id.0).collect(),
        },
    }
}

fn wire_phase(phase: Phase, tuning: &Tuning, boss: Option<&Boss>) -> WirePhase {
    match phase {
        Phase::Day { day, ticks_left } => WirePhase::Day {
            day,
            secs_left: secs(ticks_left, tuning),
        },
        Phase::Night { day, step, .. } => WirePhase::Night {
            day,
            wave_count: tuning.nights.wave_count(day),
            step: match step {
                NightStep::Waiting {
                    next_wave,
                    ticks_left,
                } => WireNightStep::Waiting {
                    next_wave,
                    secs_left: secs(ticks_left, tuning),
                },
                NightStep::Fighting { wave, .. } => WireNightStep::Fighting { wave },
                NightStep::Warning { ticks_left } => WireNightStep::Warning {
                    secs_left: secs(ticks_left, tuning),
                },
                // 보스의 단계에 보스가 없는 것은 규칙의 어느 Tick 끝에서도 생기지 않습니다. 그래도 값을 정해야 하므로 보스가 막 나타나기 직전으로 보냅니다.
                NightStep::Boss => boss.map_or(WireNightStep::Warning { secs_left: 0.0 }, |boss| {
                    WireNightStep::Boss(WireBoss::of(boss))
                }),
            },
        },
        Phase::Dawn {
            next_day,
            ticks_left,
        } => WirePhase::Dawn {
            next_day,
            secs_left: secs(ticks_left, tuning),
        },
    }
}

fn wire_tower(tower: &Tower, tuning: &Tuning) -> WireTower {
    let state = match &tower.state {
        TowerState::Building { .. } => WireTowerState::Building {
            progress: clamp_progress(tower.build_progress(tuning).unwrap_or(0.0)),
        },
        TowerState::Standing { hp, last_shot, .. } => WireTowerState::Standing {
            hp: *hp,
            last_shot: last_shot.map(|shot| WireShot {
                target: shot.target.0,
                tick: shot.tick,
            }),
        },
    };
    WireTower {
        id: tower.id.0,
        tile: WireTile::from_tile(tower.tile),
        kind: WireStructureKind::of(tower.kind),
        state,
    }
}

fn wire_node(node: &GatherNode) -> WireNode {
    WireNode {
        id: node.id.0,
        amount: node.amount,
    }
}

fn wire_pile(pile: &Pile) -> WirePile {
    WirePile {
        id: pile.id.0,
        pos: WirePos::from_tiles(pile.pos),
        amount: pile.amount,
    }
}

/// `Chest::contents`는 이 크레이트 밖에서 읽을 수 없으므로 상자의 내용은 전송 형식에 들어갈 수 없습니다.
fn wire_chest(chest: &Chest) -> WireChest {
    WireChest {
        id: chest.id.0,
        tile: WireTile::from_tile(chest.tile),
    }
}

fn wire_ground_weapon(weapon: &GroundWeapon) -> WireGroundWeapon {
    WireGroundWeapon {
        id: weapon.id.0,
        pos: WirePos::from_tiles(weapon.pos),
        kind: WireWeaponKind::of(weapon.kind),
    }
}

fn wire_player(sim: &Sim, player: &Player) -> WirePlayer {
    let tuning = sim.tuning();
    let life = match &player.life {
        Life::Alive {
            hp,
            carried,
            stamina,
            interaction,
            build_refusal,
            buffs,
            ..
        } => WireLife::Alive {
            hp: *hp,
            max_hp: tuning.player.max_hp,
            carried: *carried,
            stamina: stamina.percent(tuning),
            interaction: interaction
                .as_ref()
                .map(|interaction| wire_interaction(sim, player, interaction)),
            build_refusal: build_refusal.as_ref().map(WireBuildRefusal::of),
            buffs: WireBuffs::of(buffs),
        },
        Life::Downed { bleed_left_ticks } => WireLife::Downed {
            bleed_secs_left: secs(*bleed_left_ticks, tuning),
            revive_progress: sim.revive_progress(player.id).map(clamp_progress),
        },
        Life::Dead => WireLife::Dead,
    };
    WirePlayer {
        id: player.id.0,
        team: player.team.0,
        pos: WirePos::from_tiles(player.pos),
        facing: WireAngle::from_direction(player.facing),
        life,
        last_input_seq: player.last_input_seq,
        last_attack: player.last_attack.map(|attack| WireAttack {
            tick: attack.tick,
            weapon: WireWeaponKind::of(attack.weapon),
            trace_end: attack.trace_end.map(WirePos::from_tiles),
        }),
        loadout: WireLoadout::of(&player.loadout),
        prompt: sim.prompt(player.id).map(WirePrompt::of),
        build_kind: WireStructureKind::of(player.build_kind),
    }
}

fn wire_interaction(sim: &Sim, player: &Player, interaction: &Interaction) -> WireInteraction {
    match interaction {
        Interaction::Reviving { target, .. } => WireInteraction::Reviving {
            target: target.0,
            // 진행도는 부활시키는 플레이어에게 있으므로 `Sim`이 그 플레이어를 찾아서 계산합니다.
            // 대상이 방금 사라진 Tick에는 값이 없으므로 0으로 둡니다. 부활은 다음 Tick에 취소됩니다.
            progress: sim.revive_progress(*target).map_or(0.0, clamp_progress),
        },
        Interaction::Repairing { target, .. } => WireInteraction::Repairing {
            target: match target {
                RepairTarget::Tower(tower) => WireRepairTarget::Tower { tower: tower.0 },
                RepairTarget::Wagon => WireRepairTarget::Wagon,
            },
            stash_short: sim.lacks_stash_to_repair(player.id),
        },
        Interaction::Gathering { node, .. } => WireInteraction::Gathering { node: node.0 },
        Interaction::Building { tower } => WireInteraction::Building { tower: tower.0 },
        Interaction::Knocked { .. } => WireInteraction::Knocked,
        Interaction::Opening { chest, .. } => WireInteraction::Opening {
            chest: chest.0,
            // 진행도는 여는 플레이어에게 있으므로 `Sim`이 그 플레이어의 열기에서 계산합니다.
            progress: sim.open_progress(player.id).map_or(0.0, clamp_progress),
        },
    }
}

/// 진행도가 0.0에서 1.0 사이라는 약속을 전송 형식에서 지킵니다.
fn clamp_progress(progress: f32) -> f32 {
    progress.clamp(0.0, 1.0)
}

fn wire_projectile(projectile: &Projectile) -> WireProjectile {
    WireProjectile {
        id: projectile.id.0,
        pos: WirePos::from_tiles(projectile.pos),
        facing: WireAngle::from_direction(projectile.dir),
    }
}

fn wire_monster(monster: &Monster) -> WireMonster {
    WireMonster {
        id: monster.id.0,
        status: WireMonsterStatus::new(
            WireMonsterKind::of(monster.kind),
            WireMonsterEffects::of(&monster.effects),
        ),
        pos: WirePos::from_tiles(monster.pos),
        facing: WireAngle::from_direction(monster.facing),
        hp: monster.hp,
    }
}
