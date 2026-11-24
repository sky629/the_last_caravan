//! 플레이어가 드는 무기를 확인합니다.
//! 시험하는 것은 `&Sim`이 공개하는 값(`Player::weapon`)뿐입니다.

use super::support::Scenario;
use crate::kind::WeaponKind;

#[test]
fn a_player_starts_with_the_sword() {
    let mut s = Scenario::open_field();
    let a = s.player_at(10.0, 10.0);

    assert_eq!(s.player(a).weapon, WeaponKind::Sword);
}
