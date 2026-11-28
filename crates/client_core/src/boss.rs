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

/// 위험한 영역입니다. `fill`은 0.0에서 1.0으로 차오르는 진행도이고, 1.0이 된 다음 Tick에 피해가 나거나 행동이 시작됩니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Danger {
    /// 내려찍기의 원입니다. 지면 좌표(타일)입니다.
    Circle {
        center: Vec2,
        radius: f32,
        fill: f32,
    },
    /// 돌진의 띠입니다. `from`에서 `to`까지의 선분이고 폭은 `width`입니다. 보스가 달리는 동안에는 `fill`이 1.0입니다.
    /// 네 모서리는 `lane_corners`가 구합니다.
    Lane {
        from: Vec2,
        to: Vec2,
        width: f32,
        fill: f32,
    },
    /// 졸개를 부르는 포효입니다. 보스가 선 자리(`center`)에서 퍼져 나가는 고리를 그립니다. 영역이 아니고 피해가 없습니다.
    Roar { center: Vec2, fill: f32 },
}

/// 띠의 네 모서리(지면 좌표)입니다. `from`에서 `to`로 가는 선의 양옆으로 폭의 절반씩 벗어난 점이고, 순서대로 이으면 띠의 테두리가 됩니다.
/// 차오르는 안쪽은 `to` 대신 `from`과 `to` 사이의 점을 넣어 같은 모양으로 구합니다.
pub fn lane_corners(from: Vec2, to: Vec2, width: f32) -> [Vec2; 4] {
    let along = (to - from).normalize_or_zero();
    let side = Vec2::new(-along.y, along.x) * (width * 0.5);
    [from + side, to + side, to - side, from - side]
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
            WireBossAct::Charge { from, to, progress } => Some(Danger::Lane {
                from: from.to_tiles(),
                to: to.to_tiles(),
                width: info.charge_width,
                fill: progress,
            }),
            WireBossAct::Dash { from, to } => Some(Danger::Lane {
                from: from.to_tiles(),
                to: to.to_tiles(),
                width: info.charge_width,
                fill: 1.0,
            }),
            WireBossAct::Roar { progress } => Some(Danger::Roar {
                center: self.body.pos,
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
    fn a_charge_is_a_lane_that_fills_while_winding_up_and_is_full_while_the_boss_runs() {
        let (from, to) = (WirePos { x: 128, y: 128 }, WirePos { x: 1408, y: 128 });
        let winding = boss_view(&boss_frame(
            WireBossAct::Charge {
                from,
                to,
                progress: 0.5,
            },
            3000,
        ))
        .unwrap();
        let running = boss_view(&boss_frame(WireBossAct::Dash { from, to }, 3000)).unwrap();

        let lane = |fill| {
            Some(Danger::Lane {
                from: Vec2::new(1.0, 1.0),
                to: Vec2::new(11.0, 1.0),
                width: 2.0,
                fill,
            })
        };
        assert_eq!(winding.danger(&world()), lane(0.5));
        assert_eq!(running.danger(&world()), lane(1.0));
    }

    #[test]
    fn the_four_corners_of_a_lane_are_half_its_width_to_either_side_of_its_line() {
        let corners = lane_corners(Vec2::new(2.0, 3.0), Vec2::new(12.0, 3.0), 2.0);

        assert_eq!(
            corners,
            [
                Vec2::new(2.0, 4.0),
                Vec2::new(12.0, 4.0),
                Vec2::new(12.0, 2.0),
                Vec2::new(2.0, 2.0),
            ]
        );
        let slanted = lane_corners(Vec2::new(0.0, 0.0), Vec2::new(6.0, 8.0), 2.0);
        assert!(
            (slanted[0].distance(slanted[3]) - 2.0).abs() < 1e-5,
            "the width"
        );
        assert!(
            (slanted[0].distance(slanted[1]) - 10.0).abs() < 1e-5,
            "the length"
        );
        assert!(
            (slanted[0] - slanted[3]).dot(slanted[1] - slanted[0]).abs() < 1e-4,
            "the ends are square to the line"
        );
    }

    #[test]
    fn a_roar_is_a_ring_around_the_boss_that_fills_with_the_progress() {
        let roaring = boss_view(&boss_frame(WireBossAct::Roar { progress: 0.75 }, 3000)).unwrap();

        assert_eq!(
            roaring.danger(&world()),
            Some(Danger::Roar {
                center: Vec2::new(30.0, 40.0),
                fill: 0.75,
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

    #[test]
    fn a_server_without_a_boss_sends_no_sizes_for_a_lane_or_a_roar_either() {
        let mut world = world();
        world.boss = None::<BossInfo>;
        let (from, to) = (WirePos { x: 0, y: 0 }, WirePos { x: 640, y: 0 });

        for act in [
            WireBossAct::Charge {
                from,
                to,
                progress: 0.5,
            },
            WireBossAct::Dash { from, to },
            WireBossAct::Roar { progress: 0.5 },
        ] {
            let view = boss_view(&boss_frame(act, 3000)).unwrap();
            assert_eq!(view.danger(&world), None, "{act:?}");
        }
    }
}
