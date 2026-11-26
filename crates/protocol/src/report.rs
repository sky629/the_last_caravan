//! 끝난 원정의 결과 화면입니다. 원정이 끝나는 Tick에 한 번 만들어 보내고, Tick마다 보내는 스냅샷에는 싣지 않습니다.
//! 그래서 Tick마다 보내는 크기가 기록이나 이름의 길이와 관계가 없습니다. 결과 화면 중에 들어오거나 돌아온 접속도 한 번 받습니다.

use caravan_rules::{PlayerId, Report};
use serde::{Deserialize, Serialize};

/// 끝난 원정의 요약과 개인별 기록입니다. 원정이 성공했는지 실패했는지는 스냅샷의 결과 화면 단계(`WirePhase::Debrief`)에 있습니다.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReport {
    /// 이 결과가 방의 몇 번째 원정의 것인지입니다. 스냅샷의 `expedition`과 같습니다.
    /// 브라우저는 메시지를 보낸 순서와 다르게 넘겨줄 수 있으므로, 받는 쪽은 이 번호가 지금 그리는 원정과 같을 때만 결과를 보여 줍니다.
    pub expedition: u32,
    /// 원정이 끝날 때의 날입니다(버틴 날). 성공이면 원정의 날 수입니다.
    pub day: u32,
    /// 원정이 걸린 시간(초)입니다. 초 아래는 버립니다.
    pub elapsed_secs: u32,
    /// 감시탑이 마지막 피해를 주어 처치한 몬스터의 수입니다.
    pub tower_kills: u32,
    /// 원정이 끝날 때 방에 있던 플레이어의 기록입니다. 번호가 작은 플레이어부터이고, 자리를 비운 플레이어도 있습니다.
    pub rows: Vec<WireRecordRow>,
}

/// 플레이어 한 명의 기록입니다. 네 명이 모두 12자 이름이면 결과 하나가 84바이트입니다.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRecordRow {
    /// 서버가 `PlayerName`으로 검사한 이름입니다. 결과가 열린 때의 이름이므로, 그 뒤에 방을 나가도 남습니다.
    pub name: String,
    /// 마지막 피해를 주어 처치한 몬스터의 수입니다.
    pub kills: u32,
    /// 부활을 끝까지 마친 횟수입니다.
    pub revives: u32,
    /// 쓰러진 횟수입니다.
    pub downs: u32,
    /// 마차의 보관함에 넣은 자원의 양입니다.
    pub deposited: u32,
}

impl WireReport {
    /// 끝난 원정의 결과를 전송 값으로 바꿉니다. `expedition`은 방의 몇 번째 원정인지이고, `tick_hz`는 걸린 시간을 초로 바꾸는 데 씁니다.
    /// 이름은 규칙이 알지 못하므로 `name_of`가 플레이어마다 알려 줍니다.
    pub fn from_report(
        report: &Report,
        expedition: u32,
        tick_hz: u32,
        name_of: impl Fn(PlayerId) -> String,
    ) -> WireReport {
        WireReport {
            expedition,
            day: report.day,
            elapsed_secs: u32::try_from(report.elapsed_ticks / u64::from(tick_hz))
                .unwrap_or(u32::MAX),
            tower_kills: report.tower_kills,
            rows: report
                .rows
                .iter()
                .map(|(player, record)| WireRecordRow {
                    name: name_of(*player),
                    kills: record.kills,
                    revives: record.revives,
                    downs: record.downs,
                    deposited: record.deposited,
                })
                .collect(),
        }
    }
}
