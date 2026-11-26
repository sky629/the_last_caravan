//! 스냅샷 보관과 보간입니다. 클라이언트 예측과 외삽은 하지 않습니다.
//!
//! 화면은 가장 최근에 받은 스냅샷이 아니라 `INTERP_DELAY_TICKS`만큼 이전의 방 상태를 그립니다.
//! 그 시각의 앞뒤에 있는 스냅샷 두 개가 항상 있으므로, 위치와 방향을 두 스냅샷 사이의 선형 값으로 그릴 수 있습니다.
//! 시간은 초가 아니라 서버 Tick으로 셉니다. Tick 속도는 입장할 때 받으므로 이 파일에는 20이라는 값이 없습니다.
//!
//! 섞는 것은 위치와 방향뿐입니다. HP, 걸려 있는 효과, 생존 상태, 단계 같은 이산 값은 두 스냅샷 중 이전 것에서 가져옵니다.
//! 그래야 HP 막대가 그 피해를 일으킨 공격보다 먼저 줄어들지 않고, 효과의 색도 그 효과를 건 공격보다 먼저 바뀌지 않습니다.

use std::collections::VecDeque;
use std::f32::consts::TAU;

use caravan_protocol::{
    Snapshot, WireAngle, WireAttack, WireChest, WireGroundWeapon, WireLife, WireLoadout,
    WireMonster, WireMonsterEffects, WireMonsterKind, WireNode, WirePhase, WirePile, WirePlayer,
    WireProjectile, WirePrompt, WireTower, WireWagon,
};
use caravan_rules::{Team, Vec2};

/// 가장 최근 스냅샷보다 몇 Tick 이전을 그릴지 정합니다. 20Hz에서 2 Tick은 100ms입니다.
/// 스냅샷 하나가 늦거나 사라져도 그림이 멈추지 않을 만큼의 여유이고, 조작이 화면에 나타나기까지 걸리는 시간이기도 합니다.
pub const INTERP_DELAY_TICKS: f64 = 2.0;
/// 재생 시각이 목표에서 이만큼(20Hz에서 250ms) 벗어나면 천천히 따라가지 않고 목표로 건너뜁니다.
const MAX_DRIFT_TICKS: f64 = 5.0;
/// 재생 시각이 목표를 향해 초당 이 비율만큼 다가갑니다. 스냅샷이 도착하는 간격의 흔들림을 부드럽게 흡수합니다.
const EASE_PER_SEC: f64 = 4.0;

/// 세계가 놓인 모습을 가리키는 값입니다. 원정이 시작될 때와 야영지로 돌아갈 때 세계가 다시 놓이고 플레이어가 등장 지점으로 돌아가는데,
/// 그때마다 이 값이 바뀝니다. 원정이 시작되면 원정의 번호가 바뀌고, 결과 화면에서 야영지로 돌아가면 번호는 그대로이고 야영지 여부가 바뀝니다.
/// 값이 다른 두 시각의 위치는 이어지지 않으므로, 보간과 카메라는 그 사이를 섞지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setting {
    expedition: u32,
    camp: bool,
}

impl Setting {
    pub fn of(expedition: u32, phase: &WirePhase) -> Setting {
        Setting {
            expedition,
            camp: matches!(phase, WirePhase::Camp { .. }),
        }
    }
}

/// 지금 화면에 그릴 방의 모습입니다. 위치와 방향만 섞은 값이고, 나머지는 이전 스냅샷의 값입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// 그리고 있는 서버 Tick입니다. 소수 부분은 두 스냅샷 사이의 위치입니다.
    pub tick: f64,
    pub expedition: u32,
    pub phase: WirePhase,
    pub wagon: WireWagon,
    /// 모든 플레이어가 함께 쓰는 보관함의 자원입니다.
    pub stash: u32,
    /// 서 있는 감시탑과 짓는 중인 감시탑입니다. 위치는 타일이라서 섞지 않습니다.
    pub towers: Vec<WireTower>,
    /// 채집 지점의 남은 자원입니다. 위치는 입장 안내에 있습니다.
    pub nodes: Vec<WireNode>,
    /// 땅에 떨어진 자원입니다. 움직이지 않으므로 위치를 섞지 않습니다.
    pub piles: Vec<WirePile>,
    /// 낮에 놓여 있는 무기 상자입니다. 움직이지 않으므로 위치를 섞지 않습니다.
    pub chests: Vec<WireChest>,
    /// 바닥에 놓인 무기입니다. 움직이지 않으므로 위치를 섞지 않습니다.
    pub ground_weapons: Vec<WireGroundWeapon>,
    pub players: Vec<PlayerView>,
    pub monsters: Vec<MonsterView>,
    /// 날아가는 탄환과 한 Tick 전에 끝난 탄환입니다. 끝난 탄환은 끝난 자리에 그려집니다.
    pub projectiles: Vec<ProjectileView>,
}

