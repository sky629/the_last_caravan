//! 야영지에서만 보이는 것입니다. 허수아비가 맞은 피해를 허수아비 위에 숫자로 띄웁니다.
//!
//! 피해는 이벤트가 아니라 상태입니다. 스냅샷의 야영지 단계에 허수아비마다 마지막으로 맞은 Tick과 그 Tick에 받은 피해가 있으므로,
//! 화면에 그리는 Tick이 그 직후이면 숫자를 띄웁니다. 그래서 스냅샷을 하나 놓친 클라이언트도 같은 숫자를 같은 때에 봅니다.
//! 같은 허수아비를 다시 맞히면 숫자가 새 피해로 바뀌고 처음부터 다시 떠오릅니다.

use bevy::prelude::*;
use caravan_client_core::effect_progress;
use caravan_protocol::WirePhase;

use crate::config;
use crate::convert;
use crate::net::CurrentFrame;
use crate::registry::Registry;

#[derive(Resource, Default)]
pub struct StrikeRegistry {
    numbers: Registry,
}

/// 허수아비 위에 떠 있는 숫자의 개체입니다.
#[derive(Component)]
pub struct StrikeNumber;

/// 숫자가 떠오르는 모습입니다. `progress`는 0.0에서 1.0이고, 숫자는 허수아비의 키 위에서 위로 올라가면서 투명해집니다.
fn float(foot: caravan_rules::Vec2, progress: f32) -> (Transform, Color) {
    let mut at = convert::placement(foot);
    at.y += config::SCARECROW_BODY.y + config::STRIKE_RISE * progress;
    at.z += config::STRIKE_Z_LIFT;
    (
        Transform::from_translation(at),
        config::STRIKE.with_alpha(1.0 - progress),
    )
}

