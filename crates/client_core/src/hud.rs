//! 화면 위에 표시할 내용(HUD)을 정하는 곳입니다. 문구, 막대의 채움 비율, 색의 종류만 정하고, 그리는 일은 Bevy 쪽이 합니다.
//!
//! 문구는 영어입니다. Bevy의 기본 글꼴이 영문만 표시하고, 1단계에는 글꼴 파일을 넣지 않기 때문입니다.
//! 모든 값은 지금 그리는 `Frame`에서 오므로, 막대는 그 피해를 일으킨 공격이 화면에 나타난 뒤에 줄어듭니다.

use caravan_protocol::{WireInteraction, WireLife, WireOutcome, WirePhase, WireTowerState};
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
    pub wave: Option<String>,
    /// 화면 가운데에 크게 띄우는 안내입니다. 원정의 결과, 입장 거절, 접속이 끊김이 여기에 나옵니다.
    pub banner: Option<Banner>,
    pub me: Option<Meter>,
    pub wagon: Option<Meter>,
    pub towers: Vec<Meter>,
    pub allies: Vec<Meter>,
    /// 부활이나 수리가 진행되는 동안 화면 아래에 띄우는 진행 막대입니다.
    pub action: Option<Meter>,
    /// 쓰러진 동안의 출혈 시간 안내입니다.
    pub bleed: Option<String>,
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
    Tower,
    Progress,
    Downed,
    /// 사망했거나 파괴되어 더 이상 채울 것이 없는 상태입니다.
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
            wave: None,
            banner: None,
            me: None,
            wagon: None,
            towers: Vec::new(),
            allies: Vec::new(),
            action: None,
            bleed: None,
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
        hud.wave = wave_text(frame);
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
        let numbered = frame.towers.len() > 1;
        hud.towers = frame
            .towers
            .iter()
            .map(|tower| tower_meter(tower.id, tower.max_hp, tower.state, numbered))
            .collect();
        hud.allies = frame
            .players
            .iter()
            .filter(|player| player.id != you && player.team.is_allied_with(my_team))
            .map(ally_meter)
            .collect();

        if let Some(me) = frame.player(you) {
            hud.me = Some(my_meter(me));
            hud.action = action_meter(me, frame);
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

fn wave_text(frame: &Frame) -> Option<String> {
    match frame.phase {
        WirePhase::Fighting { wave } => Some(format!("WAVE {wave} / {}", frame.wave_count)),
        WirePhase::Intermission {
            next_wave,
            secs_left,
        } => Some(format!(
            "WAVE {next_wave} / {} IN {}s",
            frame.wave_count,
            whole_secs(secs_left)
        )),
        WirePhase::Over { .. } => None,
    }
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

fn tower_meter(id: u32, max_hp: u32, state: WireTowerState, numbered: bool) -> Meter {
    let name = if numbered {
        format!("TOWER {id}")
    } else {
        "TOWER".to_string()
    };
    match state {
        WireTowerState::Standing { hp, .. } => Meter {
            label: format!("{name}  {hp} / {max_hp}"),
            fraction: bar_fraction(hp, max_hp),
            tone: Tone::Tower,
        },
        WireTowerState::Destroyed => Meter {
            label: format!("{name}  DESTROYED"),
            fraction: 0.0,
            tone: Tone::Gone,
        },
    }
}

/// 내가 부활이나 수리를 하고 있거나 부활당하고 있을 때의 진행 막대입니다.
/// 수리에는 진행도가 따로 없으므로, 수리하는 감시탑의 HP가 차오르는 모습을 진행도로 보여 줍니다.
fn action_meter(me: &PlayerView, frame: &Frame) -> Option<Meter> {
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
            interaction: Some(WireInteraction::Repairing { tower }),
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
        ClientEvent, RejectReason, RoomCode, ServerMsg, Welcome, WireShot, WireTower, WireWagon,
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
            interaction: None,
        }
    }

    fn player(id: u32, life: WireLife) -> PlayerView {
        PlayerView {
            id,
            team: Team::CARAVAN,
            pos: Vec2::new(5.0, 5.0),
            facing: Vec2::new(1.0, 0.0),
            life,
            last_attack_tick: None,
        }
    }

    fn frame(players: Vec<PlayerView>) -> Frame {
        Frame {
            tick: 100.0,
            expedition: 2,
            phase: WirePhase::Fighting { wave: 2 },
            wave_count: 3,
            wagon: WireWagon {
                hp: 400,
                max_hp: 500,
            },
            towers: vec![WireTower {
                id: 1,
                max_hp: 300,
                state: WireTowerState::Standing {
                    hp: 250,
                    last_shot: None::<WireShot>,
                },
            }],
            players,
            monsters: Vec::new(),
        }
    }

    #[test]
    fn the_wave_wagon_tower_and_room_are_shown() {
        let session = session_in_room();
        let hud = Hud::of(&session, Some(&frame(vec![player(YOU, alive(72))])));

        assert_eq!(hud.wave.as_deref(), Some("WAVE 2 / 3"));
        assert_eq!(hud.expedition.as_deref(), Some("EXPEDITION 2"));
        assert_eq!(hud.room.as_deref(), Some("ROOM test"));
        assert_eq!(hud.connection, "Connected");
        let wagon = hud.wagon.unwrap();
        assert_eq!(wagon.label, "WAGON  400 / 500");
        assert!((wagon.fraction - 0.8).abs() < 1e-6);
        assert_eq!(hud.towers[0].label, "TOWER  250 / 300");
        assert_eq!(hud.banner, None);
    }

    #[test]
    fn between_waves_the_countdown_is_rounded_up() {
        let session = session_in_room();
        let mut frame = frame(vec![player(YOU, alive(100))]);
        frame.phase = WirePhase::Intermission {
            next_wave: 3,
            secs_left: 3.2,
        };
        assert_eq!(
            Hud::of(&session, Some(&frame)).wave.as_deref(),
            Some("WAVE 3 / 3 IN 4s")
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
                interaction: Some(WireInteraction::Reviving {
                    target: 2,
                    progress: 0.5,
                }),
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
                interaction: Some(WireInteraction::Repairing { tower: 1 }),
            },
        );
        let action = Hud::of(&session, Some(&frame(vec![repairer])))
            .action
            .unwrap();
        assert_eq!(action.label, "REPAIRING TOWER  250 / 300");
        assert!((action.fraction - 250.0 / 300.0).abs() < 1e-6);
    }

    #[test]
    fn a_destroyed_tower_is_marked_and_its_bar_is_empty() {
        let session = session_in_room();
        let mut frame = frame(vec![player(YOU, alive(100))]);
        frame.towers[0].state = WireTowerState::Destroyed;
        let hud = Hud::of(&session, Some(&frame));
        assert_eq!(hud.towers[0].label, "TOWER  DESTROYED");
        assert_eq!(hud.towers[0].fraction, 0.0);
        assert_eq!(hud.towers[0].tone, Tone::Gone);
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
        assert_eq!(Hud::of(&session, Some(&frame)).wave, None);
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
        assert_eq!(hud.wave, None);
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