impl Frame {
    pub fn setting(&self) -> Setting {
        Setting::of(self.expedition, &self.phase)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerView {
    pub id: u32,
    /// 플레이어가 편들어 싸우는 쪽입니다. 같은 편인지는 `Team::is_allied_with`로 판정합니다.
    pub team: Team,
    /// 지면 좌표(타일)입니다.
    pub pos: Vec2,
    /// 길이가 1인 지면 방향입니다.
    pub facing: Vec2,
    pub life: WireLife,
    /// 마지막으로 한 공격과 공격한 무기입니다.
    pub last_attack: Option<WireAttack>,
    /// 가진 두 칸의 무기입니다. 보간하지 않고 앞쪽 스냅샷의 값을 씁니다.
    pub loadout: WireLoadout,
    /// F를 누르면 하게 되는 일 가운데 화면에 안내할 것입니다. 보간하지 않고 앞쪽 스냅샷의 값을 씁니다.
    pub prompt: Option<WirePrompt>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonsterView {
    pub id: u32,
    pub kind: WireMonsterKind,
    /// 걸려 있는 효과입니다. 보간하지 않고 앞쪽 스냅샷의 값을 씁니다.
    pub effects: WireMonsterEffects,
    pub pos: Vec2,
    pub facing: Vec2,
    pub hp: u32,
}

/// 몬스터의 몸통 색으로 보이는 효과입니다. 여러 효과가 함께 걸려 있어도 하나만 보입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShownEffect {
    Stun,
    Slow,
    Bleed,
}

impl MonsterView {
    /// 몸통 색으로 보일 효과입니다. 행동을 가장 크게 바꾸는 효과가 먼저여서 스턴, 감속, 출혈의 순서로 앞선 하나입니다.
    pub fn shown_effect(&self) -> Option<ShownEffect> {
        let effects = self.effects;
        if effects.stunned {
            Some(ShownEffect::Stun)
        } else if effects.slowed {
            Some(ShownEffect::Slow)
        } else if effects.bleeding {
            Some(ShownEffect::Bleed)
        } else {
            None
        }
    }
}

/// 탄환 하나입니다. 날아가는 방향의 짧은 선을 그리는 데 필요한 위치와 방향입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileView {
    pub id: u32,
    /// 지면 좌표(타일)입니다.
    pub pos: Vec2,
    /// 길이가 1인 지면 방향입니다.
    pub facing: Vec2,
}

impl Frame {
    pub fn player(&self, id: u32) -> Option<&PlayerView> {
        self.players.iter().find(|player| player.id == id)
    }
}

/// 받은 스냅샷과 재생 시각입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotBuffer {
    tick_hz: f64,
    /// Tick이 오래된 것부터 정렬되어 있습니다.
    snapshots: VecDeque<Snapshot>,
    /// 그리고 있는 서버 Tick입니다. 첫 `advance`가 정합니다.
    playback: Option<f64>,
}

impl SnapshotBuffer {
    pub fn new(tick_hz: u32) -> SnapshotBuffer {
        SnapshotBuffer {
            tick_hz: f64::from(tick_hz),
            snapshots: VecDeque::new(),
            playback: None,
        }
    }

    /// 이미 받은 것보다 오래되었거나 같은 Tick의 스냅샷은 버립니다.
    /// 접속 방식에 따라 메시지의 순서가 바뀔 수 있고, 오래된 상태를 섞으면 캐릭터가 뒤로 움직이기 때문입니다.
    pub fn push(&mut self, snapshot: Snapshot) {
        if self
            .newest()
            .is_some_and(|newest| snapshot.tick <= newest.tick)
        {
            return;
        }
        self.snapshots.push_back(snapshot);
    }

    pub fn newest(&self) -> Option<&Snapshot> {
        self.snapshots.back()
    }

    /// 재생 시각을 프레임 시간만큼 앞으로 보내면서 `최신 Tick - 지연`으로 다가가게 합니다.
    pub fn advance(&mut self, dt_secs: f64) {
        let Some(newest) = self.newest() else {
            return;
        };
        let target = newest.tick as f64 - INTERP_DELAY_TICKS;
        let playback = match self.playback {
            None => target,
            Some(current) => {
                let ticking = current + dt_secs * self.tick_hz;
                if (target - ticking).abs() > MAX_DRIFT_TICKS {
                    target
                } else {
                    ticking + (target - ticking) * (dt_secs * EASE_PER_SEC).min(1.0)
                }
            }
        };
        self.playback = Some(playback);
        // 재생 시각이 지나간 스냅샷은 버립니다. 재생 시각 직전의 스냅샷 하나는 보간에 쓰므로 남깁니다.
        while self
            .snapshots
            .get(1)
            .is_some_and(|next| next.tick as f64 <= playback)
        {
            self.snapshots.pop_front();
        }
    }

