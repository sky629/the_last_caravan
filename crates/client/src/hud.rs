//! 화면 위의 HUD입니다 (Bevy UI). 무엇을 표시할지는 `caravan_client_core::Hud`가 정하고, 이 파일은 그 값을 노드에 옮깁니다.
//!
//! 왼쪽 위에 내 HP와 달리기 게이지, 마차, 동료의 막대가 있고, 오른쪽 위에 날과 낮밤, 낮의 남은 시간이나 무리 번호, 보관함의 자원, 내가 든 자원, 내가 가진 무기 두 칸이 있고,
//! 야영지에서는 그 아래에 준비 목록과 `R` 안내와 시작까지 남은 시간이 있고,
//! 아래 가운데에 F를 누르면 하게 되는 일의 안내와 건설과 수리의 거절 이유와 부활, 수리, 건설의 진행 막대가 있고,
//! 오른쪽 아래에 방 코드가 있고, 화면 가운데에 원정의 결과나 접속 문제가 크게 나옵니다.

use bevy::prelude::*;
use caravan_client_core::{BannerTone, Hud, Meter, Tone};

use crate::net::{ClientFlow, CurrentFrame};

const METER_WIDTH: f32 = 260.0;
const ACTION_WIDTH: f32 = 380.0;
const METER_HEIGHT: f32 = 22.0;
const LABEL_SIZE: f32 = 14.0;

const PANEL_BACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const TEXT: Color = Color::srgb(0.92, 0.92, 0.92);
const TEXT_DIM: Color = Color::srgb(0.65, 0.65, 0.68);
const BLEED: Color = Color::srgb(1.0, 0.45, 0.25);

fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Health => Color::srgb(0.30, 0.78, 0.35),
        Tone::Wagon => Color::srgb(0.80, 0.52, 0.24),
        Tone::Progress => Color::srgb(0.95, 0.80, 0.25),
        Tone::Stamina => Color::srgb(0.30, 0.62, 0.92),
        Tone::Downed => Color::srgb(0.90, 0.40, 0.15),
        Tone::Gone => Color::srgb(0.38, 0.38, 0.40),
    }
}

fn banner_color(tone: BannerTone) -> Color {
    match tone {
        BannerTone::Info => TEXT,
        BannerTone::Good => Color::srgb(0.45, 1.0, 0.55),
        BannerTone::Bad => Color::srgb(1.0, 0.42, 0.36),
    }
}

/// 막대 하나(뒤판, 채움, 문구)를 이루는 노드 중 갱신할 것들입니다. 막대의 뒤판 노드가 이 컴포넌트를 가집니다.
#[derive(Component)]
pub struct MeterSlot {
    fill: Entity,
    label: Entity,
}

#[derive(Component)]
pub struct MeterFill;

/// 갱신할 노드들의 목록입니다. 시작할 때 만든 것은 처음부터 있고, 동료의 막대는 입장한 뒤에 정해집니다.
#[derive(Resource)]
pub struct HudNodes {
    connection: Entity,
    room: Entity,
    expedition: Entity,
    day: Entity,
    timer: Entity,
    stash: Entity,
    carry: Entity,
    weapon: Entity,
    /// 야영지의 준비 목록입니다. 한 줄씩 담는 상자이고, 줄은 방의 정원만큼 입장한 뒤에 만듭니다.
    camp_rows_box: Entity,
    camp_rows: Vec<Entity>,
    camp_hint: Entity,
    camp_countdown: Entity,
    hint: Entity,
    banner_title: Entity,
    banner_detail: Entity,
    bleed: Entity,
    refusal: Entity,
    me: Entity,
    stamina: Entity,
    wagon: Entity,
    action: Entity,
    allies_panel: Entity,
    allies: Vec<Entity>,
    roster_built: bool,
}

fn text_node(commands: &mut Commands, size: f32, color: Color) -> Entity {
    commands
        .spawn((
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
            // 글자가 클수록 그림자를 조금 더 띄웁니다.
            TextShadow {
                offset: Vec2::splat((size / 16.0).clamp(1.0, 3.0)),
                ..default()
            },
        ))
        .id()
}

/// 막대 하나를 만듭니다. 처음에는 숨겨져 있고, 값이 생기면 `update`가 보여 줍니다.
fn meter_node(commands: &mut Commands, width: f32) -> Entity {
    let fill = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                height: percent(100),
                width: percent(0),
                ..default()
            },
            BackgroundColor(tone_color(Tone::Health)),
            MeterFill,
        ))
        .id();
    let label = text_node(commands, LABEL_SIZE, TEXT);
    commands
        .spawn((
            Node {
                width: px(width),
                height: px(METER_HEIGHT),
                display: Display::None,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(PANEL_BACK),
            MeterSlot { fill, label },
        ))
        .add_children(&[fill, label])
        .id()
}

