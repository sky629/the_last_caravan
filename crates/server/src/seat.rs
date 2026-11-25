//! 자리 하나입니다. 방에 있는 플레이어 한 명과, 그 플레이어에게 닿는 방법(이름, 표식, 접속, 입력)입니다.
//!
//! 자리의 상태는 따로 저장하지 않고 세 값(`link`, `quiet_ticks`, `dormant`)에서 정합니다. 그래서 세 값과 어긋난 상태가 없습니다.
//!
//! ```text
//!                       입력이 옴 (quiet_ticks = 0)
//!   자리에 있음 <------------------------------------ 자리를 비움 (접속은 열림)
//!     |   quiet_ticks >= away_ticks                     ^
//!     +-------------------------------------------------+
//!     |   접속이 닫힘 (끊김, 서버가 무응답으로 닫음)
//!     +-----------------------------> 자리를 비움 (접속 없음) --quiet_ticks >= hold_ticks--> 자리가 끝남 (퇴장과 같음)
//!
//!   자리에 있음 --(야영지에서 still_ticks >= camp_idle_ticks)--> 자리를 비움 (접속은 열림, dormant)
//!   자리를 비움 (dormant) --(조작이 옴)--> 자리에 있음
//!
//!   (어느 상태에서든) 같은 표식으로 입장 --> 자리에 있음 (새 접속. 이전 접속은 SeatTakenOver를 받음)
//!   (어느 상태에서든) Leave --> 자리가 끝남 (곧바로)
//! ```
//!
//! `quiet_ticks`는 이 자리의 접속에서 마지막 메시지가 온 뒤 지나간 서버 Tick이고, 방이 멈춘 동안에도 늘어납니다.
//! 그래서 60초는 입력이 멈춘 때부터 세고, 접속이 끊긴 자리와 접속은 열려 있지만 조용한 자리가 같은 시계를 씁니다.
//!
//! `still_ticks`는 마지막 조작(입력의 내용이 바뀐 것) 뒤 지나간 서버 Tick입니다. 입력이 계속 와도 내용이 같으면 늘어나므로,
//! 창을 켜 둔 채 자리를 뜬 사람을 알아봅니다. 이 시간은 자리를 내놓는 시간(`quiet_ticks`)에는 들지 않아서 자리는 그대로 남습니다.
//! `dormant`는 야영지에서 `still_ticks`가 한도에 닿으면 켜지고, 조작이 오면 꺼집니다. 원정이 시작되어도 켜진 채로 이어지므로,
//! 그 사람 없이 시작한 원정에서 캐릭터는 자리를 비운 사람처럼 보호받다가 조작이 오면 합류합니다.

use caravan_protocol::{LinkId, PlayerName, SeatToken};
use caravan_rules::Presence;

use crate::input::MemberInput;

pub(crate) struct Seat {
    pub name: PlayerName,
    pub token: SeatToken,
    /// 지금 이 자리를 쓰는 접속입니다. 접속이 닫혔으면 `None`이고, 같은 표식으로 다시 입장하면 새 접속이 들어옵니다.
    pub link: Option<LinkId>,
    quiet_ticks: u32,
    still_ticks: u32,
    dormant: bool,
    pub input: MemberInput,
}

impl Seat {
    pub fn new(name: PlayerName, token: SeatToken, link: LinkId) -> Seat {
        Seat {
            name,
            token,
            link: Some(link),
            quiet_ticks: 0,
            still_ticks: 0,
            dormant: false,
            input: MemberInput::new(),
        }
    }

    /// 자리에 있는 것은 접속이 있고, `away_ticks`보다 짧게 조용했고, 야영지에서 조작이 없어 자리를 비운 것이 아닐 때뿐입니다.
    pub fn presence(&self, away_ticks: u32) -> Presence {
        if self.link.is_some() && self.quiet_ticks < away_ticks && !self.dormant {
            Presence::Here
        } else {
            Presence::Away
        }
    }

    /// 자리를 내놓을 때가 되었습니다.
    pub fn is_over(&self, hold_ticks: u32) -> bool {
        self.quiet_ticks >= hold_ticks
    }

    /// 이 자리의 접속에서 메시지가 왔습니다. 버려지는 입력(순서 번호가 오래된 입력)도 접속이 살아 있다는 표시입니다.
    pub fn heard(&mut self) {
        self.quiet_ticks = 0;
    }

    /// 이 자리의 접속에서 조작(입력의 내용이 바뀜)이 왔습니다. 야영지에서 자리를 비웠던 것도 끝납니다.
    pub fn operated(&mut self) {
        self.still_ticks = 0;
        self.dormant = false;
    }

    /// 서버 Tick 하나가 지났습니다. 방이 멈춘 Tick에도 부릅니다.
    pub fn pass_tick(&mut self) {
        self.quiet_ticks = self.quiet_ticks.saturating_add(1);
        self.still_ticks = self.still_ticks.saturating_add(1);
    }

    /// 야영지의 Tick마다 부릅니다. 조작 없이 `camp_idle_ticks`가 지났으면 자리를 비운 것으로 하고, 그 Tick에 처음 그렇게 되었으면 참을 돌려줍니다.
    pub fn doze_off(&mut self, camp_idle_ticks: u32) -> bool {
        let dozes = !self.dormant && self.still_ticks >= camp_idle_ticks;
        self.dormant |= dozes;
        dozes
    }

