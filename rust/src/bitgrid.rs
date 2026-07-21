use crate::CELLS;

pub const WORDS: usize = CELLS.div_ceil(64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(align(32))]
pub struct BitGrid {
    words: [u64; WORDS],
}

impl Default for BitGrid {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl BitGrid {
    pub const EMPTY: Self = Self { words: [0; WORDS] };

    #[inline(always)]
    pub fn contains(&self, cell: usize) -> bool {
        debug_assert!(cell < CELLS);
        self.words[cell >> 6] & (1_u64 << (cell & 63)) != 0
    }

    #[inline(always)]
    pub fn insert(&mut self, cell: usize) {
        debug_assert!(cell < CELLS);
        self.words[cell >> 6] |= 1_u64 << (cell & 63);
    }

    #[inline(always)]
    pub fn remove(&mut self, cell: usize) {
        debug_assert!(cell < CELLS);
        self.words[cell >> 6] &= !(1_u64 << (cell & 63));
    }

    #[inline]
    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    #[inline]
    pub fn count(self) -> u32 {
        self.words.iter().map(|word| word.count_ones()).sum()
    }

    #[inline]
    pub fn intersects(self, other: Self) -> bool {
        self.words
            .iter()
            .zip(other.words)
            .any(|(left, right)| left & right != 0)
    }

    #[inline]
    pub fn union_assign(&mut self, other: Self) {
        for (left, right) in self.words.iter_mut().zip(other.words) {
            *left |= right;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BitGrid;
    use crate::CELLS;

    #[test]
    fn packed_cells_round_trip() {
        let mut grid = BitGrid::EMPTY;
        for cell in [0, 1, 63, 64, 127, CELLS - 1] {
            grid.insert(cell);
            assert!(grid.contains(cell));
        }
        assert_eq!(grid.count(), 6);
        grid.remove(64);
        assert!(!grid.contains(64));
        assert_eq!(grid.count(), 5);
    }
}
