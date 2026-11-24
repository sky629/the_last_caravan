//! 조작을 입력 메시지로 바꾸고, 서버 Tick마다 하나씩 내보냅니다.
//!
//! 키보드와 마우스를 읽는 일은 Bevy 쪽이 하고, 이 파일은 읽은 값을 `InputMsg`로 바꾸는 규칙과 보내는 때를 정합니다.
//! 입력 메시지에는 위치도 피해량도 없고 방향과 버튼뿐입니다.

use caravan_protocol::{InputMsg, WireVec2};
use caravan_rules::Vec2;

use crate::iso;

/// 프레임마다 읽은 조작 상태입니다. 아무것도 누르지 않은 상태가 기본값이며, 이 값을 보내면 서버에서 중립 입력이 됩니다.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// WASD입니다. 화면을 기준으로 누른 방향이며, 지면의 이동 방향으로 바꾸는 일은 `to_input`이 합니다.
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    /// 마우스 커서가 가리키는 지면 위치(타일)입니다. 커서가 창 밖에 있으면 없습니다.
    pub aim_at: Option<Vec2>,
    /// 마우스 왼쪽 버튼입니다.
    pub attack: bool,
    /// F 키입니다. 누르고 있는 동안 부활과 수리를 진행합니다.
    pub interact: bool,
    /// B 키입니다. 누르고 있는 동안 바라보는 쪽 타일에 감시탑을 짓습니다.
    pub build: bool,
}

impl Controls {
    /// `me`는 내 캐릭터가 화면에 그려진 지면 위치입니다. 조준 방향은 그 위치에서 커서가 가리키는 위치로 향합니다.
    /// 내 위치를 모르거나 커서가 창 밖이면 조준 방향은 0이고, 서버는 지금 보는 방향을 유지합니다.
    pub fn to_input(&self, seq: u32, me: Option<Vec2>) -> InputMsg {
        let screen_dir = Vec2::new(axis(self.right, self.left), axis(self.up, self.down));
        let move_dir = iso::screen_dir_to_ground(screen_dir);
        let aim = match (self.aim_at, me) {
            (Some(target), Some(me)) => (target - me).normalize_or_zero(),
            _ => Vec2::ZERO,
        };
        InputMsg {
            seq,
            move_dir: WireVec2::new(move_dir.x, move_dir.y),
            aim: WireVec2::new(aim.x, aim.y),
            attack: self.attack,
            interact: self.interact,
            build: self.build,
        }
    }
}

fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(positive) - f32::from(negative)
}

/// 입력에 순서 번호를 붙이고, 서버 Tick 간격마다 하나씩 내보냅니다.
/// 서버는 일정 시간 동안 메시지가 없는 접속을 닫으므로, 아무것도 누르지 않아도 중립 입력을 보내서 접속이 살아 있음을 알립니다.
#[derive(Clone, Debug, PartialEq)]
pub struct InputSender {
    period_secs: f64,
    /// 다음에 보낼 입력의 순서 번호입니다. 서버는 이미 받은 번호보다 크지 않은 입력을 버립니다.
    next_seq: u32,
    /// 다음 입력을 보낼 시각입니다 (앱이 시작된 뒤 지난 초).
    next_at: f64,
    /// 공격 버튼이 눌렸다가 입력을 보내기 전에 떼어졌는지 기억합니다.
    attack_latched: bool,
}

impl InputSender {
    pub fn new(tick_hz: u32) -> InputSender {
        InputSender {
            period_secs: 1.0 / f64::from(tick_hz),
            next_seq: 1,
            next_at: 0.0,
            attack_latched: false,
        }
    }