fn column(position: Node) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        ..position
    }
}

/// 시작할 때 한 번 실행해서 HUD의 노드를 만듭니다.
pub fn setup(mut commands: Commands) {
    // 왼쪽 위: 내 HP와 달리기 게이지, 마차, 동료들.
    let me = meter_node(&mut commands, METER_WIDTH);
    let stamina = meter_node(&mut commands, METER_WIDTH);
    let wagon = meter_node(&mut commands, METER_WIDTH);
    let allies_panel = commands
        .spawn(column(Node {
            row_gap: px(4),
            ..default()
        }))
        .id();
    commands
        .spawn(column(Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: px(10),
            row_gap: px(4),
            ..default()
        }))
        .add_children(&[me, stamina, wagon, allies_panel]);

    // 오른쪽 위: 날과 낮밤, 낮의 남은 시간이나 무리 번호, 원정 번호, 보관함, 내가 든 자원, 내가 든 무기.
    let day = text_node(&mut commands, 30.0, TEXT);
    let timer = text_node(&mut commands, 20.0, TEXT);
    let expedition = text_node(&mut commands, 16.0, TEXT_DIM);
    let stash = text_node(&mut commands, 22.0, TEXT);
    let carry = text_node(&mut commands, 22.0, TEXT);
    let weapon = text_node(&mut commands, 22.0, TEXT);
    // 야영지에서는 이 아래에 준비 목록이 나옵니다.
    let camp_rows_box = commands
        .spawn(column(Node {
            align_items: AlignItems::FlexEnd,
            margin: UiRect::top(px(14)),
            ..default()
        }))
        .id();
    let camp_hint = text_node(&mut commands, 24.0, TEXT);
    let camp_countdown = text_node(&mut commands, 30.0, TEXT);
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(10),
            right: px(16),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            ..default()
        })
        .add_children(&[
            day,
            timer,
            expedition,
            stash,
            carry,
            weapon,
            camp_rows_box,
            camp_hint,
            camp_countdown,
        ]);

    // 아래 가운데: F를 누르면 하게 되는 일, 출혈 시간, 건설과 수리의 거절 이유, 부활이나 수리나 건설의 진행 막대.
    let hint = text_node(&mut commands, 24.0, TEXT);
    let bleed = text_node(&mut commands, 24.0, BLEED);
    let refusal = text_node(&mut commands, 24.0, BLEED);
    let action = meter_node(&mut commands, ACTION_WIDTH);
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: px(28),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6),
            ..default()
        })
        .add_children(&[hint, bleed, refusal, action]);

    // 아래: 접속 상태와 방 코드.
    let connection = text_node(&mut commands, 14.0, TEXT_DIM);
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: px(8),
            left: px(12),
            ..default()
        })
        .add_child(connection);
    let room = text_node(&mut commands, 18.0, TEXT);
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: px(8),
            right: px(16),
            ..default()
        })
        .add_child(room);

    // 가운데: 원정의 결과, 입장 거절, 접속이 끊김.
    let banner_title = text_node(&mut commands, 46.0, TEXT);
    let banner_detail = text_node(&mut commands, 22.0, TEXT);
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(10),
            ..default()
        })
        .add_children(&[banner_title, banner_detail]);

    commands.insert_resource(HudNodes {
        connection,
        room,
        expedition,
        day,
        timer,
        stash,
        carry,
        weapon,
        camp_rows_box,
        camp_rows: Vec::new(),
        camp_hint,
        camp_countdown,
        hint,
        banner_title,
        banner_detail,
        bleed,
        refusal,
        me,
        stamina,
        wagon,
        action,
        allies_panel,
        allies: Vec::new(),
        roster_built: false,
    });
}

/// 입장한 직후 한 번, 동료의 막대와 야영지의 준비 목록의 줄을 만듭니다.
/// 수는 `Welcome`이 알려 준 방의 정원에서 정합니다. `Session`이 입장할 때 정원의 상한을 확인했으므로 그대로 믿습니다.
pub fn build_roster(mut commands: Commands, flow: Res<ClientFlow>, mut nodes: ResMut<HudNodes>) {
    if nodes.roster_built {
        return;
    }
    let Some(joined) = flow.joined() else {
        return;
    };
    // 내 막대는 따로 있으므로 동료 막대는 정원보다 하나 적습니다.
    let ally_rows = (joined.welcome.capacity as usize).saturating_sub(1);
    for _ in 0..ally_rows {
        let row = meter_node(&mut commands, METER_WIDTH);
        commands.entity(nodes.allies_panel).add_child(row);
        nodes.allies.push(row);
    }
    for _ in 0..joined.welcome.capacity {
        let row = text_node(&mut commands, 22.0, TEXT);
        commands.entity(nodes.camp_rows_box).add_child(row);
        nodes.camp_rows.push(row);
    }
    nodes.roster_built = true;
}

