//! 시작 화면을 그리고, Bevy의 키보드 이벤트를 `FormKey`로 바꿉니다. 상태와 규칙은 `caravan_client_core`의 `StartForm`에 있습니다.
//!
//! 글자는 `KeyboardInput`의 `text`(Shift가 반영된 글자)를 씁니다. 웹은 Bevy가 브라우저의 기본 동작을 막으므로(기본값)
//! Tab이 브라우저의 포커스를 옮기지 않습니다. 붙여 넣기(Ctrl+V, Cmd+V)는 Bevy의 클립보드에서 읽습니다.
//! 웹에서는 브라우저가 처음 한 번 읽기 권한을 묻고, 거절하면 칸이 그대로이고 안내 한 줄이 보입니다.

use bevy::clipboard::{Clipboard, ClipboardRead};
use bevy::input::keyboard::{Key, KeyboardFocusLost, KeyboardInput};
use bevy::prelude::*;
use caravan_client_core::{Field, FormKey, StartForm};

use crate::net::{ClientFlow, Net, run_commands};

const TEXT: Color = Color::srgb(0.92, 0.92, 0.92);
const TEXT_DIM: Color = Color::srgb(0.60, 0.60, 0.64);
const NOTICE: Color = Color::srgb(1.0, 0.45, 0.25);
const FIELD_BACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const BORDER_IDLE: Color = Color::srgb(0.30, 0.30, 0.34);
const BORDER_FOCUS: Color = Color::srgb(1.00, 0.84, 0.25);
const LABEL_WIDTH: f32 = 90.0;
const FIELD_WIDTH: f32 = 380.0;

/// 시작 화면의 UI 노드입니다. 시작할 때 한 번 만들고, 시작 화면이 아니면 숨깁니다.
#[derive(Resource)]
pub struct StartNodes {
    root: Entity,
    name_box: Entity,
    name_text: Entity,
    room_box: Entity,
    room_text: Entity,
    enter: Entity,
    notice: Entity,
}

fn text_node(commands: &mut Commands, size: f32, color: Color, text: &str) -> Entity {
    commands
        .spawn((
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
        ))
        .id()
}

/// 입력 칸 하나입니다. 이름표, 테두리가 있는 상자, 상자 안의 글자로 이루어진 한 줄을 만들고 (줄, 상자, 글자)를 돌려줍니다.
fn field_row(commands: &mut Commands, label: &str) -> (Entity, Entity, Entity) {
    let name = text_node(commands, 22.0, TEXT_DIM, label);
    let label = commands
        .spawn(Node {
            width: px(LABEL_WIDTH),
            ..default()
        })
        .add_child(name)
        .id();
    let text = text_node(commands, 26.0, TEXT, "");
    let boxed = commands
        .spawn((
            Node {
                width: px(FIELD_WIDTH),
                height: px(48),
                padding: UiRect::horizontal(px(12)),
                border: UiRect::all(px(2)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(FIELD_BACK),
            BorderColor::all(BORDER_IDLE),
        ))
        .add_child(text)
        .id();
    let row = commands
        .spawn(Node {
            align_items: AlignItems::Center,
            ..default()
        })
        .add_children(&[label, boxed])
        .id();
    (row, boxed, text)
}

/// 시작할 때 한 번 실행해서 시작 화면의 노드를 만듭니다.
pub fn setup(mut commands: Commands) {
    let title = text_node(&mut commands, 64.0, TEXT, "THE LAST CARAVAN");
    let (name_row, name_box, name_text) = field_row(&mut commands, "NAME");
    let (room_row, room_box, room_text) = field_row(&mut commands, "ROOM");
    let room_hint = text_node(
        &mut commands,
        16.0,
        TEXT_DIM,
        "Leave ROOM blank to start a new room",
    );
    let enter = text_node(&mut commands, 30.0, TEXT, "");
    let notice = text_node(&mut commands, 22.0, NOTICE, "");
    let footer = text_node(
        &mut commands,
        16.0,
        TEXT_DIM,
        "TAB  NEXT FIELD      CTRL+V / CMD+V  PASTE",
    );
    let root = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(16),
            display: Display::None,
            ..default()
        })
        .add_children(&[title, name_row, room_row, room_hint, enter, notice, footer])
        .id();
    commands.insert_resource(StartNodes {
        root,
        name_box,
        name_text,
        room_box,
        room_text,
        enter,
        notice,
    });
}

/// 방에 들어가서 세계를 그리고 있는 동안 참입니다.
pub fn in_room(flow: Res<ClientFlow>) -> bool {
    flow.joined().is_some()
}

