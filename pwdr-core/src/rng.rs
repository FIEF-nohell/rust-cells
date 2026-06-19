//! Small, fast, deterministic PRNG for the simulation core.
//!
//! No `thread_rng`, no global state. Same seed + same call sequence => same
//! output on every platform. This is a SplitMix64-seeded xoshiro256** which is
//! fast, has a long period, and good statistical quality for sim jitter.

/// Deterministic PRNG. `Clone` so a grid can be snapshotted/restored exactly.
#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Construct from a 64-bit seed. Seeding via SplitMix64 so even adjacent
    /// seeds (0, 1, 2 ...) produce well-decorrelated streams.
    pub fn new(seed: u64) -> Self {
        let mut sm = seed;
        let mut next = || {
            sm = sm.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = sm;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Rng {
            s: [next(), next(), next(), next()],
        }
    }

    /// Exact internal state — for save/load round-trips.
    pub fn state(&self) -> [u64; 4] {
        self.s
    }

    /// Reconstruct from a previously saved [`Rng::state`].
    pub fn from_state(s: [u64; 4]) -> Self {
        Rng { s }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1]
            .wrapping_mul(5)
            .rotate_left(7)
            .wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `[0, n)`. Uses Lemire's multiply-shift; fast, low bias.
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        ((self.next_u32() as u64 * n as u64) >> 32) as u32
    }

    /// Fair coin.
    #[inline]
    pub fn bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Returns true with probability `p` (clamped to [0,1]).
    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        if p <= 0.0 {
            return false;
        }
        if p >= 1.0 {
            return true;
        }
        // 24 bits of mantissa precision is plenty for reaction probabilities.
        (self.next_u32() >> 8) < (p * (1u32 << 24) as f32) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let diffs = (0..100).filter(|_| a.next_u64() != b.next_u64()).count();
        assert!(diffs > 90, "streams should mostly differ, got {diffs}");
    }

    #[test]
    fn below_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            assert!(r.below(10) < 10);
        }
    }

    #[test]
    fn chance_bounds() {
        let mut r = Rng::new(9);
        assert!(!r.chance(0.0));
        assert!(r.chance(1.0));
        let hits = (0..10_000).filter(|_| r.chance(0.5)).count();
        assert!((4000..6000).contains(&hits), "p=0.5 gave {hits}/10000");
    }
}
