//! 결과 화면입니다. 원정이 끝나면 세계가 멈추고, 모두가 확인하거나 정해진 시간이 지나면 야영지로 돌아갑니다.
//!
//! ```text
//!   원정이 끝남 --> 결과 화면 --(모두가 확인함, 또는 debrief_secs가 지남)--> 야영지
//! ```

use std::collections::BTreeSet;

use crate::camp::who_must_agree;
use crate::expedition::Outcome;
use crate::ids::PlayerId;
use crate::input::PlayerInputs;
use crate::tuning::Tuning;
use crate::world::World;

/// 끝난 원정의 요약입니다. 원정이 끝나는 Tick에 한 번 만들고 바뀌지 않습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub outcome: Outcome,
    /// 원정이 끝날 때의 날입니다(버틴 날). 성공이면 원정의 날 수입니다.
    pub day: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Debrief {
    report: Report,
    confirmed: BTreeSet<PlayerId>,
    /// 결과 화면이 끝나기까지 남은 Tick입니다. 모두가 확인하면 그 전에 끝납니다.
    ticks_left: u32,
}

impl Debrief {
    pub(crate) fn new(report: Report, t: &Tuning) -> Debrief {
        Debrief {
            report,
            confirmed: BTreeSet::new(),
            ticks_left: t.ticks(t.camp.debrief_secs),
        }
    }

    pub fn outcome(&self) -> Outcome {
        self.report.outcome
    }

    pub fn report(&self) -> &Report {
        &self.report
    }

    /// 확인한 플레이어를 번호 순서로 돌려줍니다.
    pub fn confirmed(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.confirmed.iter().copied()
    }

    pub fn ticks_left(&self) -> u32 {
        self.ticks_left
    }

    /// 확인 키를 반영하고 시간을 줄입니다. 결과 화면을 끝낼 때가 되었으면 참입니다.
    /// 확인은 한 번 켜면 끌 수 없으므로 같은 키가 두 번 와도 결과가 같습니다. 방에 아무도 없으면 시간이 다 지나야 끝납니다.
    pub(crate) fn advance(&mut self, world: &World, inputs: &PlayerInputs) -> bool {
        for (id, input) in inputs {
            if input.confirm && world.players.contains_key(id) {
                self.confirmed.insert(*id);
            }
        }
        self.ticks_left = self.ticks_left.saturating_sub(1);
        let mut needed = who_must_agree(world).peekable();
        let everyone =
            needed.peek().is_some() && needed.all(|player| self.confirmed.contains(&player.id));
        everyone || self.ticks_left == 0
    }

    /// 방을 떠난 플레이어의 확인을 지웁니다.
    pub(crate) fn forget(&mut self, id: PlayerId) {
        self.confirmed.remove(&id);
    }
}