/// 접속하거나 입장하는 중이거나 방에 있는 동안 참입니다. 세션이 끝나면 `Flow`가 곧바로 시작 화면으로 돌아가므로 끝난 세션은 없습니다.
pub fn playing(flow: Res<ClientFlow>) -> bool {
    flow.0.session().is_some()
}

/// 키보드 이벤트를 읽는 동안 기억할 것입니다. 시작 화면이 떠 있는 동안에만 값이 있고, 방에 있는 동안에는 비어 있습니다.
#[derive(Default)]
pub struct KeyState {
    held: HeldKeys,
    /// 웹은 클립보드를 비동기로 읽으므로, 읽는 중인 일을 다음 프레임에 이어서 확인합니다.
    paste: Option<ClipboardRead>,
}

/// 시작 화면에서 누르고 있는 키입니다.
#[derive(Default)]
struct HeldKeys {
    /// Ctrl이나 Cmd를 누르고 있습니다. 수정 키는 `ButtonInput`이 아니라 이벤트에서 따라갑니다.
    modifier_down: bool,
    /// 시작 화면에서 누르기 시작해서 아직 떼지 않은 키입니다. 방에서 누르기 시작한 키가 시작 화면으로 돌아온 뒤에 되풀이되어도 글자가 되지 않게 합니다.
    pressed_here: Vec<KeyCode>,
}

/// 키보드 이벤트 하나를 읽어서 폼에 줄 키를 `keys`에 더합니다. 붙여 넣기 키이면 참을 돌려줍니다.
fn read_event(held: &mut HeldKeys, event: &KeyboardInput, keys: &mut Vec<FormKey>) -> bool {
    if matches!(
        event.key_code,
        KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::SuperLeft | KeyCode::SuperRight
    ) {
        held.modifier_down = event.state.is_pressed();
        return false;
    }
    if !event.state.is_pressed() {
        held.pressed_here.retain(|code| *code != event.key_code);
        return false;
    }
    if event.repeat {
        if !held.pressed_here.contains(&event.key_code) {
            return false;
        }
    } else if !held.pressed_here.contains(&event.key_code) {
        held.pressed_here.push(event.key_code);
    }
    match &event.logical_key {
        Key::Backspace => keys.push(FormKey::Backspace),
        // 키를 누르고 있을 때 되풀이되는 것은 글자 지우기뿐입니다. 입장과 칸 이동이 되풀이되면 안 됩니다.
        Key::Tab if !event.repeat => keys.push(FormKey::Tab),
        Key::Enter if !event.repeat => keys.push(FormKey::Enter),
        Key::Character(letter) if held.modifier_down => {
            return letter.eq_ignore_ascii_case("v") && !event.repeat;
        }
        _ => {
            let typed = event.text.iter().flat_map(|text| text.chars());
            keys.extend(typed.filter(|c| !c.is_control()).map(FormKey::Char));
        }
    }
    false
}

/// 키보드 이벤트를 읽어서 `Flow::on_key`에 넘기고, 돌아온 일을 실행합니다.
/// 시작 화면이 아닐 때도 프레임마다 실행해서 이벤트를 읽어 버립니다. 그래서 방에서 누른 키와 방에 있는 동안 끝난 붙여 넣기가
/// 시작 화면으로 돌아온 뒤에 칸에 들어가지 않습니다.
pub fn read_form_keys(
    mut events: MessageReader<KeyboardInput>,
    mut focus_lost: MessageReader<KeyboardFocusLost>,
    mut state: Local<KeyState>,
    mut net: NonSendMut<Net>,
    mut flow: ResMut<ClientFlow>,
    mut clipboard: ResMut<Clipboard>,
) {
    if flow.0.form().is_none() {
        events.clear();
        focus_lost.clear();
        *state = KeyState::default();
        return;
    }
    if focus_lost.read().count() > 0 {
        state.held = HeldKeys::default();
    }
    let mut keys = Vec::new();
    for event in events.read() {
        if read_event(&mut state.held, event, &mut keys) {
            state.paste = Some(clipboard.fetch_text());
        }
    }
    if let Some(read) = &mut state.paste
        && let Some(result) = read.poll_result()
    {
        state.paste = None;
        match result {
            Ok(text) => keys.push(FormKey::Paste(text)),
            Err(err) => {
                warn!("cannot read the clipboard: {err}");
                flow.0.on_paste_failed();
            }
        }
    }
    for key in keys {
        let commands = flow.0.on_key(key);
        run_commands(commands, &mut flow.0, &mut net, &mut clipboard);
    }
}

