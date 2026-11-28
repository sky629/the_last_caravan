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
    /// `from`에서 `to`까지 이은 선분의 양옆으로 `half_width`만큼 넓힌 띠입니다. 두 끝은 선분에 수직으로 잘려 있습니다.
    Lane {
        from: Vec2,
        to: Vec2,
        half_width: f32,
    },
}

impl Zone {
    /// 영역의 중심입니다. 원은 중심이고 띠는 두 끝의 가운데입니다.
    pub fn center(&self) -> Vec2 {
        match *self {
            Zone::Circle { center, .. } => center,
            Zone::Lane { from, to, .. } => (from + to) * 0.5,
        }
    }

    /// 점 `p`가 영역 안에 있는지입니다. 경계는 안쪽입니다.
    pub fn contains(&self, p: Vec2) -> bool {
        match *self {
            Zone::Circle { center, radius } => center.distance(p) <= radius,
            Zone::Lane {
                from,
                to,
                half_width,
            } => {
                let Some((along_axis, length)) = lane_axis(from, to) else {
                    return from.distance(p) <= half_width;
                };
                let offset = p - from;
                let along = offset.dot(along_axis);
                let across = offset.dot(Vec2::new(-along_axis.y, along_axis.x));
                (0.0..=length).contains(&along) && across.abs() <= half_width
            }
        }
    }

    /// 축에 정렬된 사각형이 영역과 조금이라도 겹치는지입니다. 포탑의 타일과 마차가 이 질문을 받습니다.
    pub fn overlaps(&self, rect: Rect) -> bool {
        match *self {
            Zone::Circle { center, radius } => rect.distance_to(center) <= radius,
            Zone::Lane {
                from,
                to,
                half_width,
            } => {
                let Some((along_axis, _)) = lane_axis(from, to) else {
                    return rect.distance_to(from) <= half_width;
                };
                let across_axis = Vec2::new(-along_axis.y, along_axis.x);
                let lane = [
                    from + across_axis * half_width,
                    from - across_axis * half_width,
                    to + across_axis * half_width,
                    to - across_axis * half_width,
                ];
                let corners = [
                    rect.min,
                    Vec2::new(rect.max.x, rect.min.y),
                    rect.max,
                    Vec2::new(rect.min.x, rect.max.y),
                ];
                // 두 볼록 사각형이 겹치지 않으면 둘을 가르는 축이 한 변의 수직 방향 가운데 있습니다. 네 축을 모두 보면 됩니다.
                [
                    along_axis,
                    across_axis,
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0),
                ]
                .into_iter()
                .all(|axis| {
                    let (lane_low, lane_high) = extent(&lane, axis);
                    let (rect_low, rect_high) = extent(&corners, axis);
                    lane_low <= rect_high && rect_low <= lane_high
                })
            }
        }
    }
}

/// 띠의 중심선 방향(길이 1)과 길이입니다. 두 끝이 같아서 방향이 없으면 `None`입니다.
fn lane_axis(from: Vec2, to: Vec2) -> Option<(Vec2, f32)> {
    let axis = to - from;
    let length = axis.length();
    (length > 0.0).then(|| (axis * (1.0 / length), length))
}

/// 꼭짓점들을 `axis` 위에 내려놓았을 때 값의 범위입니다.
fn extent(points: &[Vec2; 4], axis: Vec2) -> (f32, f32) {
    points
        .iter()
        .map(|point| point.dot(axis))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), value| {
            (low.min(value), high.max(value))
        })
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
    /// 예고가 1 Tick이면 보이는 Tick이 하나뿐이므로 처음부터 1.0입니다.
    pub fn progress(&self) -> f32 {
        Telegraph::progress_at(self.ticks_left, self.total_ticks)
    }

    /// `total_ticks` Tick이 걸리는 일이 `ticks_left` Tick 남았을 때의 진행도입니다(0.0 이상 1.0 이하).
    /// 영역이 없는 시간(보스의 포효)도 같은 식으로 진행도를 구합니다.
    pub fn progress_at(ticks_left: u32, total_ticks: u32) -> f32 {
        let steps = total_ticks.saturating_sub(1);
        if steps == 0 {
            return 1.0;
        }
        (total_ticks.saturating_sub(ticks_left) as f32 / steps as f32).min(1.0)
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
    strike_fresh(world, zone, damage, &mut Vec::new(), hits);
}