    /// `link`가 닫혔습니다. 이 자리의 접속이 그것이었으면 접속을 비우고 참을 돌려줍니다.
    /// 이미 다른 접속이 이어받은 자리이면 아무것도 하지 않고 거짓을 돌려줍니다.
    pub fn close_link(&mut self, link: LinkId) -> bool {
        let is_current = self.link == Some(link);
        if is_current {
            self.link = None;
        }
        is_current
    }

    /// 같은 표식으로 들어온 새 접속이 자리를 이어받습니다. 이전 접속이 있으면 돌려줍니다. 이전 접속에 알리고 처리하는 것은 방이 아니라 허브의 일입니다.
    /// 돌아온 사람이 이름을 바꿨으면 새 이름을 씁니다. 새 클라이언트는 입력의 순서 번호를 1부터 다시 세므로 입력 상태를 새로 만듭니다.
    pub fn take_over(&mut self, link: LinkId, name: PlayerName) -> Option<LinkId> {
        let old = self.link.replace(link);
        self.name = name;
        self.input = MemberInput::new();
        self.heard();
        self.operated();
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWAY: u32 = 60;
    const HOLD: u32 = 1200;

    fn seat() -> Seat {
        Seat::new(
            PlayerName::parse("KANG").unwrap(),
            SeatToken([1; 16]),
            LinkId(1),
        )
    }

    fn pass(seat: &mut Seat, ticks: u32) {
        for _ in 0..ticks {
            seat.pass_tick();
        }
    }

    #[test]
    fn a_seat_with_a_link_is_here_until_it_has_been_quiet_for_the_away_time() {
        let mut seat = seat();
        pass(&mut seat, AWAY - 1);
        assert_eq!(seat.presence(AWAY), Presence::Here);

        seat.pass_tick();
        assert_eq!(seat.presence(AWAY), Presence::Away);
        assert!(!seat.is_over(HOLD), "away is not over");

        seat.heard();
        assert_eq!(
            seat.presence(AWAY),
            Presence::Here,
            "the same link comes back"
        );
    }

    #[test]
    fn a_seat_without_a_link_is_away_at_once_and_ends_by_the_clock_of_the_last_message() {
        let mut seat = seat();
        pass(&mut seat, 10);
        assert!(seat.close_link(LinkId(1)));
        assert_eq!(seat.presence(AWAY), Presence::Away);

        pass(&mut seat, HOLD - 11);
        assert!(!seat.is_over(HOLD));
        seat.pass_tick();
        assert!(
            seat.is_over(HOLD),
            "counted from the last message, not from the close"
        );
    }

    #[test]
    fn a_seat_that_does_nothing_in_the_camp_is_away_until_an_operation_and_keeps_its_place() {
        const CAMP_IDLE: u32 = 1200;
        let mut seat = seat();
        pass(&mut seat, CAMP_IDLE - 1);
        seat.heard();
        assert!(!seat.doze_off(CAMP_IDLE), "not long enough yet");
        assert_eq!(seat.presence(AWAY), Presence::Here);

        seat.pass_tick();
        assert!(
            seat.doze_off(CAMP_IDLE),
            "messages without an operation do not count"
        );
        assert!(!seat.doze_off(CAMP_IDLE), "it is reported once");
        assert_eq!(seat.presence(AWAY), Presence::Away);
        assert!(!seat.is_over(HOLD), "the place is not given up");

        seat.heard();
        assert_eq!(
            seat.presence(AWAY),
            Presence::Away,
            "a message that changes nothing does not bring it back"
        );

        seat.operated();
        assert_eq!(seat.presence(AWAY), Presence::Here);
        pass(&mut seat, 10);
        assert!(!seat.doze_off(CAMP_IDLE), "the time starts over");
    }

    #[test]
    fn closing_a_link_the_seat_no_longer_uses_changes_nothing() {
        let mut seat = seat();
        let old = seat.take_over(LinkId(2), PlayerName::parse("KANG").unwrap());
        assert_eq!(old, Some(LinkId(1)));

        assert!(
            !seat.close_link(LinkId(1)),
            "the old link was already replaced"
        );
        assert_eq!(seat.link, Some(LinkId(2)));
        assert_eq!(seat.presence(AWAY), Presence::Here);
    }

    #[test]
    fn taking_over_starts_the_input_and_the_clock_over_and_takes_the_new_name() {
        let mut seat = seat();
        seat.input.accept(&caravan_protocol::InputMsg {
            seq: 50,
            ..Default::default()
        });
        pass(&mut seat, 500);
        assert!(seat.doze_off(500), "it had been away in the camp");
        seat.close_link(LinkId(1));

        let old = seat.take_over(LinkId(7), PlayerName::parse("MIRO").unwrap());

        assert_eq!(old, None, "no link was left to replace");
        assert_eq!(seat.name.as_str(), "MIRO");
        assert_eq!(
            seat.presence(AWAY),
            Presence::Here,
            "the person who came back is at the keyboard"
        );
        assert!(!seat.doze_off(500), "the time starts over");
        seat.input.accept(&caravan_protocol::InputMsg {
            seq: 1,
            attack: true,
            ..Default::default()
        });
        assert!(
            seat.input.for_tick(10).attack,
            "the new client counts its inputs from 1 again"
        );
    }
}
