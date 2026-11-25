//! 규칙의 무작위 수가 나오는 유일한 곳입니다. `Rng`는 `loot` 모듈 밖에서 만들 수 없으므로, 무작위를 쓰는 규칙은 `loot`에만 있습니다.
//!
//! 씨앗(`Seed`)은 `derive`로 단계마다 이어 갑니다. 서버의 방, 원정, 날, 용도의 순서입니다.
//! 용도마다 따로 이어 간 씨앗으로 `Rng`를 만들므로, 한 용도에서 몇 번 뽑든 다른 용도의 결과가 바뀌지 않습니다.
//! 그래서 무기 표에 무기를 더해서 상자의 내용을 뽑는 횟수나 순서가 달라져도, 상자가 놓이는 자리는 같은 씨앗에서 그대로입니다.

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

/// SplitMix64입니다. 외부 크레이트 없이 몇 줄로 되고, 같은 씨앗에서 어느 플랫폼에서나 같은 수열이 나옵니다(정수 연산뿐입니다).
pub(super) struct Rng {
    state: u64,
}

impl Rng {
    pub(super) fn new(seed: Seed) -> Rng {
        Rng { state: seed.0 }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        mix(self.state)
    }

    /// 0 이상 `bound` 미만의 수입니다. 어느 수도 같은 확률로 나오도록, 나머지가 치우치는 구간의 값은 버리고 다시 뽑습니다.
    /// `bound`가 0이면 0입니다.
    pub(super) fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        let bound = u64::from(bound);
        let usable = u64::MAX - u64::MAX % bound;
        loop {
            let value = self.next_u64();
            if value < usable {
                return (value % bound) as u32;
            }
        }
    }

    /// `0..from`에서 서로 다른 수 `count`개를 뽑습니다. `count`가 `from`보다 크면 `from`개입니다.
    /// 앞에서부터 하나씩 남은 수와 자리를 바꾸는 방식이므로, `count`가 커져도 앞의 수들은 같습니다.
    pub(super) fn pick_distinct(&mut self, count: usize, from: usize) -> Vec<usize> {
        let mut pool: Vec<usize> = (0..from).collect();
        let count = count.min(from);
        for slot in 0..count {
            let other = slot + self.below((from - slot) as u32) as usize;
            pool.swap(slot, other);
        }
        pool.truncate(count);
        pool
    }

    /// 무게에 비례하는 확률로 순서 하나를 뽑습니다. 무게가 0인 순서는 나오지 않고, 무게의 합이 0이면 `None`입니다.
    /// 무게의 합은 `u32`에 들어가야 합니다(`Tuning::validate`가 무게마다 상한을 둡니다).
    pub(super) fn pick_weighted(&mut self, weights: &[u32]) -> Option<usize> {
        let total: u32 = weights.iter().sum();
        if total == 0 {
            return None;
        }
        let mut roll = self.below(total);
        for (index, weight) in weights.iter().enumerate() {
            if roll < *weight {
                return Some(index);
            }
            roll -= weight;
        }
        None
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

    #[test]
    fn the_same_seed_gives_the_same_numbers_and_another_seed_gives_other_numbers() {
        let draw = |seed: u64| -> Vec<u32> {
            let mut rng = Rng::new(Seed(seed));
            (0..8).map(|_| rng.below(1000)).collect()
        };

        assert_eq!(draw(5), draw(5));
        assert_ne!(draw(5), draw(6));
    }

    #[test]
    fn below_stays_under_its_bound_and_reaches_every_number() {
        let mut rng = Rng::new(Seed(3));
        let mut seen = [false; 6];
        for _ in 0..200 {
            let value = rng.below(6);
            assert!(value < 6);
            seen[value as usize] = true;
        }
        assert!(seen.iter().all(|reached| *reached), "{seen:?}");
        assert_eq!(rng.below(0), 0, "an empty range gives 0");
        assert_eq!(rng.below(1), 0);
    }

    #[test]
    fn pick_distinct_gives_different_numbers_inside_the_range() {
        for seed in 0..50 {
            let mut rng = Rng::new(Seed(seed));
            let mut picked = rng.pick_distinct(3, 8);
            assert_eq!(picked.len(), 3);
            assert!(picked.iter().all(|index| *index < 8));
            picked.sort();
            picked.dedup();
            assert_eq!(picked.len(), 3, "seed {seed}: the same number twice");
        }
    }

    #[test]
    fn pick_distinct_gives_everything_when_asked_for_more_than_there_is() {
        let mut picked = Rng::new(Seed(1)).pick_distinct(5, 3);
        picked.sort();
        assert_eq!(picked, [0, 1, 2]);
        assert!(Rng::new(Seed(1)).pick_distinct(0, 3).is_empty());
        assert!(Rng::new(Seed(1)).pick_distinct(2, 0).is_empty());
    }

    #[test]
    fn pick_distinct_keeps_the_first_numbers_when_asked_for_more() {
        let two = Rng::new(Seed(9)).pick_distinct(2, 8);
        let three = Rng::new(Seed(9)).pick_distinct(3, 8);
        assert_eq!(two, three[..2]);
    }

    #[test]
    fn pick_weighted_never_picks_a_zero_weight_and_follows_the_weights() {
        let mut rng = Rng::new(Seed(11));
        let mut counts = [0u32; 4];
        for _ in 0..4000 {
            let index = rng
                .pick_weighted(&[0, 1, 3, 0])
                .expect("a weight is above 0");
            counts[index] += 1;
        }
        assert_eq!((counts[0], counts[3]), (0, 0));
        assert!(
            counts[2] > 2 * counts[1],
            "a weight of 3 comes out about three times as often as a weight of 1: {counts:?}"
        );
        assert!(counts[1] > 0);
    }

    #[test]
    fn pick_weighted_without_any_weight_picks_nothing() {
        assert_eq!(Rng::new(Seed(1)).pick_weighted(&[0, 0]), None);
        assert_eq!(Rng::new(Seed(1)).pick_weighted(&[]), None);
    }
}