/// 칸에 보이는 글자입니다. 입력 중인 칸에는 끝에 밑줄이 붙습니다.
fn field_text(form: &StartForm, field: Field) -> String {
    let (text, focused) = match field {
        Field::Name => (form.name.text(), form.focus == Field::Name),
        Field::Room => (form.room.text(), form.focus == Field::Room),
    };
    if focused {
        format!("{text}_")
    } else {
        text.to_string()
    }
}

fn set_text(texts: &mut Query<&mut Text>, entity: Entity, value: &str) {
    if let Ok(mut text) = texts.get_mut(entity)
        && text.0 != value
    {
        text.0 = value.to_string();
    }
}

fn set_border(borders: &mut Query<&mut BorderColor>, entity: Entity, focused: bool) {
    if let Ok(mut border) = borders.get_mut(entity) {
        let color = BorderColor::all(if focused { BORDER_FOCUS } else { BORDER_IDLE });
        border.set_if_neq(color);
    }
}

/// 시작 화면이면 칸의 글자, 입력 중인 칸의 표시, 안내 한 줄을 그리고, 아니면 숨깁니다.
pub fn draw_form(
    flow: Res<ClientFlow>,
    nodes: Res<StartNodes>,
    mut layout: Query<&mut Node>,
    mut texts: Query<&mut Text>,
    mut borders: Query<&mut BorderColor>,
) {
    let form = flow.0.form();
    if let Ok(mut root) = layout.get_mut(nodes.root) {
        let display = if form.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if root.display != display {
            root.display = display;
        }
    }
    let Some(form) = form else {
        return;
    };
    set_text(&mut texts, nodes.name_text, &field_text(form, Field::Name));
    set_text(&mut texts, nodes.room_text, &field_text(form, Field::Room));
    set_border(&mut borders, nodes.name_box, form.focus == Field::Name);
    set_border(&mut borders, nodes.room_box, form.focus == Field::Room);
    set_text(&mut texts, nodes.enter, form.enter_hint());
    set_text(
        &mut texts,
        nodes.notice,
        form.notice.as_deref().unwrap_or(""),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::ButtonState;
    use caravan_client_core::{Flow, LaunchOptions, Memory, StartForm};

    fn key(key_code: KeyCode, logical_key: Key, text: Option<&str>) -> KeyboardInput {
        KeyboardInput {
            key_code,
            logical_key,
            state: ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }

    fn letter(c: &str) -> KeyboardInput {
        key(KeyCode::KeyA, Key::Character(c.into()), Some(c))
    }

    fn modifier(key_code: KeyCode, state: ButtonState) -> KeyboardInput {
        KeyboardInput {
            state,
            ..key(key_code, Key::Control, None)
        }
    }

    /// 이벤트를 차례로 읽어서 (폼에 간 키, 붙여 넣기를 청한 횟수)를 돌려줍니다.
    fn read(events: &[KeyboardInput]) -> (Vec<FormKey>, usize) {
        let (mut held, mut keys, mut pastes) = (HeldKeys::default(), Vec::new(), 0);
        for event in events {
            if read_event(&mut held, event, &mut keys) {
                pastes += 1;
            }
        }
        (keys, pastes)
    }

    #[test]
    fn typed_letters_reach_the_form_as_they_were_typed() {
        let (keys, pastes) = read(&[letter("K"), letter("a"), letter("7")]);
        assert_eq!(
            keys,
            vec![FormKey::Char('K'), FormKey::Char('a'), FormKey::Char('7')]
        );
        assert_eq!(pastes, 0);
    }

    #[test]
    fn control_or_command_with_v_asks_for_a_paste_and_types_nothing() {
        for held in [
            KeyCode::ControlLeft,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ] {
            let (keys, pastes) = read(&[modifier(held, ButtonState::Pressed), letter("v")]);
            assert!(keys.is_empty(), "{held:?} + v types no letter");
            assert_eq!(pastes, 1, "{held:?} + v pastes");
        }
    }

    #[test]
    fn a_letter_after_the_modifier_was_released_is_typed_again() {
        let (keys, pastes) = read(&[
            modifier(KeyCode::ControlLeft, ButtonState::Pressed),
            modifier(KeyCode::ControlLeft, ButtonState::Released),
            letter("v"),
        ]);
        assert_eq!(keys, vec![FormKey::Char('v')]);
        assert_eq!(pastes, 0);
    }

    #[test]
    fn other_letters_with_the_modifier_down_are_not_typed() {
        let (keys, pastes) = read(&[
            modifier(KeyCode::ControlLeft, ButtonState::Pressed),
            letter("c"),
        ]);
        assert!(keys.is_empty());
        assert_eq!(pastes, 0);
    }

    #[test]
    fn holding_enter_or_tab_acts_once_but_holding_backspace_keeps_erasing() {
        let held = |key_code, logical_key| KeyboardInput {
            repeat: true,
            ..key(key_code, logical_key, None)
        };
        let (keys, _) = read(&[
            key(KeyCode::Enter, Key::Enter, None),
            held(KeyCode::Enter, Key::Enter),
            key(KeyCode::Tab, Key::Tab, None),
            held(KeyCode::Tab, Key::Tab),
            key(KeyCode::Backspace, Key::Backspace, None),
            held(KeyCode::Backspace, Key::Backspace),
        ]);
        assert_eq!(
            keys,
            vec![
                FormKey::Enter,
                FormKey::Tab,
                FormKey::Backspace,
                FormKey::Backspace
            ]
        );
    }

    #[test]
    fn a_key_that_was_already_down_when_the_start_screen_appeared_types_nothing_until_pressed_again()
     {
        let repeated = KeyboardInput {
            repeat: true,
            ..letter("d")
        };
        let released = KeyboardInput {
            state: ButtonState::Released,
            ..letter("d")
        };
        let (keys, _) = read(&[
            repeated.clone(),
            repeated.clone(),
            released,
            letter("d"),
            repeated,
        ]);
        assert_eq!(keys, vec![FormKey::Char('d'), FormKey::Char('d')]);
    }

    #[test]
    fn releasing_a_key_and_control_characters_type_nothing() {
        let released = KeyboardInput {
            state: ButtonState::Released,
            ..letter("a")
        };
        let escape = key(KeyCode::Escape, Key::Escape, Some("\u{1b}"));
        let (keys, _) = read(&[released, escape]);
        assert!(keys.is_empty());
    }

    #[test]
    fn the_field_in_use_ends_with_an_underscore_and_the_other_does_not() {
        let mut form = StartForm::new("KANG", "k7m", None);
        assert_eq!(form.focus, Field::Room);
        assert_eq!(field_text(&form, Field::Name), "KANG");
        assert_eq!(field_text(&form, Field::Room), "k7m_");

        form.focus = Field::Name;
        assert_eq!(field_text(&form, Field::Name), "KANG_");
        assert_eq!(field_text(&form, Field::Room), "k7m");
    }

    fn drawn(flow: Flow) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ClientFlow(flow))
            .add_systems(Startup, setup)
            .add_systems(Update, draw_form);
        app.update();
        app
    }

    fn start_flow(name: Option<&str>) -> Flow {
        let launch = LaunchOptions::from_values(None, name.map(str::to_string), None, false);
        Flow::new(launch, Memory::default(), None).0
    }

    fn text_of(app: &mut App, pick: impl Fn(&StartNodes) -> Entity) -> String {
        let entity = pick(app.world().resource::<StartNodes>());
        app.world().get::<Text>(entity).unwrap().0.clone()
    }

    fn shown(app: &mut App) -> bool {
        let root = app.world().resource::<StartNodes>().root;
        app.world().get::<Node>(root).unwrap().display == Display::Flex
    }

    #[test]
    fn the_start_screen_shows_the_typed_text_the_next_step_and_the_reason_for_coming_back() {
        let mut flow = start_flow(None);
        for c in "KANG".chars() {
            flow.on_key(FormKey::Char(c));
        }
        flow.on_key(FormKey::Tab);
        for c in "k7m2q".chars() {
            flow.on_key(FormKey::Char(c));
        }
        let mut app = drawn(flow);

        assert!(shown(&mut app));
        assert_eq!(text_of(&mut app, |n| n.name_text), "KANG");
        assert_eq!(text_of(&mut app, |n| n.room_text), "k7m2q_");
        assert_eq!(text_of(&mut app, |n| n.enter), "ENTER  JOIN");
        assert_eq!(text_of(&mut app, |n| n.notice), "");
    }

    #[test]
    fn an_empty_room_field_says_that_enter_makes_a_new_room() {
        let mut app = drawn(start_flow(Some("KANG")));
        assert_eq!(text_of(&mut app, |n| n.enter), "ENTER  CREATE A NEW ROOM");
    }

    #[test]
    fn the_start_screen_is_hidden_once_the_room_is_entered() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::testing::joined_flow())
            .add_systems(Startup, setup)
            .add_systems(Update, draw_form);
        app.update();
        assert!(!shown(&mut app));
    }
}
