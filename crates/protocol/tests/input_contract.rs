//! 클라이언트가 서버에 보내는 것이 조작 의도뿐이라는 약속을 고정합니다.
//!
//! 이 파일의 두 테스트는 값을 비교하지 않고 모양을 고정합니다.
//! 필드나 메시지 종류를 추가하면 이 테스트가 컴파일되지 않으므로, 추가한 사람이 이 약속을 다시 읽게 됩니다.

use caravan_protocol::{ClientMsg, InputMsg, WireVec2};

#[test]
fn an_input_carries_a_sequence_number_two_directions_and_eight_buttons_and_nothing_else() {
    let input = InputMsg::default();
    // 필드를 모두 나열해서 분해합니다. 위치, 피해량, 대상, 명중 여부 같은 필드가 생기면 여기서 컴파일이 멈춥니다.
    let InputMsg {
        seq: _,
        move_dir: WireVec2 { x: _, y: _ },
        aim: WireVec2 { x: _, y: _ },
        attack: _,
        interact: _,
        interact_pressed: _,
        build: _,
        sprint: _,
        switch_weapon: _,
        toggle_ready: _,
        confirm: _,
    } = input;
}

#[test]
fn the_client_can_only_join_send_input_or_leave() {
    fn name(msg: &ClientMsg) -> &'static str {
        // 와일드카드 없이 모든 변형을 나열하므로, 변형이 늘면 이 함수가 컴파일되지 않습니다.
        match msg {
            ClientMsg::Join {
                version: _,
                room: _,
                name: _,
                token: _,
            } => "join",
            ClientMsg::Input(_) => "input",
            ClientMsg::Leave => "leave",
        }
    }
    let join = ClientMsg::Join {
        version: 1,
        room: Some("test".to_string()),
        name: "KANG".to_string(),
        token: None,
    };
    assert_eq!(name(&join), "join");
    assert_eq!(name(&ClientMsg::Input(InputMsg::default())), "input");
    assert_eq!(name(&ClientMsg::Leave), "leave");
}