    /// 프레임마다 부릅니다. 보낼 때가 되었으면 입력 메시지를 돌려줍니다.
    ///
    /// 프레임은 입력보다 자주 돌므로, 두 입력 사이에 눌렸다가 떼어진 클릭도 다음 입력에 공격으로 실어서 잃지 않게 합니다.
    pub fn poll(
        &mut self,
        now_secs: f64,
        controls: &Controls,
        me: Option<Vec2>,
    ) -> Option<InputMsg> {
        self.attack_latched |= controls.attack;
        if now_secs < self.next_at {
            return None;
        }
        // 프레임이 오래 멈췄다가 돌아와도 밀린 입력을 몰아서 보내지 않습니다.
        self.next_at = if self.next_at + self.period_secs > now_secs {
            self.next_at + self.period_secs
        } else {
            now_secs + self.period_secs
        };
        let held = Controls {
            attack: self.attack_latched,
            ..*controls
        };
        let input = held.to_input(self.next_seq, me);
        self.next_seq += 1;
        self.attack_latched = false;
        Some(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: WireVec2, x: f32, y: f32) -> bool {
        (a.x - x).abs() < 1e-5 && (a.y - y).abs() < 1e-5
    }

    const HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn pressing_nothing_gives_a_neutral_input() {
        let input = Controls::default().to_input(7, Some(Vec2::new(5.0, 5.0)));
        assert_eq!(
            input,
            InputMsg {
                seq: 7,
                ..InputMsg::default()
            }
        );
    }

    #[test]
    fn w_moves_up_the_screen_which_is_diagonal_on_the_ground() {
        let controls = Controls {
            up: true,
            ..Controls::default()
        };
        let input = controls.to_input(1, None);
        assert!(close(input.move_dir, HALF, HALF), "{:?}", input.move_dir);
    }

    #[test]
    fn two_keys_give_a_ground_direction_of_length_one() {
        let up_right = Controls {
            up: true,
            right: true,
            ..Controls::default()
        };
        let input = up_right.to_input(1, None);
        assert!(close(input.move_dir, 1.0, 0.0), "{:?}", input.move_dir);
    }

    #[test]
    fn opposite_keys_cancel_out() {
        let controls = Controls {
            up: true,
            down: true,
            left: true,
            right: true,
            ..Controls::default()
        };
        assert_eq!(controls.to_input(1, None).move_dir, WireVec2::ZERO);
    }

    #[test]
    fn buttons_are_passed_through() {
        let controls = Controls {
            attack: true,
            interact: true,
            build: true,
            ..Controls::default()
        };
        let input = controls.to_input(1, None);
        assert!(input.attack && input.interact && input.build);
    }

    #[test]
    fn the_aim_points_from_my_character_to_the_cursor() {
        let controls = Controls {
            aim_at: Some(Vec2::new(10.0, 15.0)),
            ..Controls::default()
        };
        let input = controls.to_input(1, Some(Vec2::new(10.0, 10.0)));
        assert!(close(input.aim, 0.0, 1.0), "{:?}", input.aim);
    }

    #[test]
    fn without_a_cursor_or_my_position_the_aim_is_zero() {
        let on_the_map = Some(Vec2::new(10.0, 15.0));
        let no_cursor = Controls::default().to_input(1, Some(Vec2::new(10.0, 10.0)));
        assert_eq!(no_cursor.aim, WireVec2::ZERO);
        let unknown_me = Controls {
            aim_at: on_the_map,
            ..Controls::default()
        }
        .to_input(1, None);
        assert_eq!(unknown_me.aim, WireVec2::ZERO);
    }

    fn seq_of(input: Option<InputMsg>) -> Option<u32> {
        input.map(|input| input.seq)
    }

    #[test]
    fn one_input_is_due_per_server_tick_with_an_increasing_seq() {
        let mut sender = InputSender::new(20);
        let idle = Controls::default();

        assert_eq!(seq_of(sender.poll(1.000, &idle, None)), Some(1));
        assert_eq!(seq_of(sender.poll(1.020, &idle, None)), None);
        assert_eq!(seq_of(sender.poll(1.051, &idle, None)), Some(2));
        assert_eq!(seq_of(sender.poll(1.060, &idle, None)), None);
    }

    #[test]
    fn a_stalled_frame_does_not_cause_a_burst_of_inputs() {
        let mut sender = InputSender::new(20);
        let idle = Controls::default();
        sender.poll(1.0, &idle, None);

        assert_eq!(seq_of(sender.poll(9.000, &idle, None)), Some(2));
        assert_eq!(seq_of(sender.poll(9.010, &idle, None)), None);
        assert_eq!(seq_of(sender.poll(9.051, &idle, None)), Some(3));
    }

    #[test]
    fn a_click_released_between_two_inputs_is_still_sent_once() {
        let mut sender = InputSender::new(20);
        let idle = Controls::default();
        let click = Controls {
            attack: true,
            ..Controls::default()
        };
        sender.poll(1.00, &idle, None);

        assert_eq!(sender.poll(1.01, &click, None), None);
        assert_eq!(sender.poll(1.02, &idle, None), None);
        let sent = sender.poll(1.06, &idle, None).unwrap();
        assert!(sent.attack, "the click that already ended is not lost");
        let next = sender.poll(1.12, &idle, None).unwrap();
        assert!(!next.attack, "and it is sent only once");
    }
}
