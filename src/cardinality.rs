use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

/// Threshold for switching from exact to approximate cardinality
const EXACT_THRESHOLD: usize = 10_000;

/// HyperLogLog precision (14 bits = 16384 registers)
const HLL_PRECISION: usize = 14;
const HLL_NUM_REGISTERS: usize = 1 << HLL_PRECISION;

/// Cardinality estimator that switches from exact counting to HyperLogLog
#[derive(Debug, Clone)]
pub struct CardinalityEstimator {
    /// Exact set of values (used when cardinality is low)
    exact_set: Option<HashSet<String>>,
    /// HyperLogLog registers (used when cardinality is high)
    hll_registers: Vec<u8>,
    /// Total count of values seen
    count: usize,
    /// Whether we've switched to HLL mode
    using_hll: bool,
}

impl Default for CardinalityEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl CardinalityEstimator {
    pub fn new() -> Self {
        Self {
            exact_set: Some(HashSet::new()),
            hll_registers: vec![0; HLL_NUM_REGISTERS],
            count: 0,
            using_hll: false,
        }
    }

    /// Adds a value to the estimator
    pub fn add(&mut self, value: &str) {
        self.count += 1;

        if self.using_hll {
            self.add_to_hll(value);
        } else {
            let set = self.exact_set.as_mut().unwrap();
            set.insert(value.to_string());

            // Check if we need to switch to HLL
            if set.len() > EXACT_THRESHOLD {
                self.switch_to_hll();
            }
        }
    }

    /// Switches from exact counting to HyperLogLog
    fn switch_to_hll(&mut self) {
        if let Some(set) = self.exact_set.take() {
            for value in set {
                self.add_to_hll(&value);
            }
            self.using_hll = true;
        }
    }

    /// Adds a value to HyperLogLog
    fn add_to_hll(&mut self, value: &str) {
        let hash = self.hash_value(value);

        // Use first HLL_PRECISION bits for register index
        let register_idx = (hash >> (64 - HLL_PRECISION)) as usize;

        // Count leading zeros in remaining bits + 1
        let remaining_bits = (hash << HLL_PRECISION) | (1 << (HLL_PRECISION - 1));
        let leading_zeros = remaining_bits.leading_zeros() as u8 + 1;

        // Update register if this is larger
        if leading_zeros > self.hll_registers[register_idx] {
            self.hll_registers[register_idx] = leading_zeros;
        }
    }

    /// Computes a 64-bit hash of a value
    fn hash_value(&self, value: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// Returns the estimated cardinality
    pub fn estimate(&self) -> usize {
        if !self.using_hll {
            return self.exact_set.as_ref().map_or(0, |s| s.len());
        }

        // HyperLogLog estimation
        let m = HLL_NUM_REGISTERS as f64;
        let alpha = match HLL_PRECISION {
            4 => 0.673,
            5 => 0.697,
            6 => 0.709,
            _ => 0.7213 / (1.0 + 1.079 / m),
        };

        // Calculate harmonic mean of 2^(-register)
        let sum: f64 = self
            .hll_registers
            .iter()
            .map(|&r| 2.0_f64.powi(-(r as i32)))
            .sum();

        let raw_estimate = alpha * m * m / sum;

        // Apply corrections
        let estimate = if raw_estimate <= 2.5 * m {
            // Small range correction
            let zeros = self.hll_registers.iter().filter(|&&r| r == 0).count();
            if zeros > 0 {
                m * (m / zeros as f64).ln()
            } else {
                raw_estimate
            }
        } else if raw_estimate <= (1u64 << 32) as f64 / 30.0 {
            // No correction needed
            raw_estimate
        } else {
            // Large range correction
            -((1u64 << 32) as f64) * (1.0 - raw_estimate / (1u64 << 32) as f64).ln()
        };

        estimate.round() as usize
    }

    /// Returns true if using exact counting
    pub fn is_exact(&self) -> bool {
        !self.using_hll
    }

    /// Returns the total count of values seen
    #[cfg(test)]
    pub fn total_count(&self) -> usize {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_cardinality() {
        let mut estimator = CardinalityEstimator::new();

        estimator.add("a");
        estimator.add("b");
        estimator.add("c");
        estimator.add("a"); // duplicate

        assert!(estimator.is_exact());
        assert_eq!(estimator.estimate(), 3);
        assert_eq!(estimator.total_count(), 4);
    }

    #[test]
    fn test_exact_threshold() {
        let mut estimator = CardinalityEstimator::new();

        // Add exactly EXACT_THRESHOLD unique values
        for i in 0..EXACT_THRESHOLD {
            estimator.add(&i.to_string());
        }

        assert!(estimator.is_exact());
        assert_eq!(estimator.estimate(), EXACT_THRESHOLD);

        // Add one more to trigger switch
        estimator.add("trigger_switch");
        assert!(!estimator.is_exact());
    }

    #[test]
    fn test_hll_estimation() {
        let mut estimator = CardinalityEstimator::new();

        // Add many unique values to trigger HLL
        let num_values = 50_000;
        for i in 0..num_values {
            estimator.add(&format!("value_{}", i));
        }

        assert!(!estimator.is_exact());

        let estimate = estimator.estimate();
        // HLL should be within ~5% accuracy
        let error = (estimate as f64 - num_values as f64).abs() / num_values as f64;
        assert!(error < 0.05, "HLL error too high: {:.2}%", error * 100.0);
    }

    #[test]
    fn test_hll_with_duplicates() {
        let mut estimator = CardinalityEstimator::new();

        // Add unique values to trigger HLL
        for i in 0..15_000 {
            estimator.add(&format!("value_{}", i));
        }

        // Add many duplicates
        for _ in 0..10_000 {
            estimator.add("duplicate");
        }

        assert!(!estimator.is_exact());

        let estimate = estimator.estimate();
        // Should estimate around 15,001 unique values
        let expected = 15_001;
        let error = (estimate as f64 - expected as f64).abs() / expected as f64;
        assert!(error < 0.10, "HLL error too high: {:.2}%", error * 100.0);
    }

    #[test]
    fn test_empty_estimator() {
        let estimator = CardinalityEstimator::new();
        assert_eq!(estimator.estimate(), 0);
        assert_eq!(estimator.total_count(), 0);
        assert!(estimator.is_exact());
    }

    #[test]
    fn test_single_value_repeated() {
        let mut estimator = CardinalityEstimator::new();

        for _ in 0..1000 {
            estimator.add("same");
        }

        assert!(estimator.is_exact());
        assert_eq!(estimator.estimate(), 1);
        assert_eq!(estimator.total_count(), 1000);
    }
}
