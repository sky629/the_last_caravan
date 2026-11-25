//! 플레이어 한 명의 입력을 모아서 Tick마다 규칙에 넘길 `PlayerInput` 하나로 만듭니다.
//!
//! 클라이언트의 입력은 Tick과 맞지 않는 때에 도착하고, 순서가 바뀌거나 빠지거나 멈출 수 있습니다.
//! 이 파일은 그 불규칙한 입력을 Tick마다 하나씩 규칙에 넘기는 규칙적인 흐름으로 바꿉니다.
//! 메시지의 값은 믿을 수 없으므로, 규칙의 `MoveDir::new`와 `AimDir::new`로 검사하고 고친 뒤에만 규칙에 들어갑니다.

use caravan_protocol::InputMsg;
use caravan_rules::{AimDir, MoveDir, PlayerInput};

/// 플레이어 한 명의 입력 상태입니다.
pub(crate) struct MemberInput {
    /// 지금까지 받아들인 입력 가운데 가장 큰 순서 번호입니다. 아직 받아들인 입력이 없으면 `None`입니다.
    newest_seq: Option<u32>,
    /// 지난 Tick 이후 받아들인 입력을 하나로 합친 값입니다. 새 입력이 없었으면 `None`입니다.
    merged: Option<PlayerInput>,
    /// 가장 최근에 받아들인 입력에서 누른 순간(`switch_weapon`, `interact_pressed`)을 지운 것입니다. 새 입력이 없는 Tick에 다시 사용합니다.
    held: PlayerInput,
    /// 새 입력 없이 지나간 Tick의 수입니다.
    silent_ticks: u32,
}

impl MemberInput {
    /// 입력을 한 번도 받지 않은 상태입니다. 이때 플레이어는 아무것도 누르지 않은 것으로 움직입니다.
    pub fn new() -> MemberInput {
        MemberInput {
            newest_seq: None,
            merged: None,
            held: PlayerInput::default(),
            silent_ticks: 0,
        }
    }

    /// 입력 메시지 하나를 받습니다. 이미 받아들인 순서 번호보다 크지 않은 입력은 버립니다.
    /// 접속 방식에 따라 입력의 도착 순서가 바뀔 수 있으므로, 늦게 도착한 옛 입력이 새 입력을 덮어쓰면 안 됩니다.
    pub fn accept(&mut self, msg: &InputMsg) {
        if self.newest_seq.is_some_and(|newest| msg.seq <= newest) {
            return;
        }
        self.newest_seq = Some(msg.seq);
        let input = to_rules(msg);
        self.merged = Some(match self.merged {
            Some(older) => merge(older, input),
            None => input,
        });
        self.held = input.held();
    }

    /// 이번 Tick에 규칙에 넘길 입력을 정하고, 다음 Tick을 위해 모은 입력을 비웁니다.
    ///
    /// - 지난 Tick 이후 받아들인 입력이 있으면 그것들을 합친 값입니다.
    /// - 없으면 가장 최근 입력을 다시 씁니다. 키를 누르고 있는 동안 입력이 한두 Tick 늦어도 캐릭터가 멈추지 않습니다.
    ///   다시 쓰는 입력에는 키를 누른 순간(Q, F)이 없으므로, 한 번 누른 키가 Tick마다 되풀이되지 않습니다.
    /// - 그렇게 `stale_ticks`번을 넘겨서 입력이 없으면 아무것도 누르지 않은 입력입니다.
    ///   백그라운드에서 실행이 제한된 브라우저 탭의 캐릭터가 마지막 키를 누른 채로 계속 달리지 않게 합니다.
    pub fn for_tick(&mut self, stale_ticks: u32) -> PlayerInput {
        if let Some(merged) = self.merged.take() {
            self.silent_ticks = 0;
            return merged;
        }
        self.silent_ticks = self.silent_ticks.saturating_add(1);
        if self.silent_ticks > stale_ticks {
            // 순서 번호는 이어 갑니다. 0으로 돌아가면 클라이언트가 서버가 어디까지 처리했는지 잘못 알게 됩니다.
            PlayerInput {
                seq: self.held.seq,
                ..PlayerInput::default()
            }
        } else {
            self.held
        }
    }
}

/// 메시지의 값을 규칙이 받는 값으로 바꿉니다. 믿을 수 없는 값(NaN, 무한대, 너무 긴 방향)은 여기서 고쳐집니다.
fn to_rules(msg: &InputMsg) -> PlayerInput {
    PlayerInput {
        seq: msg.seq,
        move_dir: MoveDir::new(msg.move_dir.x, msg.move_dir.y),
        aim: AimDir::new(msg.aim.x, msg.aim.y),
        attack: msg.attack,
        interact: msg.interact,
        interact_pressed: msg.interact_pressed,
        build: msg.build,
        sprint: msg.sprint,
        switch_weapon: msg.switch_weapon,
    }
}

