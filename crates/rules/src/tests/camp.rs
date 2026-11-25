//! 야영지를 확인합니다: 허수아비는 야영지에만 놓입니다.

use crate::kind::MonsterKind;
use crate::tuning::Tuning;

#[test]
fn the_scarecrow_is_refused_in_waves_and_as_a_guard() {
    let mut t = Tuning::default();
    t.nights.plans[0].waves[0].kind = MonsterKind::Scarecrow;
    assert!(t.validate().is_err());

    let mut t = Tuning::default();
    t.guards.kind = MonsterKind::Scarecrow;
    assert!(t.validate().is_err());
}
