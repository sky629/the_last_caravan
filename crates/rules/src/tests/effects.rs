//! 몬스터에게 걸리는 효과(감속, 출혈, 스턴)를 확인합니다.

use crate::kind::WeaponKind;
use crate::tuning::{Debuff, Tuning};

#[test]
fn every_weapon_carries_the_effect_of_its_row() {
    let t = Tuning::default();
    let bleed = |damage| Some(Debuff::Bleed { damage, secs: 3.0 });
    let stun = Some(Debuff::Stun { secs: 0.8 });
    let slow = Some(Debuff::Slow {
        factor: 0.5,
        secs: 2.0,
    });
    for (kind, on_hit) in [
        (WeaponKind::Sword, bleed(5)),
        (WeaponKind::Spear, bleed(6)),
        (WeaponKind::Greatsword, stun),
        (WeaponKind::Rifle, stun),
        (WeaponKind::Bow, slow),
        (WeaponKind::Crossbow, slow),
    ] {
        assert_eq!(t.weapons.get(kind).on_hit, on_hit, "{kind:?}");
    }
}

#[test]
fn the_watchtower_does_not_apply_an_effect() {
    let t = Tuning::default();
    assert_eq!(t.structures.watchtower.on_hit, None);
}