    /// 지금 재생 시각의 방 모습입니다. 스냅샷을 받고 `advance`를 부르기 전에는 없습니다.
    pub fn frame(&self) -> Option<Frame> {
        self.sample_at(self.playback?)
    }

    /// `tick` 시각의 방 모습입니다. 그 시각을 앞뒤로 둘러싼 스냅샷 두 개 사이에서 위치와 방향을 섞습니다.
    /// 가장 오래된 스냅샷보다 앞이면 가장 오래된 것을, 가장 최근 스냅샷보다 뒤이면 가장 최근 것을 그대로 돌려줍니다.
    pub fn sample_at(&self, tick: f64) -> Option<Frame> {
        let at_or_before = self.snapshots.partition_point(|s| s.tick as f64 <= tick);
        let older_index = at_or_before.saturating_sub(1);
        let older = self.snapshots.get(older_index)?;
        let Some(newer) = self.snapshots.get(older_index + 1) else {
            return Some(frame_of(older, None, 0.0, older.tick as f64));
        };

        let span = (newer.tick - older.tick) as f64;
        let progress = ((tick - older.tick as f64) / span).clamp(0.0, 1.0);
        let shown_tick = older.tick as f64 + progress * span;
        // 세계가 다시 놓이면 플레이어가 등장 지점으로 돌아가므로, 그 앞뒤를 섞으면 맵을 가로지르는 모습이 나옵니다.
        let same_setting = Setting::of(older.expedition, &older.phase)
            == Setting::of(newer.expedition, &newer.phase);
        let blend_with = same_setting.then_some(newer);
        Some(frame_of(older, blend_with, progress as f32, shown_tick))
    }
}

/// `older`의 상태를 그리되, `newer`가 있으면 위치와 방향을 `alpha`만큼 `newer` 쪽으로 옮깁니다.
/// `newer`에 없는 개체는 `older`의 위치 그대로 그립니다. 그 개체가 사라지는 때는 재생 시각이 `newer`를 지나는 때입니다.
/// `newer`에만 있는 개체는 그리지 않습니다. 재생 시각이 `newer`를 지나면 나타납니다.
fn frame_of(older: &Snapshot, newer: Option<&Snapshot>, alpha: f32, tick: f64) -> Frame {
    // 스냅샷의 개체 목록은 번호 순서로 정렬되어 있으므로 이진 탐색으로 같은 번호를 찾습니다 (`Snapshot`의 약속).
    let players = older
        .players
        .iter()
        .map(|a| {
            let b = newer.and_then(|newer| {
                let index = newer.players.binary_search_by_key(&a.id, |p| p.id).ok()?;
                newer.players.get(index)
            });
            player_view(a, b, alpha)
        })
        .collect();
    let monsters = older
        .monsters
        .iter()
        .map(|a| {
            let b = newer.and_then(|newer| {
                let index = newer.monsters.binary_search_by_key(&a.id, |m| m.id).ok()?;
                newer.monsters.get(index)
            });
            monster_view(a, b, alpha)
        })
        .collect();
    // 탄환은 직선으로 일정한 속도로 날아가므로, 두 스냅샷 사이에서 위치를 선형으로 섞은 값이 실제 위치와 같습니다.
    // 뒤쪽 스냅샷에 없는 탄환(맞았거나, 막혔거나, 사거리를 다 날았습니다)은 재생 시각이 뒤쪽 스냅샷을 지날 때까지 끝난 자리에 그립니다.
    let projectiles = older
        .projectiles
        .iter()
        .map(|a| {
            let b = newer.and_then(|newer| {
                let index = newer
                    .projectiles
                    .binary_search_by_key(&a.id, |p| p.id)
                    .ok()?;
                newer.projectiles.get(index)
            });
            projectile_view(a, b, alpha)
        })
        .collect();
    Frame {
        tick,
        expedition: older.expedition,
        phase: older.phase.clone(),
        wagon: older.wagon,
        stash: older.stash,
        towers: older.towers.clone(),
        nodes: older.nodes.clone(),
        piles: older.piles.clone(),
        chests: older.chests.clone(),
        ground_weapons: older.ground_weapons.clone(),
        players,
        monsters,
        projectiles,
    }
}

fn player_view(a: &WirePlayer, b: Option<&WirePlayer>, alpha: f32) -> PlayerView {
    let (pos, facing) = match b {
        Some(b) => (
            lerp(a.pos.to_tiles(), b.pos.to_tiles(), alpha),
            blend_facing(a.facing, b.facing, alpha),
        ),
        None => (a.pos.to_tiles(), a.facing.to_direction()),
    };
    PlayerView {
        id: a.id,
        team: Team(a.team),
        pos,
        facing,
        life: a.life,
        last_attack: a.last_attack,
        loadout: a.loadout,
        prompt: a.prompt,
    }
}

