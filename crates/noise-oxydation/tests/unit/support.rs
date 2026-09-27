//! Helpers shared by the unit tests.

use crate::convert::narrow;

/// Deterministic xorshift64* generator for reproducible test signals.
pub(crate) struct TestRng(u64);

impl TestRng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Uniform value in `[-1, 1)`.
    pub(crate) fn next_signed(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        // The top 24 bits of the scrambled state, scaled from [0, 2^24) to [-1, 1).
        let bits = u32::try_from(self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40).expect("24 bits");
        f64::from(bits) / f64::from(1_u32 << 23) - 1.0
    }

    pub(crate) fn next_signed_f32(&mut self) -> f32 {
        narrow(self.next_signed())
    }
}
