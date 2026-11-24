//! 화면 위에 표시할 내용(HUD)을 정하는 곳입니다. 문구, 막대의 채움 비율, 색의 종류만 정하고, 그리는 일은 Bevy 쪽이 합니다.
//!
//! 문구는 영어입니다. Bevy의 기본 글꼴이 영문만 표시하고, 글꼴 파일을 따로 넣지 않기 때문입니다.
//! 모든 값은 지금 그리는 `Frame`에서 오므로, 막대는 그 피해를 일으킨 공격이 화면에 나타난 뒤에 줄어듭니다.

use caravan_protocol::{
    WireBuildRefusal, WireCannotBuild, WireInteraction, WireLife, WireNightStep, WireOutcome,
    WirePhase, WireRepairTarget, WireTowerState, WireWeaponKind,
};
use caravan_rules::Team;

use crate::interp::{Frame, PlayerView};
use crate::session::Session;

/// 한 프레임에 표시할 모든 것입니다. 없는 값은 `None`이거나 비어 있고, Bevy 쪽은 그 칸을 숨깁니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Hud {
    /// 접속 상태 한 줄입니다. 언제나 있습니다.
    pub connection: String,
    pub room: Option<String>,
    pub expedition: Option<String>,
    /// 「DAY 1 / 3」이나 「NIGHT 2 / 3」처럼 지금이 몇 번째 날의 낮인지 밤인지입니다. 결과를 보여 주는 동안에는 없습니다.
    pub day: Option<String>,
    /// 낮의 남은 시간, 밤의 무리 번호, 새벽이 끝나기까지의 시간입니다. 결과를 보여 주는 동안에는 없습니다.
    pub timer: Option<String>,
    /// 모든 플레이어가 함께 쓰는 보관함의 자원입니다.
    pub stash: Option<String>,
    /// 내가 들고 있는 자원과 들 수 있는 상한입니다. 살아 있을 때만 있습니다.
    pub carry: Option<String>,
    /// 내가 들고 있는 무기와 바꾸는 키입니다. 내 캐릭터가 화면에 있으면 항상 있고, 쓰러졌어도 무기는 들고 있습니다.
    pub weapon: Option<String>,
    /// 화면 가운데에 크게 띄우는 안내입니다. 원정의 결과, 입장 거절, 접속이 끊김이 여기에 나옵니다.
    pub banner: Option<Banner>,
    pub me: Option<Meter>,
    /// 내 달리기 게이지입니다. 살아 있을 때만 있습니다.
    pub stamina: Option<Meter>,
    pub wagon: Option<Meter>,
    pub allies: Vec<Meter>,
    /// 부활, 수리, 건설이 진행되는 동안 화면 아래에 띄우는 진행 막대입니다.
    pub action: Option<Meter>,
    /// 쓰러진 동안의 출혈 시간 안내입니다.
    pub bleed: Option<String>,
    /// 건설 키를 누르고 있지만 세울 수 없을 때, 또는 수리하는 중인데 보관함이 비용을 낼 수 없을 때의 이유입니다.
    pub refusal: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Meter {
    pub label: String,
    /// 0.0에서 1.0 사이의 채움 비율입니다.
    pub fraction: f32,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Banner {
    pub title: String,
    pub detail: Option<String>,
    pub tone: BannerTone,
}

/// 막대의 색을 고르는 종류입니다. 실제 색은 Bevy 쪽이 정합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Health,
    Wagon,
    Progress,
    Stamina,
    Downed,
    /// 사망해서 더 이상 채울 것이 없는 상태입니다.
    Gone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerTone {
    Info,
    Good,
    Bad,
}

impl Hud {
    /// 접속 상태와 지금 그릴 방의 모습에서 HUD에 표시할 내용을 만듭니다. 방에 들어가기 전에는 `frame`이 없습니다.
    pub fn of(session: &Session, frame: Option<&Frame>) -> Hud {
        let mut hud = Hud {
            connection: session.status_text(),
            room: None,
            expedition: None,
            day: None,
            timer: None,
            stash: None,
            carry: None,
            weapon: None,
            banner: None,
            me: None,
            stamina: None,
            wagon: None,
            allies: Vec::new(),
            action: None,
            bleed: None,
            refusal: None,
        };
        if let Some(joined) = session.joined() {
            hud.room = Some(format!("ROOM {}", joined.room));
        }
        hud.banner = session_banner(session, frame);

        let (Some(joined), Some(frame)) = (session.joined(), frame) else {
            return hud;
        };
        let you = joined.welcome.you;
        let my_team = Team(joined.welcome.team);
        hud.expedition = Some(format!("EXPEDITION {}", frame.expedition));
        (hud.day, hud.timer) = phase_text(frame.phase, joined.welcome.world.day_count);
        hud.stash = Some(format!("STASH  {}", frame.stash));
        if let WirePhase::Over {
            outcome, secs_left, ..
        } = frame.phase
        {
            hud.banner = Some(result_banner(outcome, secs_left));
        }
        hud.wagon = Some(Meter {
            label: format!("WAGON  {} / {}", frame.wagon.hp, frame.wagon.max_hp),
            fraction: bar_fraction(frame.wagon.hp, frame.wagon.max_hp),
            tone: Tone::Wagon,
        });
        hud.allies = frame
            .players
            .iter()
            .filter(|player| player.id != you && player.team.is_allied_with(my_team))
            .map(ally_meter)
            .collect();

        if let Some(me) = frame.player(you) {
            hud.me = Some(my_meter(me));
            hud.weapon = Some(weapon_text(me.weapon));
            hud.stamina = stamina_meter(me);
            hud.action = action_meter(me, frame, joined.welcome.world.node_amount);
            if let WireLife::Alive {
                carried,
                interaction,
                build_refusal,
                ..
            } = me.life
            {
                hud.carry = Some(carry_text(carried, joined.welcome.world.carry_capacity));
                hud.refusal = build_refusal
                    .map(refusal_text)
                    .or_else(|| repair_refusal_text(interaction));
            }
            if let WireLife::Downed {
                bleed_secs_left,
                revive_progress,
            } = me.life
            {
                let secs = whole_secs(bleed_secs_left);
                hud.bleed = Some(if revive_progress.is_some() {
                    format!("DOWNED  BEING REVIVED  ({secs}s LEFT)")
                } else {
                    format!("DOWNED  BLEEDING OUT IN {secs}s")
                });
            }
        }
        hud
    }
}

/// 방에 들어가기 전, 거절, 접속이 끊긴 뒤에 화면 가운데에 띄울 안내입니다.
/// 이유가 길 수 있으므로 짧은 제목을 크게, 이유를 그 아래에 작게 나눠서 보여 줍니다.
fn session_banner(session: &Session, frame: Option<&Frame>) -> Option<Banner> {
    if let Some(problem) = session.problem() {
        return Some(Banner {
            title: problem.heading.to_uppercase(),
            detail: Some(problem.reason),
            tone: BannerTone::Bad,
        });
    }
    match session {
        Session::InRoom(_) if frame.is_some() => None,
        Session::InRoom(_) => Some(Banner {
            title: "Waiting for the first snapshot...".to_string(),
            detail: None,
            tone: BannerTone::Info,
        }),
        _ => Some(Banner {
            title: session.status_text(),
            detail: None,
            tone: BannerTone::Info,
        }),
    }
}

fn result_banner(outcome: WireOutcome, secs_left: f32) -> Banner {
    let (title, tone) = match outcome {
        WireOutcome::Victory => ("EXPEDITION SUCCESS", BannerTone::Good),
        WireOutcome::Defeat => ("EXPEDITION FAILED", BannerTone::Bad),
    };
    Banner {
        title: title.to_string(),
        detail: Some(format!("Next expedition in {}s", whole_secs(secs_left))),
        tone,
    }
}

/// 화면 오른쪽 위에 적는 두 줄입니다. 첫 줄은 몇 번째 날의 낮인지 밤인지이고, 둘째 줄은 거기서 기다리는 것입니다.
fn phase_text(phase: WirePhase, day_count: u32) -> (Option<String>, Option<String>) {
    let (day, timer) = match phase {
        WirePhase::Day { day, secs_left } => (
            format!("DAY {day} / {day_count}"),
            format!("NIGHT IN {}s", whole_secs(secs_left)),
        ),
        WirePhase::Night {
            day,
            wave_count,
            step,
        } => (
            format!("NIGHT {day} / {day_count}"),
            match step {
                WireNightStep::Waiting {
                    next_wave,
                    secs_left,
                } => format!(
                    "WAVE {next_wave} / {wave_count} IN {}s",
                    whole_secs(secs_left)
                ),
                WireNightStep::Fighting { wave } => format!("WAVE {wave} / {wave_count}"),
            },
        ),
        WirePhase::Dawn {
            next_day,
            secs_left,
        } => (
            "DAWN".to_string(),
            format!("DAY {next_day} / {day_count} IN {}s", whole_secs(secs_left)),
        ),
        WirePhase::Over { .. } => return (None, None),
    };
    (Some(day), Some(timer))
}

fn my_meter(me: &PlayerView) -> Meter {
    match me.life {
        WireLife::Alive { hp, max_hp, .. } => Meter {
            label: format!("YOU  {hp} / {max_hp}"),
            fraction: bar_fraction(hp, max_hp),
            tone: Tone::Health,
        },
        WireLife::Downed { .. } => Meter {
            label: "YOU  DOWNED".to_string(),
            fraction: 0.0,
            tone: Tone::Downed,
        },
        WireLife::Dead => Meter {
            label: "YOU  DEAD".to_string(),
            fraction: 0.0,
            tone: Tone::Gone,
        },
    }
}

/// 내 달리기 게이지입니다. 서버가 백분율을 보내므로 100에 대한 비율입니다. 쓰러졌거나 사망했으면 게이지가 없습니다.
fn stamina_meter(me: &PlayerView) -> Option<Meter> {
    match me.life {
        WireLife::Alive { stamina, .. } => Some(Meter {
            label: "STAMINA".to_string(),
            fraction: bar_fraction(u32::from(stamina), 100),
            tone: Tone::Stamina,
        }),
        WireLife::Downed { .. } | WireLife::Dead => None,
    }
}

fn ally_meter(ally: &PlayerView) -> Meter {
    let name = format!("P{}", ally.id);
    match ally.life {
        WireLife::Alive { hp, max_hp, .. } => Meter {
            label: format!("{name}  {hp} / {max_hp}"),
            fraction: bar_fraction(hp, max_hp),
            tone: Tone::Health,
        },
        WireLife::Downed {
            bleed_secs_left,
            revive_progress,
        } => Meter {
            label: match revive_progress {
                Some(progress) => format!("{name}  REVIVING {}%", percent(progress)),
                None => format!("{name}  DOWNED {}s", whole_secs(bleed_secs_left)),
            },
            fraction: revive_progress.unwrap_or(0.0),
            tone: Tone::Downed,
        },
        WireLife::Dead => Meter {
            label: format!("{name}  DEAD"),
            fraction: 0.0,
            tone: Tone::Gone,
        },
    }
}

/// 내가 부활, 수리, 채집, 건설을 하고 있거나 부활당하고 있을 때의 진행 막대입니다.
/// 수리에는 진행도가 따로 없으므로, 수리하는 감시탑이나 마차의 HP가 차오르는 모습을 진행도로 보여 줍니다.
/// 채집도 마찬가지로 채집하는 지점의 남은 자원(`node_amount`가 가득 찬 양)이 줄어드는 모습을 보여 줍니다.
fn action_meter(me: &PlayerView, frame: &Frame, node_amount: u32) -> Option<Meter> {
    match me.life {
        WireLife::Alive {
            interaction: Some(WireInteraction::Reviving { target, progress }),
            ..
        } => Some(Meter {
            label: format!("REVIVING P{target}"),
            fraction: progress,
            tone: Tone::Progress,
        }),
        WireLife::Alive {
            interaction:
                Some(WireInteraction::Repairing {
                    target: WireRepairTarget::Wagon,
                    ..
                }),
            ..
        } => Some(Meter {
            label: format!(
                "REPAIRING WAGON  {} / {}",
                frame.wagon.hp, frame.wagon.max_hp
            ),
            fraction: bar_fraction(frame.wagon.hp, frame.wagon.max_hp),
            tone: Tone::Progress,
        }),
        WireLife::Alive {
            interaction:
                Some(WireInteraction::Repairing {
                    target: WireRepairTarget::Tower { tower },
                    ..
                }),
            ..
        } => {
            let tower = frame
                .towers
                .iter()
                .find(|candidate| candidate.id == tower)?;
            let WireTowerState::Standing { hp, .. } = tower.state else {
                return None;
            };
            Some(Meter {
                label: format!("REPAIRING TOWER  {hp} / {}", tower.max_hp),
                fraction: bar_fraction(hp, tower.max_hp),
                tone: Tone::Progress,
            })
        }
        WireLife::Alive {
            interaction: Some(WireInteraction::Gathering { node }),
            ..
        } => {
            let node = frame.nodes.iter().find(|candidate| candidate.id == node)?;
            Some(Meter {
                label: format!("GATHERING  {} / {node_amount}", node.amount),
                fraction: bar_fraction(node.amount, node_amount),
                tone: Tone::Progress,
            })
        }
        WireLife::Alive {
            interaction: Some(WireInteraction::Building { tower }),
            ..
        } => {
            let tower = frame
                .towers
                .iter()
                .find(|candidate| candidate.id == tower)?;
            let WireTowerState::Building { progress } = tower.state else {
                return None;
            };
            Some(Meter {
                label: "BUILDING TOWER".to_string(),
                fraction: progress,
                tone: Tone::Progress,
            })
        }
        WireLife::Downed {
            revive_progress: Some(progress),
            ..
        } => Some(Meter {
            label: "BEING REVIVED".to_string(),
            fraction: progress,
            tone: Tone::Progress,
        }),
        _ => None,
    }
}

/// 든 자원과 상한입니다. 상한까지 들면 더 채집할 수 없으므로, 마차로 가져가라는 안내를 붙입니다.
/// 들고 있는 무기의 이름과 바꾸는 키입니다. `match`가 모든 무기를 다루므로, 무기를 추가하면 여기서 컴파일 오류가 납니다.
fn weapon_text(weapon: WireWeaponKind) -> String {
    let name = match weapon {
        WireWeaponKind::Sword => "SWORD",
        WireWeaponKind::Bow => "BOW",
    };
    format!("WEAPON  {name}  [Q]")
}

fn carry_text(carried: u32, capacity: u32) -> String {
    if carried >= capacity {
        format!("CARRY  {carried} / {capacity}  FULL - TAKE IT TO THE WAGON")
    } else {
        format!("CARRY  {carried} / {capacity}")
    }
}

/// 건설이 거절된 이유를 알리는 문구입니다.
fn refusal_text(refusal: WireBuildRefusal) -> String {
    let why = match refusal.why {
        WireCannotBuild::NotEnoughStash => "NOT ENOUGH IN THE STASH",
        WireCannotBuild::TowerLimit => "TOWER LIMIT REACHED",
        WireCannotBuild::OutsideVillage => "TOO FAR FROM THE WAGON",
        WireCannotBuild::TileTaken => "THE TILE IS TAKEN",
        WireCannotBuild::WouldBlockPath => "IT WOULD BLOCK THE WAY TO THE WAGON",
    };
    format!("CANNOT BUILD: {why}")
}

/// 수리하는 중인데 보관함의 자원이 수리의 비용보다 적을 때 알리는 문구입니다.
fn repair_refusal_text(interaction: Option<WireInteraction>) -> Option<String> {
    matches!(
        interaction,
        Some(WireInteraction::Repairing {
            stash_short: true,
            ..
        })
    )
    .then(|| "CANNOT REPAIR: NOT ENOUGH IN THE STASH".to_string())
}

/// 막대의 채움 비율입니다. 화면에 막대를 그리는 곳은 모두 이 함수로 비율을 구합니다.
/// 서버가 보낸 최댓값이 0이어도 나누지 않고 빈 막대(0)로 돌려주며, 값이 최댓값을 넘으면 가득 찬 막대(1)입니다.
pub fn bar_fraction(value: u32, max: u32) -> f32 {
    if max == 0 {
        0.0
    } else {
        (value as f32 / max as f32).clamp(0.0, 1.0)
    }
}

fn percent(fraction: f32) -> u32 {
    (fraction * 100.0).round() as u32
}

/// 남은 시간은 올림해서 보여 줍니다. 0.4초가 남았을 때 「0초」라고 하면 이미 끝난 것처럼 보이기 때문입니다.
fn whole_secs(secs: f32) -> u32 {
    secs.ceil().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use caravan_protocol::{
        ClientEvent, RejectReason, RoomCode, ServerMsg, Welcome, WireNode, WireShot, WireTile,
        WireTower, WireWagon,
    };
    use caravan_rules::{Sim, Team, Tuning, Vec2};

    use crate::interp::PlayerView;
    use crate::launch::DEFAULT_SERVER;

    const YOU: u32 = 1;

    fn session_in_room() -> Session {
        let mut sim = Sim::new(Tuning::default());
        let first = sim.add_player().unwrap();
        let mut welcome = Welcome::from_sim(&sim, sim.player(first).unwrap());
        welcome.you = YOU;
        let mut session =
            Session::new(RoomCode::parse("test").unwrap(), DEFAULT_SERVER.to_string());
        session.on_event(ClientEvent::Connected);
        session.on_event(ClientEvent::Message(ServerMsg::Welcome(welcome)));
        session
    }

    fn alive(hp: u32) -> WireLife {
        WireLife::Alive {
            hp,
            max_hp: 100,
            carried: 0,
            stamina: 100,
            interaction: None,
            build_refusal: None,
        }
    }

    fn player(id: u32, life: WireLife) -> PlayerView {
        PlayerView {
            id,
            team: Team::CARAVAN,
            pos: Vec2::new(5.0, 5.0),
            facing: Vec2::new(1.0, 0.0),
            life,
            last_attack: None,
            weapon: WireWeaponKind::Sword,
        }
    }

    fn frame(players: Vec<PlayerView>) -> Frame {
        Frame {
            tick: 100.0,
            expedition: 2,
            phase: WirePhase::Night {
                day: 2,
                wave_count: 3,
                step: WireNightStep::Fighting { wave: 2 },
            },
            wagon: WireWagon {
                hp: 400,
                max_hp: 500,
            },
            stash: 7,
            nodes: vec![WireNode { id: 1, amount: 9 }],
            piles: Vec::new(),
            towers: vec![WireTower {
                id: 1,
                tile: WireTile { x: 15, y: 11 },
                max_hp: 300,
                state: WireTowerState::Standing {
                    hp: 250,
                    last_shot: None::<WireShot>,
                },
            }],
            players,
            monsters: Vec::new(),
            projectiles: Vec::new(),
        }
    }

    #[test]
    fn the_day_wave_wagon_stash_and_room_are_shown() {
        let session = session_in_room();
        let hud = Hud::of(&session, Some(&frame(vec![player(YOU, alive(72))])));

        assert_eq!(hud.day.as_deref(), Some("NIGHT 2 / 3"));
        assert_eq!(hud.timer.as_deref(), Some("WAVE 2 / 3"));
        assert_eq!(hud.expedition.as_deref(), Some("EXPEDITION 2"));
        assert_eq!(hud.room.as_deref(), Some("ROOM test"));
        assert_eq!(hud.connection, "Connected");
        let wagon = hud.wagon.unwrap();
        assert_eq!(wagon.label, "WAGON  400 / 500");
        assert!((wagon.fraction - 0.8).abs() < 1e-6);
        assert_eq!(hud.stash.as_deref(), Some("STASH  7"));
        assert_eq!(hud.banner, None);
    }

    #[test]
    fn every_step_of_the_day_names_the_day_and_what_comes_next() {
        let session = session_in_room();
        let shown = |phase| {
            let mut frame = frame(vec![player(YOU, alive(100))]);
            frame.phase = phase;
            let hud = Hud::of(&session, Some(&frame));
            (hud.day, hud.timer)
        };
        let text = |day: &str, timer: &str| (Some(day.to_string()), Some(timer.to_string()));

        // 남은 시간은 올림해서 보여 줍니다. 3.2초가 남았을 때 「3초」라고 하면 4번째 초가 끝나기 전에 시작되는 것처럼 보입니다.
        assert_eq!(
            shown(WirePhase::Day {
                day: 1,
                secs_left: 72.2
            }),
            text("DAY 1 / 3", "NIGHT IN 73s")
        );
        assert_eq!(
            shown(WirePhase::Night {
                day: 2,
                wave_count: 3,
                step: WireNightStep::Waiting {
                    next_wave: 3,
                    secs_left: 3.2
                }
            }),
            text("NIGHT 2 / 3", "WAVE 3 / 3 IN 4s")
        );
        assert_eq!(
            shown(WirePhase::Dawn {
                next_day: 3,
                secs_left: 0.4
            }),
            text("DAWN", "DAY 3 / 3 IN 1s")
        );
        assert_eq!(
            shown(WirePhase::Over {
                outcome: WireOutcome::Victory,
                secs_left: 5.0
            }),
            (None, None),
            "the result takes the place of the day"
        );
    }

    #[test]
    fn my_hp_and_my_allies_are_listed_separately() {
        let session = session_in_room();
        let players = vec![
            player(YOU, alive(72)),
            player(2, alive(40)),
            player(
                3,
                WireLife::Downed {
                    bleed_secs_left: 22.4,
                    revive_progress: None,
                },
            ),
            player(4, WireLife::Dead),
        ];
        let hud = Hud::of(&session, Some(&frame(players)));

        let me = hud.me.unwrap();
        assert_eq!(me.label, "YOU  72 / 100");
        assert!((me.fraction - 0.72).abs() < 1e-6);
        let labels: Vec<&str> = hud.allies.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["P2  40 / 100", "P3  DOWNED 23s", "P4  DEAD"]);
        assert_eq!(hud.allies[2].tone, Tone::Gone);
    }

    fn alive_with_stamina(stamina: u8) -> WireLife {
        WireLife::Alive {
            hp: 100,
            max_hp: 100,
            carried: 0,
            stamina,
            interaction: None,
            build_refusal: None,
        }
    }

    #[test]
    fn my_stamina_is_a_bar_under_my_hp_that_follows_the_percentage_from_the_server() {
        let session = session_in_room();
        let full = Hud::of(&session, Some(&frame(vec![player(YOU, alive(72))])));
        let meter = full.stamina.unwrap();
        assert_eq!(meter.label, "STAMINA");
        assert_eq!(meter.fraction, 1.0);
        assert_eq!(meter.tone, Tone::Stamina);

        for (percent, fraction) in [(40, 0.4), (1, 0.01), (0, 0.0)] {
            let me = player(YOU, alive_with_stamina(percent));
            let hud = Hud::of(&session, Some(&frame(vec![me])));
            let meter = hud.stamina.expect("a living player has a gauge");
            assert!(
                (meter.fraction - fraction).abs() < 1e-6,
                "{percent} percent gave {}",
                meter.fraction
            );
        }
        let over = player(YOU, alive_with_stamina(250));
        let hud = Hud::of(&session, Some(&frame(vec![over])));
        assert_eq!(
            hud.stamina.unwrap().fraction,
            1.0,
            "never more than a full bar"
        );
    }

    #[test]
    fn there_is_no_stamina_bar_while_downed_or_dead_or_before_the_first_snapshot() {
        let session = session_in_room();
        let downed = player(
            YOU,
            WireLife::Downed {
                bleed_secs_left: 20.0,
                revive_progress: None,
            },
        );
        assert_eq!(Hud::of(&session, Some(&frame(vec![downed]))).stamina, None);
        let dead = player(YOU, WireLife::Dead);
        assert_eq!(Hud::of(&session, Some(&frame(vec![dead]))).stamina, None);
        assert_eq!(Hud::of(&session, None).stamina, None);
    }

    #[test]
    fn the_held_weapon_is_shown_with_the_key_that_switches_it() {
        let session = session_in_room();
        let sword = Hud::of(&session, Some(&frame(vec![player(YOU, alive(100))])));
        assert_eq!(sword.weapon.as_deref(), Some("WEAPON  SWORD  [Q]"));

        let mut archer = player(YOU, alive(100));
        archer.weapon = WireWeaponKind::Bow;
        let bow = Hud::of(&session, Some(&frame(vec![archer])));
        assert_eq!(bow.weapon.as_deref(), Some("WEAPON  BOW  [Q]"));
    }

    #[test]
    fn the_weapon_is_shown_even_while_downed_but_not_before_the_first_snapshot_or_for_an_ally() {
        let session = session_in_room();
        let mut downed = player(
            YOU,
            WireLife::Downed {
                bleed_secs_left: 20.0,
                revive_progress: None,
            },
        );
        downed.weapon = WireWeaponKind::Bow;
        assert_eq!(
            Hud::of(&session, Some(&frame(vec![downed])))
                .weapon
                .as_deref(),
            Some("WEAPON  BOW  [Q]")
        );
        assert_eq!(Hud::of(&session, None).weapon, None);

        let mut ally = player(2, alive(100));
        ally.weapon = WireWeaponKind::Bow;
        let hud = Hud::of(&session, Some(&frame(vec![player(YOU, alive(100)), ally])));
        assert_eq!(
            hud.weapon.as_deref(),
            Some("WEAPON  SWORD  [Q]"),
            "only mine"
        );
    }

    #[test]
    fn the_stamina_of_another_player_is_not_shown() {
        let session = session_in_room();
        let tired_ally = player(2, alive_with_stamina(5));
        let hud = Hud::of(
            &session,
            Some(&frame(vec![player(YOU, alive(100)), tired_ally])),
        );
        assert_eq!(hud.stamina.unwrap().fraction, 1.0, "only mine");
        assert_eq!(hud.allies[0].label, "P2  100 / 100");
    }

    #[test]
    fn a_bar_is_empty_when_the_maximum_is_zero_and_never_leaves_zero_to_one() {
        assert_eq!(bar_fraction(0, 0), 0.0);
        assert_eq!(bar_fraction(50, 0), 0.0);
        assert_eq!(bar_fraction(25, 100), 0.25);
        assert_eq!(bar_fraction(0, 100), 0.0);
        assert_eq!(bar_fraction(150, 100), 1.0);
    }

    #[test]
    fn a_player_on_my_team_is_listed_as_an_ally() {
        let session = session_in_room();
        let teammate = PlayerView {
            team: Team::CARAVAN,
            ..player(2, alive(40))
        };
        let hud = Hud::of(
            &session,
            Some(&frame(vec![player(YOU, alive(72)), teammate])),
        );
        let labels: Vec<&str> = hud.allies.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["P2  40 / 100"]);
    }

    #[test]
    fn a_player_on_a_hostile_team_is_not_listed_as_an_ally() {
        let session = session_in_room();
        let rival = PlayerView {
            team: Team(2),
            ..player(2, alive(40))
        };
        let hud = Hud::of(&session, Some(&frame(vec![player(YOU, alive(72)), rival])));
        assert_eq!(hud.allies, []);
    }

    #[test]
    fn a_downed_ally_being_revived_shows_the_progress() {
        let session = session_in_room();
        let ally = player(
            2,
            WireLife::Downed {
                bleed_secs_left: 20.0,
                revive_progress: Some(0.4),
            },
        );
        let hud = Hud::of(&session, Some(&frame(vec![player(YOU, alive(100)), ally])));
        assert_eq!(hud.allies[0].label, "P2  REVIVING 40%");
    }

    #[test]
    fn while_downed_the_bleed_countdown_and_the_revive_bar_are_shown() {
        let session = session_in_room();
        let bleeding = player(
            YOU,
            WireLife::Downed {
                bleed_secs_left: 23.0,
                revive_progress: None,
            },
        );
        let hud_bleeding = Hud::of(&session, Some(&frame(vec![bleeding])));
        assert_eq!(
            hud_bleeding.bleed.as_deref(),
            Some("DOWNED  BLEEDING OUT IN 23s")
        );
        assert_eq!(hud_bleeding.action, None);
        assert_eq!(hud_bleeding.me.unwrap().label, "YOU  DOWNED");

        let rescued = player(
            YOU,
            WireLife::Downed {
                bleed_secs_left: 23.0,
                revive_progress: Some(0.25),
            },
        );
        let hud_rescued = Hud::of(&session, Some(&frame(vec![rescued])));
        let action = hud_rescued.action.unwrap();
        assert_eq!(action.label, "BEING REVIVED");
        assert!((action.fraction - 0.25).abs() < 1e-6);
    }

    #[test]
    fn reviving_a_teammate_shows_a_progress_bar() {
        let session = session_in_room();
        let reviver = player(
            YOU,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: Some(WireInteraction::Reviving {
                    target: 2,
                    progress: 0.5,
                }),
                build_refusal: None,
            },
        );
        let action = Hud::of(&session, Some(&frame(vec![reviver])))
            .action
            .unwrap();
        assert_eq!(action.label, "REVIVING P2");
        assert_eq!(action.fraction, 0.5);
    }

    #[test]
    fn repairing_the_tower_shows_its_hp_as_the_progress() {
        let session = session_in_room();
        let repairer = player(
            YOU,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: Some(WireInteraction::Repairing {
                    target: WireRepairTarget::Tower { tower: 1 },
                    stash_short: false,
                }),
                build_refusal: None,
            },
        );
        let action = Hud::of(&session, Some(&frame(vec![repairer])))
            .action
            .unwrap();
        assert_eq!(action.label, "REPAIRING TOWER  250 / 300");
        assert!((action.fraction - 250.0 / 300.0).abs() < 1e-6);
    }

    #[test]
    fn repairing_the_wagon_shows_its_hp_as_the_progress() {
        let session = session_in_room();
        let repairer = player(
            YOU,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: Some(WireInteraction::Repairing {
                    target: WireRepairTarget::Wagon,
                    stash_short: false,
                }),
                build_refusal: None,
            },
        );
        let mut frame = frame(vec![repairer]);
        frame.wagon = WireWagon {
            hp: 300,
            max_hp: 500,
        };
        let action = Hud::of(&session, Some(&frame)).action.unwrap();
        assert_eq!(action.label, "REPAIRING WAGON  300 / 500");
        assert!((action.fraction - 0.6).abs() < 1e-6);
    }

    fn repairer_of_the_wagon(stash_short: bool) -> PlayerView {
        player(
            YOU,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: Some(WireInteraction::Repairing {
                    target: WireRepairTarget::Wagon,
                    stash_short,
                }),
                build_refusal: None,
            },
        )
    }

    #[test]
    fn a_repair_the_stash_cannot_pay_says_so_and_keeps_showing_the_repair() {
        let session = session_in_room();
        let hud = Hud::of(&session, Some(&frame(vec![repairer_of_the_wagon(true)])));

        assert_eq!(
            hud.refusal.as_deref(),
            Some("CANNOT REPAIR: NOT ENOUGH IN THE STASH")
        );
        assert_eq!(
            hud.action.map(|action| action.label).as_deref(),
            Some("REPAIRING WAGON  400 / 500"),
            "the bar still says what the player is trying to do"
        );
    }

    #[test]
    fn a_repair_the_stash_can_pay_shows_no_refusal() {
        let session = session_in_room();
        let hud = Hud::of(&session, Some(&frame(vec![repairer_of_the_wagon(false)])));
        assert_eq!(hud.refusal, None);
    }

    #[test]
    fn a_repair_refusal_of_another_player_is_not_shown_to_me() {
        let session = session_in_room();
        let other = PlayerView {
            id: 2,
            ..repairer_of_the_wagon(true)
        };
        let hud = Hud::of(&session, Some(&frame(vec![player(YOU, alive(100)), other])));
        assert_eq!(hud.refusal, None);
    }

    #[test]
    fn gathering_shows_what_is_left_in_the_node() {
        let session = session_in_room();
        let gatherer = player(
            YOU,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 3,
                stamina: 100,
                interaction: Some(WireInteraction::Gathering { node: 1 }),
                build_refusal: None,
            },
        );
        let action = Hud::of(&session, Some(&frame(vec![gatherer])))
            .action
            .unwrap();
        assert_eq!(action.label, "GATHERING  9 / 15");
        assert!((action.fraction - 9.0 / 15.0).abs() < 1e-6);
    }

    #[test]
    fn a_full_load_says_where_to_take_it() {
        assert_eq!(carry_text(9, 10), "CARRY  9 / 10");
        assert_eq!(
            carry_text(10, 10),
            "CARRY  10 / 10  FULL - TAKE IT TO THE WAGON"
        );
    }

    #[test]
    fn what_i_carry_is_shown_against_the_capacity_only_while_i_am_alive() {
        let session = session_in_room();
        let carrying = player(
            YOU,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 7,
                stamina: 100,
                interaction: None,
                build_refusal: None,
            },
        );
        let hud = Hud::of(&session, Some(&frame(vec![carrying])));
        assert_eq!(hud.carry.as_deref(), Some("CARRY  7 / 10"));

        let downed = player(
            YOU,
            WireLife::Downed {
                bleed_secs_left: 20.0,
                revive_progress: None,
            },
        );
        assert_eq!(Hud::of(&session, Some(&frame(vec![downed]))).carry, None);
        assert_eq!(Hud::of(&session, None).carry, None);
    }

    #[test]
    fn building_a_tower_shows_the_progress_of_the_site() {
        let session = session_in_room();
        let builder = player(
            YOU,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: Some(WireInteraction::Building { tower: 2 }),
                build_refusal: None,
            },
        );
        let mut frame = frame(vec![builder]);
        frame.towers.push(WireTower {
            id: 2,
            tile: WireTile { x: 16, y: 11 },
            max_hp: 300,
            state: WireTowerState::Building { progress: 0.4 },
        });
        let hud = Hud::of(&session, Some(&frame));
        let action = hud.action.unwrap();
        assert_eq!(action.label, "BUILDING TOWER");
        assert_eq!(action.fraction, 0.4);
        assert_eq!(hud.refusal, None);
    }

    #[test]
    fn every_reason_a_build_is_refused_has_its_own_text() {
        let session = session_in_room();
        let reasons = [
            (WireCannotBuild::NotEnoughStash, "NOT ENOUGH IN THE STASH"),
            (WireCannotBuild::TowerLimit, "TOWER LIMIT REACHED"),
            (WireCannotBuild::OutsideVillage, "TOO FAR FROM THE WAGON"),
            (WireCannotBuild::TileTaken, "THE TILE IS TAKEN"),
            (
                WireCannotBuild::WouldBlockPath,
                "IT WOULD BLOCK THE WAY TO THE WAGON",
            ),
        ];
        let mut texts = Vec::new();
        for (why, expected) in reasons {
            let refused = player(
                YOU,
                WireLife::Alive {
                    hp: 100,
                    max_hp: 100,
                    carried: 0,
                    stamina: 100,
                    interaction: None,
                    build_refusal: Some(WireBuildRefusal {
                        tile: WireTile { x: 3, y: 4 },
                        why,
                    }),
                },
            );
            let hud = Hud::of(&session, Some(&frame(vec![refused])));
            let text = hud.refusal.expect("the reason is shown");
            assert_eq!(text, format!("CANNOT BUILD: {expected}"));
            texts.push(text);
        }
        texts.sort();
        texts.dedup();
        assert_eq!(texts.len(), reasons.len(), "no two reasons share a text");
    }

    #[test]
    fn a_refusal_of_another_player_is_not_shown_to_me() {
        let session = session_in_room();
        let refused = player(
            2,
            WireLife::Alive {
                hp: 100,
                max_hp: 100,
                carried: 0,
                stamina: 100,
                interaction: None,
                build_refusal: Some(WireBuildRefusal {
                    tile: WireTile { x: 3, y: 4 },
                    why: WireCannotBuild::TileTaken,
                }),
            },
        );
        let hud = Hud::of(
            &session,
            Some(&frame(vec![player(YOU, alive(100)), refused])),
        );
        assert_eq!(hud.refusal, None);
    }

    #[test]
    fn the_result_is_shown_with_the_time_until_the_next_expedition() {
        let session = session_in_room();
        let mut frame = frame(vec![player(YOU, alive(100))]);
        frame.phase = WirePhase::Over {
            outcome: WireOutcome::Defeat,
            secs_left: 4.6,
        };
        let failed = Hud::of(&session, Some(&frame)).banner.unwrap();
        assert_eq!(failed.title, "EXPEDITION FAILED");
        assert_eq!(failed.detail.as_deref(), Some("Next expedition in 5s"));
        assert_eq!(failed.tone, BannerTone::Bad);

        frame.phase = WirePhase::Over {
            outcome: WireOutcome::Victory,
            secs_left: 1.0,
        };
        let won = Hud::of(&session, Some(&frame)).banner.unwrap();
        assert_eq!(won.title, "EXPEDITION SUCCESS");
        assert_eq!(won.tone, BannerTone::Good);
        assert_eq!(Hud::of(&session, Some(&frame)).day, None);
    }

    #[test]
    fn a_refused_join_shows_the_reason() {
        let mut session =
            Session::new(RoomCode::parse("test").unwrap(), DEFAULT_SERVER.to_string());
        session.on_event(ClientEvent::Connected);
        session.on_event(ClientEvent::Message(ServerMsg::JoinRejected {
            reason: RejectReason::RoomFull { capacity: 4 },
        }));
        let hud = Hud::of(&session, None);
        let banner = hud.banner.unwrap();
        assert_eq!(banner.title, "JOIN REJECTED");
        assert_eq!(
            banner.detail.as_deref(),
            Some("the room is full (4 players)")
        );
        assert_eq!(banner.tone, BannerTone::Bad);
        assert_eq!(hud.me, None);
    }

    #[test]
    fn a_lost_connection_is_stated_clearly_and_the_game_values_are_gone() {
        let mut session = session_in_room();
        session.on_event(ClientEvent::Disconnected {
            reason: "the server closed the connection".to_string(),
        });
        let hud = Hud::of(&session, None);
        let banner = hud.banner.unwrap();
        assert_eq!(banner.title, "DISCONNECTED");
        assert_eq!(
            banner.detail.as_deref(),
            Some("the server closed the connection")
        );
        assert_eq!(banner.tone, BannerTone::Bad);
        assert_eq!(
            hud.connection,
            "Disconnected: the server closed the connection"
        );
        assert_eq!(hud.day, None);
        assert_eq!(hud.room, None);
    }

    #[test]
    fn a_server_that_cannot_be_reached_is_named_with_a_hint() {
        let mut session = Session::new(
            RoomCode::parse("test").unwrap(),
            "ws://192.168.0.7:7777".to_string(),
        );
        session.on_event(ClientEvent::Disconnected {
            reason: "Unknown error".to_string(),
        });

        let hud = Hud::of(&session, None);

        let banner = hud.banner.unwrap();
        assert_eq!(banner.title, "CANNOT REACH THE SERVER");
        assert_eq!(
            banner.detail.as_deref(),
            Some(
                "Could not open ws://192.168.0.7:7777. Check the address and that the server is running."
            )
        );
        assert_eq!(banner.tone, BannerTone::Bad);
        assert!(!hud.connection.contains("Unknown error"));
    }

    #[test]
    fn before_the_room_and_before_the_first_snapshot_a_waiting_notice_is_shown() {
        let connecting = Session::new(RoomCode::parse("test").unwrap(), DEFAULT_SERVER.to_string());
        let banner = Hud::of(&connecting, None).banner.unwrap();
        assert_eq!(banner.title, "Connecting... (room test)");
        assert_eq!(banner.tone, BannerTone::Info);

        let in_room = session_in_room();
        let waiting = Hud::of(&in_room, None).banner.unwrap();
        assert_eq!(waiting.title, "Waiting for the first snapshot...");
    }
}