fn projectile_view(a: &WireProjectile, b: Option<&WireProjectile>, alpha: f32) -> ProjectileView {
    let pos = match b {
        Some(b) => lerp(a.pos.to_tiles(), b.pos.to_tiles(), alpha),
        None => a.pos.to_tiles(),
    };
    ProjectileView {
        id: a.id,
        pos,
        facing: a.facing.to_direction(),
    }
}

fn monster_view(a: &WireMonster, b: Option<&WireMonster>, alpha: f32) -> MonsterView {
    let (pos, facing) = match b {
        Some(b) => (
            lerp(a.pos.to_tiles(), b.pos.to_tiles(), alpha),
            blend_facing(a.facing, b.facing, alpha),
        ),
        None => (a.pos.to_tiles(), a.facing.to_direction()),
    };
    MonsterView {
        id: a.id,
        kind: a.status.kind(),
        effects: a.status.effects(),
        pos,
        facing,
        hp: a.hp,
    }
}

fn lerp(a: Vec2, b: Vec2, alpha: f32) -> Vec2 {
    a + (b - a) * alpha
}

/// 방향을 짧은 호를 따라 섞습니다. 각도는 256단계의 `u8`이므로, 뺄셈을 `i8`로 읽으면 한 바퀴를 넘는 쪽이 아니라 짧은 쪽의 차이가 됩니다.
fn blend_facing(a: WireAngle, b: WireAngle, alpha: f32) -> Vec2 {
    let shorter_arc = b.0.wrapping_sub(a.0) as i8;
    let steps = f32::from(a.0) + f32::from(shorter_arc) * alpha;
    let radians = steps * TAU / 256.0;
    Vec2::new(radians.cos(), radians.sin())
}