/// `strike`와 같지만 `struck`에 이미 있는 대상은 건너뛰고, 이번에 맞힌 대상을 `struck`의 끝에 더합니다.
/// 여러 Tick에 걸쳐 같은 대상 곁을 지나가는 공격(돌진)이 그 대상을 두 번 맞히지 않게 하는 기록입니다.
pub(crate) fn strike_fresh(
    world: &World,
    zone: &Zone,
    damage: &StrikeTuning,
    struck: &mut Vec<Target>,
    hits: &mut Vec<Hit>,
) {
    let mut hit = |target: Target, amount: u32| {
        if struck.contains(&target) {
            return;
        }
        struck.push(target);
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

    fn lane() -> Zone {
        Zone::Lane {
            from: Vec2::new(2.0, 5.0),
            to: Vec2::new(12.0, 5.0),
            half_width: 1.0,
        }
    }

    #[test]
    fn a_lane_holds_what_is_within_its_half_width_of_the_line_and_between_its_flat_ends() {
        let lane = lane();
        assert!(lane.contains(Vec2::new(2.0, 5.0)), "the start of the line");
        assert!(lane.contains(Vec2::new(12.0, 5.0)), "the end of the line");
        assert!(lane.contains(Vec2::new(7.0, 6.0)), "the side edge");
        assert!(lane.contains(Vec2::new(7.0, 4.0)), "the other side edge");
        assert!(
            !lane.contains(Vec2::new(7.0, 6.01)),
            "a hair beyond the side"
        );
        assert!(!lane.contains(Vec2::new(1.99, 5.0)), "behind the start");
        assert!(!lane.contains(Vec2::new(12.01, 5.0)), "past the end");
        assert!(
            !lane.contains(Vec2::new(12.5, 5.9)),
            "the ends are flat, not rounded"
        );
    }

    #[test]
    fn a_lane_turns_with_its_line() {
        let diagonal = Zone::Lane {
            from: Vec2::new(0.0, 0.0),
            to: Vec2::new(6.0, 8.0),
            half_width: 1.0,
        };
        assert!(diagonal.contains(Vec2::new(3.0, 4.0)));
        assert!(
            diagonal.contains(Vec2::new(3.0 + 0.8, 4.0 - 0.6)),
            "one tile across"
        );
        assert!(!diagonal.contains(Vec2::new(3.0 + 0.9, 4.0 - 0.7)));
        assert!(!diagonal.contains(Vec2::new(3.0, 0.0)));
    }

    #[test]
    fn a_lane_overlaps_a_rect_that_touches_its_side_or_its_end_and_not_one_beyond() {
        let lane = lane();
        assert!(
            lane.overlaps(Rect::new(6.0, 6.0, 7.0, 7.0)),
            "touching the side"
        );
        assert!(
            !lane.overlaps(Rect::new(6.0, 6.01, 7.0, 7.0)),
            "a hair beyond"
        );
        assert!(
            lane.overlaps(Rect::new(12.0, 4.0, 13.0, 5.0)),
            "touching the end"
        );
        assert!(
            !lane.overlaps(Rect::new(12.01, 4.0, 13.0, 5.0)),
            "past the end"
        );
        assert!(lane.overlaps(Rect::new(6.0, 4.0, 7.0, 5.0)), "inside it");
        assert!(
            lane.overlaps(Rect::new(0.0, 0.0, 20.0, 20.0)),
            "a rect that holds the whole lane"
        );
    }

    #[test]
    fn a_tile_beside_the_corner_of_a_diagonal_lane_is_outside_it() {
        let diagonal = Zone::Lane {
            from: Vec2::new(0.0, 0.0),
            to: Vec2::new(10.0, 10.0),
            half_width: 1.0,
        };
        // 띠의 바깥 상자 안에 들지만 띠에는 닿지 않는 타일입니다. 중심선에서 가장 가까운 모서리 (7, 4)까지 수직으로 약 2.12타일이므로 띠의 반폭 1타일을 넘습니다.
        assert!(!diagonal.overlaps(Rect::new(7.0, 3.0, 8.0, 4.0)));
        assert!(!diagonal.overlaps(Rect::new(7.0, 4.0, 8.0, 5.0)));
        assert!(
            diagonal.overlaps(Rect::new(6.0, 4.0, 7.0, 5.0)),
            "the corner (6, 5) is 0.71 tiles off the line"
        );
        assert!(
            diagonal.overlaps(Rect::new(5.0, 5.0, 6.0, 6.0)),
            "on the line"
        );
    }

    #[test]
    fn a_lane_of_no_length_is_a_circle_of_its_half_width() {
        let stub = Zone::Lane {
            from: Vec2::new(4.0, 4.0),
            to: Vec2::new(4.0, 4.0),
            half_width: 1.0,
        };
        assert!(stub.contains(Vec2::new(4.0, 5.0)));
        assert!(!stub.contains(Vec2::new(4.0, 5.01)));
        assert!(stub.overlaps(Rect::new(5.0, 3.0, 6.0, 4.0)));
        assert!(!stub.overlaps(Rect::new(5.01, 3.0, 6.0, 4.0)));
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
    fn a_countdown_without_a_zone_reports_its_progress_the_same_way() {
        assert_eq!(
            Telegraph::progress_at(4, 4),
            0.0,
            "the first tick it is shown"
        );
        assert_eq!(
            Telegraph::progress_at(1, 4),
            1.0,
            "the last tick it is shown"
        );
        assert_eq!(
            Telegraph::progress_at(1, 1),
            1.0,
            "one tick is full from the start"
        );
    }

    #[test]
    fn a_telegraph_of_no_time_still_lasts_one_tick() {
        let mut telegraph = Telegraph::new(circle(), 0);
        assert_eq!(telegraph.ticks_left, 1);
        assert!(telegraph.tick());
    }
}