/// 같은 Tick 안에 도착한 두 입력을 합칩니다. 이동, 조준, `interact`, `build`, `sprint`는 더 새로운 입력을 따릅니다.
/// `interact`와 `build`와 `sprint`는 누르고 있는 동안 이어지는 상태이므로, Tick 안에서 눌렀다 뗐다면 뗀 것으로 봅니다.
/// `attack`, `switch_weapon`, `interact_pressed`는 하나라도 눌렸으면 눌린 것으로 봅니다. Tick보다 짧게 누른 클릭과 키가 사라지지 않게 합니다.
/// 누른 순간인 조작을 더하면 `PlayerInput::held`가 그 값을 지우게 하고, 여기에도 `||`를 한 줄 더합니다.
fn merge(older: PlayerInput, newer: PlayerInput) -> PlayerInput {
    PlayerInput {
        attack: older.attack || newer.attack,
        switch_weapon: older.switch_weapon || newer.switch_weapon,
        interact_pressed: older.interact_pressed || newer.interact_pressed,
        ..newer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(seq: u32, attack: bool, interact: bool) -> InputMsg {
        InputMsg {
            seq,
            attack,
            interact,
            ..InputMsg::default()
        }
    }

    #[test]
    fn a_key_released_within_the_tick_is_released_but_a_short_click_still_counts() {
        let mut input = MemberInput::new();
        input.accept(&msg(1, true, true));
        input.accept(&msg(2, false, false));

        let tick = input.for_tick(10);

        assert!(!tick.interact, "F was already released when the tick came");
        assert!(tick.attack, "a click shorter than a tick is not lost");
    }

    #[test]
    fn a_key_pressed_within_the_tick_is_pressed() {
        let mut input = MemberInput::new();
        input.accept(&msg(1, false, false));
        input.accept(&msg(2, false, true));

        let tick = input.for_tick(10);

        assert!(tick.interact);
        assert!(!tick.attack);
    }

    #[test]
    fn the_sprint_key_follows_the_newest_input_like_the_other_held_keys() {
        let mut input = MemberInput::new();
        input.accept(&InputMsg {
            seq: 1,
            sprint: true,
            ..InputMsg::default()
        });
        assert!(input.for_tick(10).sprint, "the message reaches the rules");

        input.accept(&InputMsg {
            seq: 2,
            sprint: true,
            ..InputMsg::default()
        });
        input.accept(&InputMsg {
            seq: 3,
            sprint: false,
            ..InputMsg::default()
        });
        assert!(!input.for_tick(10).sprint, "Shift was already released");

        input.accept(&InputMsg {
            seq: 4,
            sprint: true,
            ..InputMsg::default()
        });
        input.for_tick(10);
        assert!(
            input.for_tick(10).sprint,
            "held on a tick without a message"
        );
    }

    #[test]
    fn the_build_key_follows_the_newest_input_like_the_interact_key() {
        let mut input = MemberInput::new();
        input.accept(&InputMsg {
            seq: 1,
            build: true,
            ..InputMsg::default()
        });
        input.accept(&InputMsg {
            seq: 2,
            build: false,
            ..InputMsg::default()
        });
        assert!(!input.for_tick(10).build, "B was already released");

        input.accept(&InputMsg {
            seq: 3,
            build: true,
            ..InputMsg::default()
        });
        assert!(input.for_tick(10).build);
    }

    fn q(seq: u32) -> InputMsg {
        InputMsg {
            seq,
            switch_weapon: true,
            ..InputMsg::default()
        }
    }

    #[test]
    fn a_press_of_q_reaches_the_rules_in_exactly_one_tick() {
        let mut input = MemberInput::new();
        input.accept(&InputMsg {
            attack: true,
            ..q(1)
        });

        let first = input.for_tick(10);
        assert!(first.switch_weapon, "the press reaches the rules");

        let second = input.for_tick(10);
        assert!(
            !second.switch_weapon,
            "a tick without a message does not press it again"
        );
        assert!(second.attack, "the held click is still reused");
    }

    #[test]
    fn a_press_of_q_that_was_followed_by_another_input_in_the_same_tick_is_kept() {
        let mut input = MemberInput::new();
        input.accept(&q(1));
        input.accept(&msg(2, false, false));

        assert!(input.for_tick(10).switch_weapon);
        assert!(!input.for_tick(10).switch_weapon);
    }

    #[test]
    fn a_late_or_repeated_message_with_q_does_not_press_it_again() {
        let mut input = MemberInput::new();
        input.accept(&msg(5, false, false));
        input.for_tick(10);

        input.accept(&q(3));
        input.accept(&q(5));

        assert!(
            !input.for_tick(10).switch_weapon,
            "neither is newer than seq 5"
        );
    }

    fn f(seq: u32) -> InputMsg {
        InputMsg {
            seq,
            interact: true,
            interact_pressed: true,
            ..InputMsg::default()
        }
    }

    #[test]
    fn a_press_of_f_reaches_the_rules_in_exactly_one_tick_and_the_hold_goes_on() {
        let mut input = MemberInput::new();
        input.accept(&f(1));

        let first = input.for_tick(10);
        assert!(first.interact_pressed, "the press reaches the rules");
        assert!(first.interact);

        let second = input.for_tick(10);
        assert!(
            !second.interact_pressed,
            "a late tick does not press it again"
        );
        assert!(second.interact, "but F is still held");
    }

    #[test]
    fn a_tap_of_f_that_ended_inside_the_tick_is_kept_as_a_press_but_not_as_a_hold() {
        let mut input = MemberInput::new();
        input.accept(&f(1));
        input.accept(&msg(2, false, false));

        let tick = input.for_tick(10);

        assert!(tick.interact_pressed);
        assert!(!tick.interact, "F was already released when the tick came");
    }

    #[test]
    fn a_late_or_repeated_message_with_f_does_not_press_it_again() {
        let mut input = MemberInput::new();
        input.accept(&msg(5, false, false));
        input.for_tick(10);

        input.accept(&f(3));
        input.accept(&f(5));

        assert!(!input.for_tick(10).interact_pressed);
    }

    #[test]
    fn two_presses_of_q_in_one_tick_count_once() {
        let mut input = MemberInput::new();
        input.accept(&q(1));
        input.accept(&q(2));

        assert!(input.for_tick(10).switch_weapon);
        assert!(!input.for_tick(10).switch_weapon);
    }
}
