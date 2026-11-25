//! 야영지입니다. 원정 앞에 모이는 곳이고, 모두가 준비하면 정해진 시간 뒤에 원정이 시작됩니다.
//!
//! 야영지는 원정과 같은 지도입니다. 세계를 야영지의 모습으로 놓는 일은 `World::restart(Layout::Camp)`가 하고,
//! 이 파일은 준비, 카운트다운, 허수아비가 맞은 피해의 기록만 다룹니다.
//!
//! ```text
//!   모으는 중 --(모두가 준비함)--> 카운트다운 --(countdown_secs가 지남)--> 원정의 첫날
//!   카운트다운 --(누가 준비를 끔, 준비하지 않은 사람이 들어옴)--> 모으는 중
//! ```
//!
//! 카운트다운은 Tick마다, 그리고 방의 사람이 바뀔 때마다 「모두가 준비했는가」를 다시 보고 정하므로,
//! 모두가 준비하지 않은 채로 카운트다운이 남아 있을 수 없습니다.

use std::collections::{BTreeMap, BTreeSet};

use crate::ids::{MonsterId, PlayerId};
use crate::input::PlayerInputs;
use crate::player::Player;
use crate::tuning::Tuning;
use crate::world::{Hit, Target, World};

#[derive(Clone, Debug, PartialEq)]
pub struct Camp {
    ready: BTreeSet<PlayerId>,
    /// 원정이 시작되기까지 남은 Tick입니다. 모두가 준비한 동안만 값이 있습니다.
    countdown: Option<u32>,
    /// 허수아비마다 마지막으로 맞은 Tick과 그 Tick에 받은 피해의 합입니다. 야영지에서는 피해를 적용하지 않으므로 화면에 보여 주려고만 적습니다.
    struck: BTreeMap<MonsterId, Struck>,
}

/// 허수아비가 마지막으로 맞은 때입니다. 이벤트가 아니라 상태이므로, 스냅샷을 하나 놓친 클라이언트도 같은 숫자를 그립니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Struck {
    pub tick: u64,
    pub amount: u32,
}

/// 「모두 준비」, 「모두 확인」, 원정의 실패 판정에 필요한 플레이어입니다. 자리에 있는 플레이어입니다.
/// 자리를 비운 사람은 방에 남아 있지만 나머지를 막지 않습니다.
pub(crate) fn who_must_agree(world: &World) -> impl Iterator<Item = &Player> {
    world.players.values().filter(|player| player.is_present())
}

impl Camp {
    pub(crate) fn new() -> Camp {
        Camp {
            ready: BTreeSet::new(),
            countdown: None,
            struck: BTreeMap::new(),
        }
    }

    pub fn is_ready(&self, id: PlayerId) -> bool {
        self.ready.contains(&id)
    }

    /// 준비한 플레이어를 번호 순서로 돌려줍니다.
    pub fn ready(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.ready.iter().copied()
    }

    /// 원정이 시작되기까지 남은 Tick입니다. 모두가 준비한 동안만 있습니다.
    pub fn countdown(&self) -> Option<u32> {
        self.countdown
    }

    /// 허수아비마다 마지막으로 맞은 때입니다. 번호가 작은 허수아비부터입니다.
    pub fn struck(&self) -> impl Iterator<Item = (MonsterId, Struck)> + '_ {
        self.struck.iter().map(|(id, struck)| (*id, *struck))
    }

    /// Tick의 맨 앞에서 준비 키를 반영하고 카운트다운을 진행합니다. 원정을 시작할 때가 되었으면 참입니다.
    ///
    /// 준비 키는 누를 때마다 준비를 켜고 끕니다. 자리에 있는 모두가 준비했으면 카운트다운이 시작되거나 이어지고, 그렇지 않으면 사라집니다.
    /// 그래서 누가 준비를 끄거나 준비하지 않은 사람이 들어오거나 돌아오면 세던 시간이 처음으로 돌아가고, 준비한 사람이 나가도 이어집니다.
    /// 자리에 있는 사람이 없으면 시작하지 않습니다.
    pub(crate) fn advance(&mut self, world: &World, inputs: &PlayerInputs, t: &Tuning) -> bool {
        for (id, input) in inputs {
            if input.toggle_ready && world.players.contains_key(id) && !self.ready.remove(id) {
                self.ready.insert(*id);
            }
        }
        self.countdown = match (self.everyone_ready(world), self.countdown) {
            (false, _) => None,
            (true, None) => Some(t.ticks(t.camp.countdown_secs)),
            (true, Some(left)) => Some(left.saturating_sub(1)),
        };
        self.countdown == Some(0)
    }

    /// 방의 사람이 바뀐 직후(들어옴, 나감, 자리를 비움, 돌아옴)에 부릅니다. 떠난 플레이어의 준비를 지우고,
    /// 이제 자리에 있는 모두가 준비한 상태가 아니면 카운트다운을 취소합니다. 준비한 사람이 나가거나 자리를 비우는 것은 취소하지 않고,
    /// 준비하지 않은 사람이 들어오거나 돌아오는 것은 취소합니다.
    pub(crate) fn settle(&mut self, world: &World) {
        self.ready.retain(|id| world.players.contains_key(id));
        if !self.everyone_ready(world) {
            self.countdown = None;
        }
    }

    fn everyone_ready(&self, world: &World) -> bool {
        let mut needed = who_must_agree(world).peekable();
        needed.peek().is_some() && needed.all(|player| self.ready.contains(&player.id))
    }

    /// 이번 Tick에 허수아비가 맞은 피해를 허수아비마다 더해서 적습니다. 피해는 적용하지 않습니다.
    pub(crate) fn note_strikes(&mut self, hits: &[Hit], tick: u64) {
        for hit in hits {
            let Target::Monster(id) = hit.target else {
                continue;
            };
            let entry = self.struck.entry(id).or_insert(Struck { tick, amount: 0 });
            if entry.tick != tick {
                *entry = Struck { tick, amount: 0 };
            }
            entry.amount = entry.amount.saturating_add(hit.amount);
        }
    }
}
