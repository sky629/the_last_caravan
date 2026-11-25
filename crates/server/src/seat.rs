//! 자리 하나입니다. 방에 있는 플레이어 한 명과, 그 플레이어에게 닿는 방법(이름, 접속, 입력)입니다.
//! 이름은 서버의 자리에만 있고 규칙은 알지 못합니다.

use caravan_protocol::{LinkId, PlayerName};

use crate::input::MemberInput;

pub(crate) struct Seat {
    pub name: PlayerName,
    pub link: LinkId,
    pub input: MemberInput,
}

impl Seat {
    pub fn new(name: PlayerName, link: LinkId) -> Seat {
        Seat {
            name,
            link,
            input: MemberInput::new(),
        }
    }
}