fn set_text(texts: &mut Query<(&mut Text, &mut TextColor)>, entity: Entity, value: &str) {
    if let Ok((mut text, _)) = texts.get_mut(entity)
        && text.0 != value
    {
        text.0 = value.to_string();
    }
}

fn set_text_color(texts: &mut Query<(&mut Text, &mut TextColor)>, entity: Entity, color: Color) {
    if let Ok((_, mut text_color)) = texts.get_mut(entity)
        && text_color.0 != color
    {
        text_color.0 = color;
    }
}

fn show_meter(
    entity: Entity,
    meter: Option<&Meter>,
    slots: &mut Query<(&MeterSlot, &mut Node), Without<MeterFill>>,
    fills: &mut Query<(&mut Node, &mut BackgroundColor), With<MeterFill>>,
    texts: &mut Query<(&mut Text, &mut TextColor)>,
) {
    let Ok((slot, mut node)) = slots.get_mut(entity) else {
        return;
    };
    let display = if meter.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    let Some(meter) = meter else {
        return;
    };
    if let Ok((mut fill, mut color)) = fills.get_mut(slot.fill) {
        let width = percent(meter.fraction * 100.0);
        if fill.width != width {
            fill.width = width;
        }
        let tone = tone_color(meter.tone);
        if color.0 != tone {
            color.0 = tone;
        }
    }
    set_text(texts, slot.label, &meter.label);
}

/// HUD의 내용을 이번 프레임의 값으로 맞춥니다.
pub fn update(
    flow: Res<ClientFlow>,
    frame: Res<CurrentFrame>,
    nodes: Res<HudNodes>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut slots: Query<(&MeterSlot, &mut Node), Without<MeterFill>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<MeterFill>>,
) {
    // 시작 화면에는 방에 관한 값이 없으므로 모든 칸을 비웁니다.
    let hud = match flow.0.session() {
        Some(session) => Hud::of(session, frame.0.as_ref()),
        None => Hud::none(),
    };

    set_text(&mut texts, nodes.connection, &hud.connection);
    set_text(&mut texts, nodes.room, hud.room.as_deref().unwrap_or(""));
    set_text(
        &mut texts,
        nodes.expedition,
        hud.expedition.as_deref().unwrap_or(""),
    );
    set_text(&mut texts, nodes.day, hud.day.as_deref().unwrap_or(""));
    set_text(&mut texts, nodes.timer, hud.timer.as_deref().unwrap_or(""));
    set_text(&mut texts, nodes.stash, hud.stash.as_deref().unwrap_or(""));
    set_text(&mut texts, nodes.carry, hud.carry.as_deref().unwrap_or(""));
    set_text(
        &mut texts,
        nodes.weapon,
        hud.weapon.as_deref().unwrap_or(""),
    );
    let camp = hud.camp.as_ref();
    for (index, row) in nodes.camp_rows.iter().enumerate() {
        let line = camp.and_then(|panel| panel.rows.get(index));
        set_text(&mut texts, *row, line.map_or("", String::as_str));
    }
    set_text(
        &mut texts,
        nodes.camp_hint,
        camp.map_or("", |panel| panel.hint.as_str()),
    );
    set_text(
        &mut texts,
        nodes.camp_countdown,
        camp.and_then(|panel| panel.countdown.as_deref())
            .unwrap_or(""),
    );
    set_text(&mut texts, nodes.hint, hud.hint.as_deref().unwrap_or(""));
    set_text(&mut texts, nodes.bleed, hud.bleed.as_deref().unwrap_or(""));
    set_text(
        &mut texts,
        nodes.refusal,
        hud.refusal.as_deref().unwrap_or(""),
    );
    match &hud.banner {
        Some(banner) => {
            set_text(&mut texts, nodes.banner_title, &banner.title);
            set_text(
                &mut texts,
                nodes.banner_detail,
                banner.detail.as_deref().unwrap_or(""),
            );
            let color = banner_color(banner.tone);
            set_text_color(&mut texts, nodes.banner_title, color);
            set_text_color(&mut texts, nodes.banner_detail, color);
        }
        None => {
            set_text(&mut texts, nodes.banner_title, "");
            set_text(&mut texts, nodes.banner_detail, "");
        }
    }

    let rows = [
        (nodes.me, hud.me.as_ref()),
        (nodes.stamina, hud.stamina.as_ref()),
        (nodes.wagon, hud.wagon.as_ref()),
        (nodes.action, hud.action.as_ref()),
    ];
    for (entity, meter) in rows {
        show_meter(entity, meter, &mut slots, &mut fills, &mut texts);
    }
    for (index, entity) in nodes.allies.iter().enumerate() {
        show_meter(
            *entity,
            hud.allies.get(index),
            &mut slots,
            &mut fills,
            &mut texts,
        );
    }
}