/// 공격이나 사격 같은 효과가 `event_tick`에 일어난 뒤 `duration_ticks` 동안 0.0에서 1.0으로 진행하는 값입니다.
/// 효과가 끝났으면 `None`입니다. `playback_tick`은 `Frame::tick`이므로 화면에 보이는 시각을 기준으로 효과가 나타납니다.
pub fn effect_progress(playback_tick: f64, event_tick: u64, duration_ticks: f64) -> Option<f32> {
    let age = playback_tick - event_tick as f64;
    (0.0..duration_ticks)
        .contains(&age)
        .then(|| (age / duration_ticks) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::{WireMonsterStatus, WirePos, WireWeaponKind};

    fn snapshot(tick: u64) -> Snapshot {
        Snapshot {
            tick,
            expedition: 1,
            phase: WirePhase::Day {
                day: 1,
                secs_left: 90.0,
            },
            wagon: WireWagon {
                hp: 500,
                max_hp: 500,
            },
            stash: 0,
            towers: Vec::new(),
            nodes: Vec::new(),
            piles: Vec::new(),
            chests: Vec::new(),
            ground_weapons: Vec::new(),
            players: Vec::new(),
            monsters: Vec::new(),
            projectiles: Vec::new(),
        }
    }

    fn monster(id: u32, x: f32, y: f32, hp: u32) -> WireMonster {
        WireMonster {
            id,
            status: WireMonsterStatus::plain(WireMonsterKind::Grunt),
            pos: WirePos::from_tiles(Vec2::new(x, y)),
            facing: WireAngle(0),
            hp,
        }
    }

    fn player(id: u32, x: f32, y: f32, hp: u32) -> WirePlayer {
        WirePlayer {
            id,
            team: 0,
            pos: WirePos::from_tiles(Vec2::new(x, y)),
            facing: WireAngle(0),
            life: WireLife::Alive {
                hp,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: None,
                build_refusal: None,
            },
            last_input_seq: 0,
            last_attack: None,
            loadout: WireLoadout {
                held: WireWeaponKind::Sword,
                spare: None,
            },
            prompt: None,
        }
    }

    fn buffer_with(snapshots: Vec<Snapshot>) -> SnapshotBuffer {
        let mut buffer = SnapshotBuffer::new(20);
        for snapshot in snapshots {
            buffer.push(snapshot);
        }
        buffer
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 0.01
    }

    #[test]
    fn halfway_between_two_snapshots_a_monster_is_halfway() {
        let mut older = snapshot(10);
        older.monsters = vec![monster(1, 4.0, 4.0, 60)];
        let mut newer = snapshot(11);
        newer.monsters = vec![monster(1, 5.0, 6.0, 60)];
        let buffer = buffer_with(vec![older, newer]);

        let frame = buffer.sample_at(10.5).unwrap();

        assert!(close(frame.monsters[0].pos, Vec2::new(4.5, 5.0)));
        assert!((frame.tick - 10.5).abs() < 1e-9);
    }

    #[test]
    fn the_effects_of_a_monster_come_from_the_older_snapshot_and_its_position_is_blended() {
        let slowed = WireMonsterStatus::new(
            WireMonsterKind::Grunt,
            WireMonsterEffects {
                slowed: true,
                ..WireMonsterEffects::NONE
            },
        );
        let mut older = snapshot(10);
        older.monsters = vec![monster(1, 4.0, 4.0, 60)];
        let mut newer = snapshot(11);
        newer.monsters = vec![monster(1, 5.0, 4.0, 60)];
        newer.monsters[0].status = slowed;
        let mut later = snapshot(12);
        later.monsters = vec![monster(1, 6.0, 4.0, 60)];
        later.monsters[0].status = slowed;
        let buffer = buffer_with(vec![older, newer, later]);

        let before = buffer.sample_at(10.9).unwrap();
        let after = buffer.sample_at(11.1).unwrap();

        assert_eq!(before.monsters[0].effects, WireMonsterEffects::NONE);
        assert!(close(before.monsters[0].pos, Vec2::new(4.9, 4.0)));
        assert!(after.monsters[0].effects.slowed);
        assert_eq!(after.monsters[0].kind, WireMonsterKind::Grunt);
    }

    #[test]
    fn one_effect_is_shown_and_the_stun_comes_before_the_slow_and_the_slow_before_the_bleed() {
        let shown = |stunned, slowed, bleeding| {
            let mut snapshot = snapshot(10);
            snapshot.monsters = vec![monster(1, 4.0, 4.0, 60)];
            snapshot.monsters[0].status = WireMonsterStatus::new(
                WireMonsterKind::Grunt,
                WireMonsterEffects {
                    stunned,
                    slowed,
                    bleeding,
                },
            );
            buffer_with(vec![snapshot])
                .sample_at(10.0)
                .unwrap()
                .monsters[0]
                .shown_effect()
        };

        assert_eq!(shown(false, false, false), None);
        assert_eq!(shown(false, false, true), Some(ShownEffect::Bleed));
        assert_eq!(shown(false, true, false), Some(ShownEffect::Slow));
        assert_eq!(shown(false, true, true), Some(ShownEffect::Slow));
        assert_eq!(shown(true, false, false), Some(ShownEffect::Stun));
        assert_eq!(shown(true, false, true), Some(ShownEffect::Stun));
        assert_eq!(shown(true, true, false), Some(ShownEffect::Stun));
        assert_eq!(shown(true, true, true), Some(ShownEffect::Stun));
    }

    #[test]
    fn the_stash_the_towers_the_nodes_and_the_piles_come_from_the_older_snapshot() {
        let mut older = snapshot(10);
        older.stash = 7;
        older.nodes = vec![WireNode { id: 1, amount: 9 }];
        older.piles = vec![WirePile {
            id: 1,
            pos: WirePos::from_tiles(Vec2::new(4.0, 4.0)),
            amount: 3,
        }];
        let mut newer = snapshot(11);
        newer.stash = 17;
        newer.nodes = vec![WireNode { id: 1, amount: 8 }];
        newer.piles = Vec::new();
        let buffer = buffer_with(vec![older, newer]);

        let between = buffer.sample_at(10.9).unwrap();
        assert_eq!(between.stash, 7);
        assert_eq!(between.nodes, [WireNode { id: 1, amount: 9 }]);
        assert_eq!(between.piles.len(), 1);

        let after = buffer.sample_at(11.0).unwrap();
        assert_eq!(after.stash, 17);
        assert_eq!(after.nodes, [WireNode { id: 1, amount: 8 }]);
        assert!(after.piles.is_empty(), "the pile was picked up");
    }

    #[test]
    fn a_players_team_comes_through_to_the_frame() {
        let mut rival = player(2, 8.0, 8.0, 100);
        rival.team = 3;
        let mut only = snapshot(10);
        only.players = vec![player(1, 4.0, 4.0, 100), rival];
        let buffer = buffer_with(vec![only]);

        let frame = buffer.sample_at(10.0).unwrap();

        assert_eq!(frame.players[0].team, Team(0));
        assert_eq!(frame.players[1].team, Team(3));
    }

    #[test]
    fn hp_life_and_phase_come_from_the_older_snapshot() {
        let mut older = snapshot(10);
        older.monsters = vec![monster(1, 4.0, 4.0, 60)];
        older.players = vec![player(1, 8.0, 8.0, 100)];
        let mut newer = snapshot(11);
        newer.phase = WirePhase::Debrief {
            outcome: caravan_protocol::WireOutcome::Defeat,
            secs_left: 5.0,
            confirmed: vec![],
        };
        newer.wagon.hp = 100;
        newer.monsters = vec![monster(1, 5.0, 4.0, 20)];
        newer.players = vec![player(1, 9.0, 8.0, 40)];
        let buffer = buffer_with(vec![older, newer]);

        let frame = buffer.sample_at(10.9).unwrap();

        assert_eq!(frame.monsters[0].hp, 60);
        assert_eq!(frame.wagon.hp, 500);
        assert_eq!(
            frame.phase,
            WirePhase::Day {
                day: 1,
                secs_left: 90.0
            }
        );
        assert!(matches!(
            frame.players[0].life,
            WireLife::Alive { hp: 100, .. }
        ));
        assert!(close(frame.players[0].pos, Vec2::new(8.9, 8.0)));
    }

    fn arrow(id: u32, x: f32, y: f32) -> WireProjectile {
        WireProjectile {
            id,
            pos: WirePos::from_tiles(Vec2::new(x, y)),
            facing: WireAngle(0),
        }
    }

    #[test]
    fn an_arrow_flies_in_a_straight_line_between_two_snapshots() {
        let mut older = snapshot(10);
        older.projectiles = vec![arrow(1, 4.0, 4.0), arrow(2, 8.0, 9.0)];
        let mut newer = snapshot(11);
        newer.projectiles = vec![arrow(1, 4.7, 4.0), arrow(2, 8.0, 9.7)];
        let buffer = buffer_with(vec![older, newer]);

        let frame = buffer.sample_at(10.5).unwrap();

        assert_eq!(frame.projectiles.len(), 2);
        assert!(close(frame.projectiles[0].pos, Vec2::new(4.35, 4.0)));
        assert!(close(frame.projectiles[1].pos, Vec2::new(8.0, 9.35)));
        assert!(close(frame.projectiles[0].facing, Vec2::new(1.0, 0.0)));
    }

    #[test]
    fn an_arrow_that_ended_stays_where_it_ended_until_playback_passes_the_snapshot_without_it() {
        let mut older = snapshot(10);
        older.projectiles = vec![arrow(1, 6.0, 4.0)];
        let newer = snapshot(11);
        let buffer = buffer_with(vec![older, newer]);

        let before = buffer.sample_at(10.9).unwrap();
        assert_eq!(before.projectiles.len(), 1);
        assert!(close(before.projectiles[0].pos, Vec2::new(6.0, 4.0)));

        let after = buffer.sample_at(11.0).unwrap();
        assert!(after.projectiles.is_empty());
    }

    #[test]
    fn an_arrow_that_is_only_in_the_newer_snapshot_appears_when_playback_reaches_it() {
        let older = snapshot(10);
        let mut newer = snapshot(11);
        newer.projectiles = vec![arrow(1, 4.7, 4.0)];
        let buffer = buffer_with(vec![older, newer]);

        assert!(buffer.sample_at(10.9).unwrap().projectiles.is_empty());
        assert_eq!(buffer.sample_at(11.0).unwrap().projectiles.len(), 1);
    }

    #[test]
    fn a_weapon_switch_shows_when_playback_reaches_the_snapshot_that_carries_it() {
        let mut older = snapshot(10);
        older.players = vec![player(1, 8.0, 8.0, 100)];
        let mut newer = snapshot(11);
        newer.players = vec![player(1, 8.0, 8.0, 100)];
        newer.players[0].loadout = WireLoadout {
            held: WireWeaponKind::Bow,
            spare: Some(WireWeaponKind::Sword),
        };
        let buffer = buffer_with(vec![older, newer]);

        let between = buffer.sample_at(10.9).unwrap();
        assert_eq!(between.players[0].loadout.held, WireWeaponKind::Sword);

        let after = buffer.sample_at(11.0).unwrap();
        assert_eq!(after.players[0].loadout.held, WireWeaponKind::Bow);
        assert_eq!(after.players[0].loadout.spare, Some(WireWeaponKind::Sword));
    }

    #[test]
    fn a_monster_missing_from_the_newer_snapshot_stays_until_playback_passes_it() {
        let mut older = snapshot(10);
        older.monsters = vec![monster(1, 4.0, 4.0, 60), monster(2, 7.0, 7.0, 60)];
        let mut newer = snapshot(11);
        newer.monsters = vec![monster(2, 7.0, 8.0, 60)];
        let buffer = buffer_with(vec![older, newer]);

        let before = buffer.sample_at(10.9).unwrap();
        assert_eq!(before.monsters.len(), 2);
        assert!(close(before.monsters[0].pos, Vec2::new(4.0, 4.0)));

        let after = buffer.sample_at(11.0).unwrap();
        assert_eq!(after.monsters.len(), 1);
        assert_eq!(after.monsters[0].id, 2);
    }

    #[test]
    fn a_monster_only_in_the_newer_snapshot_appears_when_playback_reaches_it() {
        let older = snapshot(10);
        let mut newer = snapshot(11);
        newer.monsters = vec![monster(5, 3.0, 3.0, 60)];
        let buffer = buffer_with(vec![older, newer]);

        assert!(buffer.sample_at(10.9).unwrap().monsters.is_empty());
        assert_eq!(buffer.sample_at(11.0).unwrap().monsters.len(), 1);
    }

    #[test]
    fn positions_are_never_blended_across_a_new_expedition() {
        let mut older = snapshot(10);
        older.players = vec![player(1, 2.0, 2.0, 100)];
        let mut newer = snapshot(11);
        newer.expedition = 2;
        newer.players = vec![player(1, 20.0, 20.0, 100)];
        let buffer = buffer_with(vec![older, newer]);

        let middle = buffer.sample_at(10.5).unwrap();
        assert_eq!(middle.expedition, 1);
        assert!(close(middle.players[0].pos, Vec2::new(2.0, 2.0)));

        let after = buffer.sample_at(11.0).unwrap();
        assert_eq!(after.expedition, 2);
        assert!(close(after.players[0].pos, Vec2::new(20.0, 20.0)));
    }

    #[test]
    fn positions_are_never_blended_from_the_result_into_the_camp() {
        let mut older = snapshot(10);
        older.phase = WirePhase::Debrief {
            outcome: caravan_protocol::WireOutcome::Defeat,
            secs_left: 1.0,
            confirmed: vec![1],
        };
        older.players = vec![player(1, 40.0, 29.5, 100)];
        let mut newer = snapshot(11);
        newer.phase = WirePhase::Camp {
            ready: Vec::new(),
            starting_in: None,
            struck: Vec::new(),
        };
        newer.players = vec![player(1, 32.0, 29.5, 100)];
        assert_eq!(older.expedition, newer.expedition);
        let buffer = buffer_with(vec![older, newer]);

        let middle = buffer.sample_at(10.5).unwrap();
        assert!(close(middle.players[0].pos, Vec2::new(40.0, 29.5)));

        let after = buffer.sample_at(11.0).unwrap();
        assert!(close(after.players[0].pos, Vec2::new(32.0, 29.5)));
    }

    #[test]
    fn facing_turns_along_the_shorter_arc() {
        let mut older = snapshot(10);
        let mut a = monster(1, 4.0, 4.0, 60);
        a.facing = WireAngle(250);
        older.monsters = vec![a];
        let mut newer = snapshot(11);
        let mut b = monster(1, 4.0, 4.0, 60);
        b.facing = WireAngle(6);
        newer.monsters = vec![b];
        let buffer = buffer_with(vec![older, newer]);

        let facing = buffer.sample_at(10.5).unwrap().monsters[0].facing;

        // 250과 6의 가운데는 짧은 호로 돌면 256(= 0, 동쪽)이고, 긴 호로 돌면 128(서쪽)입니다.
        assert!(close(facing, Vec2::new(1.0, 0.0)), "got {facing:?}");
    }

    #[test]
    fn nothing_is_drawn_ahead_of_the_newest_snapshot() {
        let mut older = snapshot(10);
        older.monsters = vec![monster(1, 4.0, 4.0, 60)];
        let mut newer = snapshot(11);
        newer.monsters = vec![monster(1, 5.0, 4.0, 60)];
        let buffer = buffer_with(vec![older, newer]);

        let frame = buffer.sample_at(50.0).unwrap();

        assert!(close(frame.monsters[0].pos, Vec2::new(5.0, 4.0)));
        assert!((frame.tick - 11.0).abs() < 1e-9);
    }

    #[test]
    fn before_the_oldest_snapshot_the_oldest_is_drawn_as_it_is() {
        let mut older = snapshot(10);
        older.monsters = vec![monster(1, 4.0, 4.0, 60)];
        let mut newer = snapshot(11);
        newer.monsters = vec![monster(1, 5.0, 4.0, 60)];
        let buffer = buffer_with(vec![older, newer]);

        let frame = buffer.sample_at(3.0).unwrap();

        assert!(close(frame.monsters[0].pos, Vec2::new(4.0, 4.0)));
        assert!((frame.tick - 10.0).abs() < 1e-9);
    }

    #[test]
    fn an_older_or_repeated_snapshot_is_ignored() {
        let mut buffer = SnapshotBuffer::new(20);
        let mut first = snapshot(12);
        first.monsters = vec![monster(1, 4.0, 4.0, 60)];
        buffer.push(first);
        let mut late = snapshot(11);
        late.monsters = vec![monster(1, 9.0, 9.0, 60)];
        buffer.push(late);
        let mut repeat = snapshot(12);
        repeat.monsters = vec![monster(1, 9.0, 9.0, 60)];
        buffer.push(repeat);

        assert_eq!(buffer.newest().unwrap().tick, 12);
        let frame = buffer.sample_at(12.0).unwrap();
        assert!(close(frame.monsters[0].pos, Vec2::new(4.0, 4.0)));
    }

    /// 스냅샷이 20Hz로 도착하고 프레임이 60Hz로 돌 때를 흉내 냅니다. 마지막 프레임까지 진행한 버퍼와 최신 Tick을 돌려줍니다.
    fn run_steady(seconds: f64) -> (SnapshotBuffer, u64) {
        let mut buffer = SnapshotBuffer::new(20);
        let frame_dt = 1.0 / 60.0;
        let mut now = 0.0;
        let mut next_arrival = 0.0;
        let mut tick = 0;
        while now < seconds {
            if now >= next_arrival {
                tick += 1;
                buffer.push(snapshot(tick));
                next_arrival += 1.0 / 20.0;
            }
            buffer.advance(frame_dt);
            now += frame_dt;
        }
        (buffer, tick)
    }

    #[test]
    fn the_playback_clock_settles_two_ticks_behind_the_newest_snapshot() {
        let (buffer, newest) = run_steady(5.0);

        let behind = newest as f64 - buffer.playback.unwrap();

        // 스냅샷이 계단식으로 도착하고 재생 시각은 매끄럽게 가므로 절반 Tick 안팎으로 흔들립니다.
        assert!((behind - 2.0).abs() < 0.6, "behind {behind}");
        assert!(
            buffer.snapshots.len() <= 4,
            "snapshots the clock has passed are dropped"
        );
        assert!(buffer.frame().is_some());
    }

    #[test]
    fn a_small_drift_is_eased_out_instead_of_jumped() {
        let (mut buffer, newest) = run_steady(5.0);
        let before = buffer.playback.unwrap();

        // 두 Tick 분량이 한꺼번에 도착한 것처럼 보이게 합니다.
        buffer.push(snapshot(newest + 2));
        buffer.advance(1.0 / 60.0);

        let moved = buffer.playback.unwrap() - before;
        assert!(
            moved > 1.0 / 3.0 && moved < 1.0,
            "moved {moved} ticks in one frame"
        );
    }

    #[test]
    fn a_large_drift_jumps_straight_to_the_target() {
        let (mut buffer, newest) = run_steady(5.0);

        // 2초 동안 멈췄다가 다시 도착한 것처럼 보이게 합니다.
        buffer.push(snapshot(newest + 40));
        buffer.advance(1.0 / 60.0);

        let expected = (newest + 40) as f64 - INTERP_DELAY_TICKS;
        assert_eq!(buffer.playback, Some(expected));
    }

    /// 스냅샷이 멈추는 동안(접속한 사람이 없어서 방이 멈춥니다) 그림이 뒤로 가지 않고, 서버가 같은 Tick에서 이어 보내면 그림도 이어집니다.
    /// 이어지는 스냅샷은 멈추기 전 Tick 바로 다음 Tick입니다. 멈춘 동안 서버의 Tick 번호도 멈추기 때문입니다.
    #[test]
    fn a_pause_in_the_snapshots_never_runs_the_picture_backwards_and_the_resume_continues_it() {
        let frame_dt = 1.0 / 60.0;
        let (mut buffer, mut newest) = run_steady(2.0);
        let mut last_shown = buffer.frame().unwrap().tick;
        let check = |buffer: &mut SnapshotBuffer, last_shown: &mut f64, what: &str| {
            buffer.advance(frame_dt);
            let shown = buffer.frame().unwrap().tick;
            assert!(
                shown >= *last_shown - 1e-9,
                "{what}: {last_shown} -> {shown}"
            );
            *last_shown = shown;
        };

        for frame in 0..(5 * 60) {
            check(
                &mut buffer,
                &mut last_shown,
                &format!("pause frame {frame}"),
            );
        }
        let held = last_shown;
        assert!(
            (held - newest as f64).abs() < 1e-9,
            "the picture rests on the newest snapshot, {held} vs {newest}"
        );

        let mut next_arrival = 0.0;
        let mut now = 0.0;
        while now < 3.0 {
            if now >= next_arrival {
                newest += 1;
                buffer.push(snapshot(newest));
                next_arrival += 1.0 / 20.0;
            }
            check(
                &mut buffer,
                &mut last_shown,
                &format!("resume at {now:.2}s"),
            );
            now += frame_dt;
        }

        let behind = newest as f64 - buffer.playback.unwrap();
        assert!((behind - 2.0).abs() < 0.6, "settled {behind} ticks behind");
    }

    #[test]
    fn effects_run_from_zero_to_one_and_then_end() {
        assert_eq!(effect_progress(10.0, 10, 4.0), Some(0.0));
        assert_eq!(effect_progress(12.0, 10, 4.0), Some(0.5));
        assert_eq!(effect_progress(14.0, 10, 4.0), None);
        assert_eq!(effect_progress(9.0, 10, 4.0), None);
    }
}
