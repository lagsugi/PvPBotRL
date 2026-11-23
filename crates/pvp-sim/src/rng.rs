use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Java-compatible 48-bit LCG for combat direction fallback; independent per player.
/// Seeding/irrelevant vanilla RNG consumption is deliberately not yet identical.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JavaRandom {
    state: u64,
    #[serde(skip)]
    measured: Option<VecDeque<f64>>,
    #[serde(skip)]
    missing_measured: bool,
}
impl JavaRandom {
    pub fn new(seed: u64) -> Self {
        Self {
            state: (seed ^ 0x5deece66d) & ((1u64 << 48) - 1),
            measured: None,
            missing_measured: false,
        }
    }
    fn next(&mut self, bits: u32) -> u64 {
        self.state = self.state.wrapping_mul(0x5deece66d).wrapping_add(11) & ((1u64 << 48) - 1);
        self.state >> (48 - bits)
    }
    pub fn next_double(&mut self) -> f64 {
        if let Some(measured) = &mut self.measured {
            if let Some(value) = measured.pop_front() {
                return value;
            }
            self.missing_measured = true;
        }
        ((self.next(26) << 27) + self.next(27)) as f64 / ((1u64 << 53) as f64)
    }

    pub(crate) fn use_measured(&mut self, values: Vec<f64>) {
        self.measured = Some(values.into());
    }

    pub(crate) fn measured_consumed(&self) -> bool {
        !self.missing_measured && self.measured.as_ref().is_none_or(VecDeque::is_empty)
    }
}
/// Stable seed derivation; scheduling and worker counts never enter this calculation.
pub fn derive_seed(seed: u64, index: u64) -> u64 {
    let mut z = seed.wrapping_add(index.wrapping_add(1).wrapping_mul(0x9e3779b97f4a7c15));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
