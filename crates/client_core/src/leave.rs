//! 방을 나가기 전에 한 번 더 묻는 확인 창입니다. 창이 떠 있는지와 키가 무엇을 하는지만 정하고, 그리는 일은 Bevy 쪽이 합니다.
//!
//! ```text
//!   Closed --Esc--> Open --Esc--> Closed
//!                    |
//!                    +--Enter--> 방을 나감
//! ```
//!
//! 서버가 게임을 진행하므로 창이 떠 있는 동안에도 이동과 공격은 그대로 서버에 갑니다.
//! 창이 떠 있는 동안의 Enter는 나가기로만 쓰이고, 결과 화면의 확인 같은 다른 Enter의 일은 하지 않습니다.
//! 창이 떠 있는 프레임에는 같은 프레임에 Esc가 창을 닫았더라도 그 프레임의 Enter가 확인이 되지 않습니다(`Flow::on_room_frame`).
//! 창은 방에 들어간 뒤의 상태(`Joined`)에만 있으므로, 방을 나가거나 접속이 끊겨서 상태가 바뀌면 함께 사라집니다.

/// 나가기 확인 창의 상태입니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LeavePrompt {
    #[default]
    Closed,
    Open,
}

/// 방 안에서 누른 키 가운데 확인 창이 읽는 것입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKey {
    Escape,
    Enter,
}

/// 키를 누른 뒤에 확인 창이 정한 일입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptStep {
    /// 방에 남습니다. 창은 이 상태가 됩니다.
    Stay(LeavePrompt),
    /// 방을 나갑니다.
    Leave,
}

impl LeavePrompt {
    pub fn is_open(self) -> bool {
        self == LeavePrompt::Open
    }

    /// 창이 이 상태일 때 `key`를 누르면 하는 일입니다. 창이 닫혀 있을 때의 Enter는 창과 관계가 없으므로 아무것도 바꾸지 않습니다.
    pub fn press(self, key: RoomKey) -> PromptStep {
        match (self, key) {
            (LeavePrompt::Closed, RoomKey::Escape) => PromptStep::Stay(LeavePrompt::Open),
            (LeavePrompt::Open, RoomKey::Escape) => PromptStep::Stay(LeavePrompt::Closed),
            (LeavePrompt::Open, RoomKey::Enter) => PromptStep::Leave,
            (LeavePrompt::Closed, RoomKey::Enter) => PromptStep::Stay(LeavePrompt::Closed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_starts_closed() {
        assert_eq!(LeavePrompt::default(), LeavePrompt::Closed);
        assert!(!LeavePrompt::default().is_open());
    }

    #[test]
    fn escape_opens_a_closed_prompt_and_closes_an_open_one_without_leaving() {
        assert_eq!(
            LeavePrompt::Closed.press(RoomKey::Escape),
            PromptStep::Stay(LeavePrompt::Open)
        );
        assert_eq!(
            LeavePrompt::Open.press(RoomKey::Escape),
            PromptStep::Stay(LeavePrompt::Closed)
        );
    }

    #[test]
    fn enter_leaves_only_while_the_prompt_is_open() {
        assert_eq!(LeavePrompt::Open.press(RoomKey::Enter), PromptStep::Leave);
        assert_eq!(
            LeavePrompt::Closed.press(RoomKey::Enter),
            PromptStep::Stay(LeavePrompt::Closed),
            "Enter without the prompt is not a way out"
        );
    }
}
