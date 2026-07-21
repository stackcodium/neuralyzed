const MODULUS: u64 = 2_147_483_647;
const MULTIPLIER: u64 = 16_807;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rng {
    seed: u32,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let normalized = seed % MODULUS;
        Self {
            seed: normalized.max(1) as u32,
        }
    }

    #[inline(always)]
    pub const fn state(self) -> u32 {
        self.seed
    }

    #[inline(always)]
    pub fn set_state(&mut self, state: u32) {
        debug_assert!(state > 0 && u64::from(state) < MODULUS);
        self.seed = state;
    }

    #[inline(always)]
    pub fn next_u31(&mut self) -> u32 {
        self.seed = ((self.seed as u64 * MULTIPLIER) % MODULUS) as u32;
        self.seed
    }

    #[inline(always)]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u31() - 1) as f64 / 2_147_483_646.0
    }

    #[inline(always)]
    pub fn int_inclusive(&mut self, low: i32, high: i32) -> i32 {
        debug_assert!(low <= high);
        low + (self.next_f64() * f64::from(high - low + 1)).floor() as i32
    }

    #[inline]
    pub fn chance(&mut self, probability: f64) -> bool {
        self.next_f64() < probability
    }

    #[inline]
    pub fn dice(&mut self, count: u8, sides: i32) -> i32 {
        (0..count).map(|_| self.int_inclusive(1, sides)).sum()
    }

    #[inline]
    pub fn pick_index(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        self.int_inclusive(0, len as i32 - 1) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn matches_typescript_integer_streams() {
        let cases = [
            (1, [1, 14, 76, 46, 54, 22, 5, 68, 68, 94], 2_007_237_709),
            (
                1_701_033,
                [32, 12, 59, 1, 66, 81, 45, 29, 61, 47],
                989_691_276,
            ),
            (
                1_701_080,
                [32, 31, 11, 57, 70, 10, 66, 20, 54, 40],
                840_583_131,
            ),
        ];
        for (seed, expected, final_state) in cases {
            let mut rng = Rng::new(seed);
            let actual = expected.map(|_| rng.int_inclusive(1, 100));
            assert_eq!(actual, expected, "seed {seed}");
            assert_eq!(rng.state(), final_state, "seed {seed}");
        }
    }
}
