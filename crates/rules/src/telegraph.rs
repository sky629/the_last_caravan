//! 예고된 공격입니다. 위험한 영역(`Zone`)이 먼저 보이고, 정해진 시간이 지나면 그 영역 안의 대상이 피해를 받습니다.
//!
//! 영역은 예고가 시작될 때 한 번 정하고 그 뒤에는 바꾸지 않습니다. 예고가 보이는 동안 영역 밖으로 나간 대상은 피해를 받지 않으므로,
//! 플레이어는 보고 피할 수 있습니다. 영역과 피해는 보스에 묶여 있지 않아서, 예고된 공격을 하는 몬스터가 늘어나도 이 파일을 그대로 씁니다.
//!
//! 영역이 맞히는 대상의 몸은 대상마다 다릅니다. 플레이어는 중심점이 영역 안에 있으면 맞고, 포탑과 마차는 사각형이 영역과 조금이라도 겹치면 맞습니다.
//! 화면의 영역은 맞는 범위와 같은 모양이므로, 영역 밖으로 나가는 것을 눈으로 보고 판단할 수 있습니다.

use crate::math::{Rect, Vec2};
use crate::tuning::StrikeTuning;
use crate::world::{Hit, Source, Target, World};

/// 위험한 영역의 모양입니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    /// 중심에서 반지름 안입니다.
    Circle { center: Vec2, radius: f32 },
}

impl Zone {
    /// 점 `p`가 영역 안에 있는지입니다. 경계는 안쪽입니다.
    pub fn contains(&self, p: Vec2) -> bool {
        match *self {
            Zone::Circle { center, radius } => center.distance(p) <= radius,
        }
    }

    /// 축에 정렬된 사각형이 영역과 조금이라도 겹치는지입니다. 포탑의 타일과 마차가 이 질문을 받습니다.
    pub fn overlaps(&self, rect: Rect) -> bool {
        match *self {
            Zone::Circle { center, radius } => rect.distance_to(center) <= radius,
        }
    }
}

/// 터지기를 기다리는 영역입니다. `ticks_left`가 0이 되는 Tick에 영역 안의 대상이 피해를 받습니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Telegraph {
    pub zone: Zone,
    /// 터질 때까지 남은 Tick입니다. 존재하는 동안에는 항상 1 이상입니다.
    pub ticks_left: u32,
    /// 예고가 시작될 때의 `ticks_left`입니다. 진행도를 구하는 데만 씁니다.
    pub total_ticks: u32,
}

impl Telegraph {
    pub fn new(zone: Zone, ticks: u32) -> Telegraph {
        let ticks = ticks.max(1);
        Telegraph {
            zone,
            ticks_left: ticks,
            total_ticks: ticks,
        }
    }

    /// 예고가 얼마나 진행되었는지입니다(0.0 이상 1.0 이하). 예고가 시작된 Tick에 0.0이고, 터지기 바로 앞의 Tick에 1.0입니다.
    /// 터지는 Tick에는 예고가 사라지므로, 화면은 가득 찬 영역을 한 번 본 뒤에 피해를 봅니다.
    pub fn progress(&self) -> f32 {
        let steps = self.total_ticks.saturating_sub(1);
        if steps == 0 {
            return 1.0;
        }
        ((self.total_ticks - self.ticks_left) as f32 / steps as f32).min(1.0)
    }

    /// 한 Tick 지나갑니다. 이번 Tick에 터지면 참입니다.
    pub fn tick(&mut self) -> bool {
        self.ticks_left = self.ticks_left.saturating_sub(1);
        self.ticks_left == 0
    }
}

/// 영역 안의 대상이 하나의 `Hit`으로 받는 피해를 `hits`에 더합니다. 대상이 플레이어인지, 포탑인지, 마차인지에 따라 피해가 다릅니다.
///
/// 맞는 플레이어는 살아 있고 자리에 있는 플레이어뿐입니다. 쓰러졌거나 사망한 플레이어와 자리를 비운 플레이어는 맞지 않고,
/// 짓는 중인 포탑도 피해를 받지 않으므로 맞히지 않습니다.
pub(crate) fn strike(world: &World, zone: &Zone, damage: &StrikeTuning, hits: &mut Vec<Hit>) {
    let mut hit = |target: Target, amount: u32| {
        hits.push(Hit {
            target,
            amount,
            by: Source::Monster,
            debuff: None,
        });
    };
    for player in world.players.values() {
        if player.is_targetable() && zone.contains(player.pos) {
            hit(Target::Player(player.id), damage.player);
        }
    }
    for tower in world.towers.values() {
        if tower.is_standing() && zone.overlaps(tower.rect()) {
            hit(Target::Tower(tower.id), damage.tower);
        }
    }
    if zone.overlaps(world.wagon.rect) {
        hit(Target::Wagon, damage.wagon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle() -> Zone {
        Zone::Circle {
            center: Vec2::new(10.0, 10.0),
            radius: 2.0,
        }
    }

    #[test]
    fn a_circle_holds_a_point_on_its_edge_and_not_one_beyond() {
        assert!(circle().contains(Vec2::new(12.0, 10.0)));
        assert!(!circle().contains(Vec2::new(12.01, 10.0)));
        assert!(circle().contains(Vec2::new(10.0, 10.0)));
    }

    #[test]
    fn a_circle_overlaps_a_rect_that_touches_it_at_the_edge_or_holds_the_middle() {
        let circle = circle();
        assert!(circle.overlaps(Rect::new(12.0, 9.0, 13.0, 11.0)), "an edge");
        assert!(
            !circle.overlaps(Rect::new(12.01, 9.0, 13.0, 11.0)),
            "a hair beyond the edge"
        );
        assert!(
            circle.overlaps(Rect::new(9.0, 9.0, 11.0, 11.0)),
            "the middle"
        );
        assert!(
            circle.overlaps(Rect::new(0.0, 0.0, 20.0, 20.0)),
            "a whole map"
        );
    }

    #[test]
    fn a_rect_past_the_corner_of_its_box_is_outside_the_circle() {
        // 원의 바깥 상자(8..12)에 들지만 원에는 닿지 않는 모서리입니다. 모서리까지의 거리는 약 2.12입니다.
        let corner = Rect::new(11.5, 11.5, 12.5, 12.5);
        assert!(!circle().overlaps(corner));
    }

    #[test]
    fn a_telegraph_runs_out_on_the_tick_its_time_is_up_and_reports_how_far_it_got() {
        let mut telegraph = Telegraph::new(circle(), 4);
        assert_eq!(telegraph.progress(), 0.0);

        assert!(!telegraph.tick());
        assert!((telegraph.progress() - 1.0 / 3.0).abs() < 1e-6);
        assert!(!telegraph.tick());
        assert!(!telegraph.tick());
        assert_eq!(
            telegraph.progress(),
            1.0,
            "full on the last tick it is shown"
        );
        assert!(telegraph.tick(), "the fourth tick");
        assert_eq!(telegraph.progress(), 1.0);
    }

    #[test]
    fn a_telegraph_of_no_time_still_lasts_one_tick() {
        let mut telegraph = Telegraph::new(circle(), 0);
        assert_eq!(telegraph.ticks_left, 1);
        assert!(telegraph.tick());
    }
}
