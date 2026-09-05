use rand::{rngs::StdRng, Rng, SeedableRng};

/// Default reservoir size for sampling
pub const DEFAULT_RESERVOIR_SIZE: usize = 10_000;

/// Reservoir sampler using Algorithm R
#[derive(Debug, Clone)]
pub struct ReservoirSampler<T> {
    reservoir: Vec<T>,
    capacity: usize,
    count: usize,
    rng: StdRng,
}

impl<T: Clone> Default for ReservoirSampler<T> {
    fn default() -> Self {
        Self::new(DEFAULT_RESERVOIR_SIZE)
    }
}

impl<T: Clone> ReservoirSampler<T> {
    /// Creates a new reservoir sampler with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            reservoir: Vec::with_capacity(capacity),
            capacity,
            count: 0,
            rng: StdRng::seed_from_u64(0),
        }
    }

    /// Adds a value to the sampler
    pub fn add(&mut self, value: T) {
        self.count += 1;

        if self.reservoir.len() < self.capacity {
            // Reservoir not full, just add
            self.reservoir.push(value);
        } else {
            // Reservoir full, randomly replace
            let j = self.rng.gen_range(0..self.count);
            if j < self.capacity {
                self.reservoir[j] = value;
            }
        }
    }

    /// Returns the sampled values
    pub fn samples(&self) -> &[T] {
        &self.reservoir
    }

    /// Returns the total count of values seen
    #[cfg(test)]
    pub fn total_count(&self) -> usize {
        self.count
    }

    /// Returns whether the reservoir is full
    #[cfg(test)]
    pub fn is_full(&self) -> bool {
        self.reservoir.len() >= self.capacity
    }
}

/// Reservoir sampler specifically for numeric values with percentile calculation
#[derive(Debug, Clone)]
pub struct NumericSampler {
    sampler: ReservoirSampler<f64>,
    sorted_cache: Option<Vec<f64>>,
}

impl Default for NumericSampler {
    fn default() -> Self {
        Self::new(DEFAULT_RESERVOIR_SIZE)
    }
}

impl NumericSampler {
    pub fn new(capacity: usize) -> Self {
        Self {
            sampler: ReservoirSampler::new(capacity),
            sorted_cache: None,
        }
    }

    /// Adds a numeric value
    pub fn add(&mut self, value: f64) {
        self.sampler.add(value);
        self.sorted_cache = None; // Invalidate cache
    }

    /// Returns the total count
    #[cfg(test)]
    pub fn total_count(&self) -> usize {
        self.sampler.total_count()
    }

    /// Returns the number of samples
    pub fn sample_count(&self) -> usize {
        self.sampler.samples().len()
    }

    /// Computes a percentile (0-100)
    pub fn percentile(&mut self, p: f64) -> Option<f64> {
        if self.sampler.samples().is_empty() {
            return None;
        }

        // Ensure sorted cache is populated
        if self.sorted_cache.is_none() {
            let mut sorted = self.sampler.samples().to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            self.sorted_cache = Some(sorted);
        }

        let sorted = self.sorted_cache.as_ref().unwrap();
        let n = sorted.len();

        if n == 1 {
            return Some(sorted[0]);
        }

        // Linear interpolation
        let p = p.clamp(0.0, 100.0) / 100.0;
        let idx = p * (n - 1) as f64;
        let lower = idx.floor() as usize;
        let upper = idx.ceil() as usize;
        let frac = idx - lower as f64;

        if lower == upper || upper >= n {
            Some(sorted[lower.min(n - 1)])
        } else {
            Some(sorted[lower] * (1.0 - frac) + sorted[upper] * frac)
        }
    }

    /// Returns the median (p50)
    #[cfg(test)]
    pub fn median(&mut self) -> Option<f64> {
        self.percentile(50.0)
    }

    /// Returns the interquartile range (p75 - p25)
    pub fn iqr(&mut self) -> Option<f64> {
        let p25 = self.percentile(25.0)?;
        let p75 = self.percentile(75.0)?;
        Some(p75 - p25)
    }

    /// Returns the samples
    pub fn samples(&self) -> &[f64] {
        self.sampler.samples()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reservoir_basic() {
        let mut sampler = ReservoirSampler::<i32>::new(5);

        for i in 0..5 {
            sampler.add(i);
        }

        assert_eq!(sampler.samples().len(), 5);
        assert_eq!(sampler.total_count(), 5);
        assert!(sampler.is_full());
    }

    #[test]
    fn test_reservoir_overflow() {
        let mut sampler = ReservoirSampler::<i32>::new(10);

        for i in 0..100 {
            sampler.add(i);
        }

        assert_eq!(sampler.samples().len(), 10);
        assert_eq!(sampler.total_count(), 100);
    }

    #[test]
    fn test_reservoir_maintains_sample() {
        let mut sampler = ReservoirSampler::<i32>::new(100);

        for i in 0..100 {
            sampler.add(i);
        }

        // All values should be in the sample
        let samples = sampler.samples();
        assert_eq!(samples.len(), 100);

        // Check all original values are present
        let mut sorted: Vec<_> = samples.to_vec();
        sorted.sort();
        assert_eq!(sorted, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn test_numeric_sampler_percentiles() {
        let mut sampler = NumericSampler::new(1000);

        // Add values 1-100
        for i in 1..=100 {
            sampler.add(i as f64);
        }

        // Check percentiles
        let p0 = sampler.percentile(0.0).unwrap();
        assert!((p0 - 1.0).abs() < 0.001);

        let p50 = sampler.percentile(50.0).unwrap();
        assert!((p50 - 50.5).abs() < 1.0);

        let p100 = sampler.percentile(100.0).unwrap();
        assert!((p100 - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_numeric_sampler_median() {
        let mut sampler = NumericSampler::new(100);

        sampler.add(1.0);
        sampler.add(2.0);
        sampler.add(3.0);
        sampler.add(4.0);
        sampler.add(5.0);

        let median = sampler.median().unwrap();
        assert!((median - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_numeric_sampler_iqr() {
        let mut sampler = NumericSampler::new(100);

        // Add values 0-100
        for i in 0..=100 {
            sampler.add(i as f64);
        }

        let iqr = sampler.iqr().unwrap();
        // IQR should be approximately 50 (75 - 25)
        assert!((iqr - 50.0).abs() < 2.0);
    }

    #[test]
    fn test_numeric_sampler_empty() {
        let mut sampler = NumericSampler::new(100);

        assert!(sampler.percentile(50.0).is_none());
        assert!(sampler.median().is_none());
        assert!(sampler.iqr().is_none());
    }

    #[test]
    fn test_numeric_sampler_single_value() {
        let mut sampler = NumericSampler::new(100);

        sampler.add(42.0);

        assert_eq!(sampler.percentile(0.0).unwrap(), 42.0);
        assert_eq!(sampler.percentile(50.0).unwrap(), 42.0);
        assert_eq!(sampler.percentile(100.0).unwrap(), 42.0);
    }

    #[test]
    fn test_numeric_sampler_with_sampling() {
        let mut sampler = NumericSampler::new(100);

        // Add many values (will trigger sampling)
        for i in 0..10_000 {
            sampler.add(i as f64);
        }

        assert_eq!(sampler.sample_count(), 100);
        assert_eq!(sampler.total_count(), 10_000);

        // Median should still be approximately in the middle
        let median = sampler.median().unwrap();
        // With sampling, this might not be exactly 5000
        assert!(median > 1000.0 && median < 9000.0);
    }
}
