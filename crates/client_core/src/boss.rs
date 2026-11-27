//! 보스가 화면에 나타나는 모습을 정합니다. 무엇을 그릴지(보스의 몸과 HP, 노려지는 플레이어, 위험한 영역과 진행도)만 정하고 그리는 일은 Bevy 쪽이 합니다.
//!
//! 모든 값은 지금 그리는 `Frame`에서 오므로, 모든 플레이어가 같은 시각에 같은 것을 봅니다. 영역의 자리와 진행도는 서버가 정한 값이고 클라이언트는 계산하지 않습니다.

use caravan_protocol::{
    WireAim, WireBoss, WireBossAct, WireMonsterKind, WireNightStep, WirePhase, WorldInfo,
};
use caravan_rules::Vec2;

use crate::hud::{Meter, Tone, bar_fraction};
use crate::interp::{Frame, MonsterView};

/// 지금 그리는 프레임의 보스입니다. 몸(위치와 HP)은 몬스터 목록에서 오고, 최대 HP와 노리는 대상과 하는 일은 단계에서 옵니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BossView {
    pub body: MonsterView,
    pub state: WireBoss,
}

/// 위험한 영역입니다. `fill`은 0.0에서 1.0으로 차오르는 진행도이고 1.0이 되는 순간 피해가 납니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Danger {
    /// 내려찍기의 원입니다. 지면 좌표(타일)입니다.
    Circle {
        center: Vec2,
        radius: f32,
        fill: f32,
    },
}

/// 지금 프레임에 보스와 싸우는 중이고 보스의 몸이 있으면 그 보스입니다.
pub fn boss_view(frame: &Frame) -> Option<BossView> {
    let WirePhase::Night {
        step: WireNightStep::Boss(state),
        ..
    } = &frame.phase
    else {
        return None;
    };
    let body = frame
        .monsters
        .iter()
        .find(|monster| monster.kind == WireMonsterKind::Boss)?;
    Some(BossView {
        body: *body,
        state: *state,
    })
}

/// 보스가 나타나기를 기다리는 중이면 남은 시간(초, 올림)입니다.
pub fn warning_secs(frame: &Frame) -> Option<u32> {
    match &frame.phase {
        WirePhase::Night {
            step: WireNightStep::Warning { secs_left },
            ..
        } => Some(secs_left.ceil().max(0.0) as u32),
        _ => None,
    }
}

impl BossView {
    /// 화면 위쪽 가운데의 HP 막대입니다.
    pub fn meter(&self) -> Meter {
        Meter {
            label: format!("BOSS  {} / {}", self.body.hp, self.state.max_hp),
            fraction: bar_fraction(self.body.hp, self.state.max_hp),
            tone: Tone::Boss,
        }
    }

    /// 보스가 노리는 플레이어의 번호입니다. 마차를 노리면 없습니다. 화면은 그 플레이어의 머리 위에 표시를 그립니다.
    pub fn aimed_player(&self) -> Option<u32> {
        match self.state.aim {
            WireAim::Player(id) => Some(id),
            WireAim::Wagon => None,
        }
    }

