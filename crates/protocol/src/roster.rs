//! 방의 명단입니다. 이름은 명단이 바뀔 때만 보내므로 Tick마다 보내는 스냅샷의 크기는 이름의 길이와 관계가 없습니다.

use serde::{Deserialize, Serialize};

/// 방의 명단 하나입니다. 번호가 작은 플레이어부터입니다.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRoster {
    /// 방이 명단을 바꿀 때마다 1씩 커지는 번호입니다. 첫 명단이 1입니다.
    /// 브라우저는 메시지를 보낸 순서와 다르게 넘겨줄 수 있으므로, 받는 쪽은 번호가 더 큰 명단만 받아들입니다.
    pub revision: u32,
    pub members: Vec<WireMember>,
}

/// 방에 있는 사람 하나입니다. 네 명이 모두 12자 이름이면 명단 하나가 약 60바이트입니다.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireMember {
    /// 플레이어 번호입니다. 스냅샷의 `WirePlayer::id`와 같습니다.
    pub id: u32,
    /// 서버가 `PlayerName`으로 검사한 이름입니다.
    pub name: String,
}
