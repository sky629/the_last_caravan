//! 규칙의 무작위 수가 나오는 곳입니다.
//!
//! 씨앗(`Seed`)은 `derive`로 단계마다 이어 갑니다. 서버의 방, 원정, 날, 용도의 순서입니다.
//! 용도마다 따로 이어 간 씨앗에서 수열을 만들므로, 한 용도에서 몇 번 뽑든 다른 용도의 결과가 바뀌지 않습니다.

/// 방이나 원정이나 날의 씨앗입니다. 같은 씨앗은 언제나 같은 결과를 냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Seed(pub u64);

const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64의 마무리 단계입니다. 서로 다른 입력은 서로 다른 출력이 됩니다.
fn mix(value: u64) -> u64 {
    let a = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let b = (a ^ (a >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    b ^ (b >> 31)
}

impl Seed {
    /// 이 씨앗에서 `key`로 이어 간 씨앗입니다. `key`가 다르면 씨앗도 다릅니다.
    /// 방의 `n`번째 원정은 `derive(n)`, 원정의 `d`번째 날은 `derive(d)`로 정합니다.
    pub fn derive(self, key: u64) -> Seed {
        Seed(mix(self.0 ^ mix(key.wrapping_add(GAMMA))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_gives_the_same_seed_for_the_same_input() {
        assert_eq!(Seed(7).derive(3), Seed(7).derive(3));
    }

    #[test]
    fn derive_gives_another_seed_for_another_key_or_another_parent() {
        let seeds = [
            Seed(7).derive(0),
            Seed(7).derive(1),
            Seed(7).derive(2),
            Seed(8).derive(1),
        ];
        for (i, a) in seeds.iter().enumerate() {
            for b in &seeds[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn derive_does_not_return_its_parent() {
        for key in 0..100 {
            assert_ne!(Seed(7).derive(key), Seed(7));
        }
    }

    #[test]
    fn derive_is_pinned_to_one_value_so_a_changed_formula_is_noticed() {
        // 같은 씨앗은 코드를 고친 뒤에도 같은 결과를 내야 하므로, 공식이 바뀌면 이 값이 달라져서 알려 줍니다.
        assert_eq!(Seed(1).derive(1), Seed(6_652_587_683_579_779_273));
    }
}