    /// 지금 그릴 위험한 영역입니다. 걷는 중이면 없습니다. 크기는 입장 안내에서 읽고, 서버가 보스의 수치를 보내지 않았으면 영역이 없습니다.
    pub fn danger(&self, world: &WorldInfo) -> Option<Danger> {
        let info = world.boss?;
        match self.state.act {
            WireBossAct::Walking => None,
            WireBossAct::Slam { center, progress } => Some(Danger::Circle {
                center: center.to_tiles(),
                radius: info.slam_radius,
                fill: progress,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::{
        BossInfo, RoomCode, SeatToken, Welcome, WireMonsterEffects, WirePos, WireWagon,
    };
    use caravan_rules::{Seed, Sim, Tuning};

    fn world() -> WorldInfo {
        let mut sim = Sim::new(Tuning::default(), Seed(1));
        let id = sim.add_player().unwrap();
        let room = RoomCode::parse("test").unwrap();
        Welcome::from_sim(&sim, sim.player(id).unwrap(), &room, SeatToken([7; 16])).world
    }

    fn state(act: WireBossAct) -> WireBoss {
        WireBoss {
            max_hp: 3000,
            aim: WireAim::Player(2),
            act,
        }
    }

    fn frame(step: WireNightStep, boss_hp: Option<u32>) -> Frame {
        Frame {
            tick: 1.0,
            expedition: 1,
            phase: WirePhase::Night {
                day: 3,
                wave_count: 3,
                step,
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
            monsters: boss_hp
                .map(|hp| MonsterView {
                    id: 9,
                    kind: WireMonsterKind::Boss,
                    effects: WireMonsterEffects::NONE,
                    pos: Vec2::new(30.0, 40.0),
                    facing: Vec2::new(1.0, 0.0),
                    hp,
                })
                .into_iter()
                .collect(),
            projectiles: Vec::new(),
        }
    }

    fn boss_frame(act: WireBossAct, hp: u32) -> Frame {
        frame(WireNightStep::Boss(state(act)), Some(hp))
    }

    #[test]
    fn the_warning_counts_down_in_whole_seconds_rounded_up() {
        let waiting = frame(WireNightStep::Warning { secs_left: 4.05 }, None);
        let just_before = frame(WireNightStep::Warning { secs_left: 0.05 }, None);

        assert_eq!(warning_secs(&waiting), Some(5));
        assert_eq!(warning_secs(&just_before), Some(1));
        assert_eq!(warning_secs(&boss_frame(WireBossAct::Walking, 3000)), None);
    }

    #[test]
    fn the_boss_is_seen_only_in_the_boss_step_and_only_with_a_body() {
        assert!(boss_view(&boss_frame(WireBossAct::Walking, 3000)).is_some());
        assert!(
            boss_view(&frame(
                WireNightStep::Boss(state(WireBossAct::Walking)),
                None
            ))
            .is_none()
        );
        assert!(boss_view(&frame(WireNightStep::Fighting { wave: 3 }, Some(3000))).is_none());
    }

    #[test]
    fn the_bar_shows_the_hp_of_the_drawn_body_against_the_max_hp_of_this_party() {
        let view = boss_view(&boss_frame(WireBossAct::Walking, 1800)).unwrap();

        let meter = view.meter();

        assert_eq!(meter.label, "BOSS  1800 / 3000");
        assert!((meter.fraction - 0.6).abs() < 1e-6);
        assert_eq!(meter.tone, Tone::Boss);
    }

    #[test]
    fn everyone_sees_who_the_boss_aims_at_and_nobody_when_it_aims_at_the_wagon() {
        let mut view = boss_view(&boss_frame(WireBossAct::Walking, 3000)).unwrap();
        assert_eq!(view.aimed_player(), Some(2));

        view.state.aim = WireAim::Wagon;

        assert_eq!(view.aimed_player(), None);
    }

    #[test]
    fn a_slam_is_a_circle_of_the_welcome_radius_that_fills_with_the_progress() {
        let slam = WireBossAct::Slam {
            center: WirePos { x: 640, y: 1280 },
            progress: 0.25,
        };
        let view = boss_view(&boss_frame(slam, 3000)).unwrap();

        assert_eq!(
            view.danger(&world()),
            Some(Danger::Circle {
                center: Vec2::new(5.0, 10.0),
                radius: 2.5,
                fill: 0.25,
            })
        );
    }

    #[test]
    fn a_walking_boss_has_no_danger() {
        let walking = boss_view(&boss_frame(WireBossAct::Walking, 3000)).unwrap();

        assert_eq!(walking.danger(&world()), None);
    }

    #[test]
    fn a_server_without_a_boss_sends_no_sizes_and_so_nothing_is_drawn() {
        let mut world = world();
        world.boss = None::<BossInfo>;
        let slam = WireBossAct::Slam {
            center: WirePos { x: 0, y: 0 },
            progress: 0.5,
        };

        let view = boss_view(&boss_frame(slam, 3000)).unwrap();

        assert_eq!(view.danger(&world), None);
    }
}