/// 스냅샷의 허수아비가 맞은 기록에 맞게 숫자를 띄우고 지웁니다. 야영지가 아니거나 숫자를 띄울 시간이 지났으면 모두 지워집니다.
pub fn sync_strikes(
    frame: Res<CurrentFrame>,
    mut strikes: ResMut<StrikeRegistry>,
    mut commands: Commands,
    mut numbers: Query<(&mut Transform, &mut TextColor, &mut Text2d), With<StrikeNumber>>,
) {
    let Some(frame) = &frame.0 else {
        return;
    };
    strikes.numbers.begin_frame();
    if let WirePhase::Camp { struck, .. } = &frame.phase {
        for hit in struck {
            let Some(progress) = effect_progress(frame.tick, hit.tick, config::STRIKE_TICKS) else {
                continue;
            };
            let Some(scarecrow) = frame.monsters.iter().find(|m| m.id == hit.monster) else {
                continue;
            };
            let (transform, color) = float(scarecrow.pos, progress);
            let amount = hit.amount.to_string();
            match strikes.numbers.find(hit.monster) {
                Some(entity) => {
                    if let Ok((mut current, mut paint, mut text)) = numbers.get_mut(entity) {
                        *current = transform;
                        paint.0 = color;
                        if text.0 != amount {
                            text.0 = amount;
                        }
                    }
                }
                None => {
                    let entity = commands
                        .spawn((
                            Text2d::new(amount),
                            TextFont {
                                font_size: FontSize::Px(config::STRIKE_SIZE),
                                ..default()
                            },
                            TextColor(color),
                            transform,
                            StrikeNumber,
                        ))
                        .id();
                    strikes.numbers.insert(&mut commands, hit.monster, entity);
                }
            }
        }
    }
    strikes.numbers.despawn_unseen(&mut commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::joined_flow;
    use crate::view::{self, Scene};
    use caravan_client_core::{Frame, MonsterView};
    use caravan_protocol::{WireMonsterEffects, WireMonsterKind, WireStruck, WireWagon};
    use caravan_rules::Vec2 as Ground;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .insert_resource(joined_flow())
            .init_resource::<CurrentFrame>()
            .init_resource::<StrikeRegistry>()
            .add_systems(Startup, crate::palette::setup_palette)
            .add_systems(
                Update,
                (
                    view::spawn_world.run_if(not(resource_exists::<Scene>)),
                    sync_strikes.run_if(resource_exists::<Scene>),
                )
                    .chain(),
            );
        app
    }

    /// `now`번째 Tick을 그리고 있는 야영지의 화면입니다. 허수아비 하나가 (40.5, 32.0)에 서 있습니다.
    fn show(app: &mut App, now: f64, struck: Vec<WireStruck>) {
        app.world_mut().resource_mut::<CurrentFrame>().0 = Some(Frame {
            tick: now,
            expedition: 0,
            phase: WirePhase::Camp {
                ready: vec![],
                starting_in: None,
                struck,
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
            monsters: vec![MonsterView {
                id: 1,
                kind: WireMonsterKind::Scarecrow,
                effects: WireMonsterEffects::NONE,
                pos: Ground::new(40.5, 32.0),
                facing: Ground::new(1.0, 0.0),
                hp: 60,
            }],
            projectiles: Vec::new(),
        });
        app.update();
    }

    fn hit(tick: u64, amount: u32) -> WireStruck {
        WireStruck {
            monster: 1,
            tick,
            amount,
        }
    }

    /// 지금 떠 있는 숫자의 글자, 높이, 투명도입니다.
    fn numbers(app: &mut App) -> Vec<(String, f32, f32)> {
        let world = app.world_mut();
        let mut query =
            world.query_filtered::<(&Text2d, &Transform, &TextColor), With<StrikeNumber>>();
        query
            .iter(world)
            .map(|(text, transform, color)| {
                (text.0.clone(), transform.translation.y, color.0.alpha())
            })
            .collect()
    }

    #[test]
    fn a_hit_shows_its_damage_above_the_scarecrow_and_floats_up_while_fading() {
        let mut app = app();

        show(&mut app, 11.0, vec![hit(10, 100)]);
        let early = numbers(&mut app);
        show(&mut app, 20.0, vec![hit(10, 100)]);
        let late = numbers(&mut app);

        let foot = convert::placement(Ground::new(40.5, 32.0)).y;
        let [(ref text, y, alpha)] = early[..] else {
            panic!("one number expected, got {early:?}");
        };
        assert_eq!(text, "100");
        assert!(y > foot + config::SCARECROW_BODY.y, "above the scarecrow");
        assert!(alpha < 1.0 && alpha > 0.9, "just started to fade: {alpha}");
        let [(_, late_y, late_alpha)] = late[..] else {
            panic!("one number expected, got {late:?}");
        };
        assert!(late_y > y, "it floats up");
        assert!(late_alpha < alpha, "and fades");
    }

    #[test]
    fn the_number_goes_away_when_its_time_is_up_and_a_missed_snapshot_changes_nothing() {
        let mut app = app();
        show(&mut app, 11.0, vec![hit(10, 100)]);
        assert_eq!(numbers(&mut app).len(), 1);

        // 화면이 한 번에 여러 Tick을 건너뛰어도 같은 기록에서 같은 모습을 그립니다.
        show(&mut app, 40.0, vec![hit(10, 100)]);
        assert_eq!(numbers(&mut app), vec![], "more than 24 ticks have passed");

        show(&mut app, 12.0, vec![hit(10, 100)]);
        assert_eq!(
            numbers(&mut app).len(),
            1,
            "the same record, the same number"
        );
    }

    #[test]
    fn a_new_hit_on_the_same_scarecrow_replaces_the_number_and_starts_it_again() {
        let mut app = app();
        show(&mut app, 20.0, vec![hit(10, 100)]);
        let before = numbers(&mut app);
        let kept = app
            .world()
            .resource::<StrikeRegistry>()
            .numbers
            .entity_of(1)
            .unwrap();

        show(&mut app, 21.0, vec![hit(20, 25)]);

        let after = numbers(&mut app);
        let [(ref text, _, alpha)] = after[..] else {
            panic!("one number expected, got {after:?}");
        };
        assert_eq!(text, "25");
        assert!(alpha > before[0].2, "it started again");
        assert_eq!(
            app.world()
                .resource::<StrikeRegistry>()
                .numbers
                .entity_of(1),
            Some(kept),
            "the same entity shows it"
        );
    }

    #[test]
    fn no_number_shows_outside_the_camp() {
        let mut app = app();
        show(&mut app, 11.0, vec![hit(10, 100)]);
        assert_eq!(numbers(&mut app).len(), 1);

        let mut frame = app.world().resource::<CurrentFrame>().0.clone().unwrap();
        frame.phase = WirePhase::Day {
            day: 1,
            secs_left: 90.0,
        };
        app.world_mut().resource_mut::<CurrentFrame>().0 = Some(frame);
        app.update();

        assert_eq!(numbers(&mut app), vec![]);
    }
}
