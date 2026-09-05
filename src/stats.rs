use crate::cardinality::CardinalityEstimator;
use crate::missing::{MissingDetector, MissingStats};
use crate::sampling::NumericSampler;
use crate::types::{DataType, TypeStats};
use std::collections::HashMap;

/// Top-K capacity for Space-Saving algorithm
const TOP_K_CAPACITY: usize = 1000;

/// Streaming statistics using Welford's algorithm
#[derive(Debug, Clone, Default)]
pub struct StreamingStats {
    count: usize,
    mean: f64,
    m2: f64, // Sum of squared differences
    min: Option<f64>,
    max: Option<f64>,
}

impl StreamingStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates statistics with a new value
    pub fn update(&mut self, value: f64) {
        self.count += 1;

        // Welford's online algorithm
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;

        // Update min/max
        self.min = Some(self.min.map_or(value, |m| m.min(value)));
        self.max = Some(self.max.map_or(value, |m| m.max(value)));
    }

    /// Returns the count of values
    pub fn count(&self) -> usize {
        self.count
    }

    /// Returns the mean
    pub fn mean(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.mean)
        } else {
            None
        }
    }

    /// Returns the sample variance
    pub fn variance(&self) -> Option<f64> {
        if self.count > 1 {
            Some(self.m2 / (self.count - 1) as f64)
        } else {
            None
        }
    }

    /// Returns the sample standard deviation
    pub fn std_dev(&self) -> Option<f64> {
        self.variance().map(|v| v.sqrt())
    }

    /// Returns the minimum value
    pub fn min(&self) -> Option<f64> {
        self.min
    }

    /// Returns the maximum value
    pub fn max(&self) -> Option<f64> {
        self.max
    }
}

/// Space-Saving algorithm for Top-K frequent values
#[derive(Debug, Clone)]
pub struct TopKTracker {
    /// Map from value to (count, error_bound)
    counters: HashMap<String, (usize, usize)>,
    capacity: usize,
    min_count: usize,
}

impl Default for TopKTracker {
    fn default() -> Self {
        Self::new(TOP_K_CAPACITY)
    }
}

impl TopKTracker {
    pub fn new(capacity: usize) -> Self {
        Self {
            counters: HashMap::new(),
            capacity,
            min_count: 0,
        }
    }

    /// Adds a value to the tracker
    pub fn add(&mut self, value: &str) {
        if let Some((count, _)) = self.counters.get_mut(value) {
            *count += 1;
        } else if self.counters.len() < self.capacity {
            self.counters.insert(value.to_string(), (1, 0));
        } else {
            // Find and replace minimum
            let min_entry = self
                .counters
                .iter()
                .min_by(|(a, (a_count, _)), (b, (b_count, _))| {
                    a_count.cmp(b_count).then_with(|| a.cmp(b))
                })
                .map(|(k, (c, _))| (k.clone(), *c));

            if let Some((min_key, min_count)) = min_entry {
                self.counters.remove(&min_key);
                self.counters
                    .insert(value.to_string(), (min_count + 1, min_count));
                self.min_count = min_count;
            }
        }
    }

    /// Returns the top K values by frequency
    pub fn top_k(&self, k: usize) -> Vec<(String, usize)> {
        let mut items: Vec<_> = self
            .counters
            .iter()
            .map(|(k, (c, _))| (k.clone(), *c))
            .collect();

        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        items.truncate(k);
        items
    }

    /// Returns the estimated count for a value
    #[cfg(test)]
    pub fn count(&self, value: &str) -> usize {
        self.counters.get(value).map(|(c, _)| *c).unwrap_or(0)
    }
}

/// Complete statistics for a single column
#[derive(Debug, Clone)]
pub struct ColumnStats {
    pub name: String,
    pub missing: MissingStats,
    pub types: TypeStats,
    pub cardinality: CardinalityEstimator,
    pub numeric_stats: StreamingStats,
    pub non_finite_count: usize,
    pub numeric_sampler: NumericSampler,
    pub top_values: TopKTracker,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
}

impl ColumnStats {
    pub fn new(name: String, sample_size: usize) -> Self {
        Self {
            name,
            missing: MissingStats::new(),
            types: TypeStats::new(),
            cardinality: CardinalityEstimator::new(),
            numeric_stats: StreamingStats::new(),
            non_finite_count: 0,
            numeric_sampler: NumericSampler::new(sample_size),
            top_values: TopKTracker::default(),
            min_length: None,
            max_length: None,
        }
    }

    /// Records a value for this column
    pub fn record(&mut self, value: &str, missing_detector: &MissingDetector) {
        let is_missing = missing_detector.is_missing(value);

        // Update missing stats
        self.missing.record(is_missing);

        // Update type stats
        self.types.record(value, is_missing);

        if !is_missing {
            // Update cardinality
            self.cardinality.add(value);

            // Update top values
            self.top_values.add(value);

            // Update string length stats
            let len = value.len();
            self.min_length = Some(self.min_length.map_or(len, |m| m.min(len)));
            self.max_length = Some(self.max_length.map_or(len, |m| m.max(len)));

            // If numeric, update numeric stats
            if let Ok(num) = value.trim().parse::<f64>() {
                if num.is_finite() {
                    self.numeric_stats.update(num);
                    self.numeric_sampler.add(num);
                } else {
                    self.non_finite_count += 1;
                }
            }
        }
    }

    /// Returns the inferred data type
    pub fn inferred_type(&self) -> DataType {
        self.types.inferred_type()
    }

    /// Returns estimated cardinality
    pub fn cardinality(&self) -> usize {
        self.cardinality.estimate()
    }

    /// Returns whether cardinality is exact or estimated
    pub fn cardinality_is_exact(&self) -> bool {
        self.cardinality.is_exact()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // StreamingStats tests
    #[test]
    fn test_streaming_stats_basic() {
        let mut stats = StreamingStats::new();

        stats.update(1.0);
        stats.update(2.0);
        stats.update(3.0);
        stats.update(4.0);
        stats.update(5.0);

        assert_eq!(stats.count(), 5);
        assert!((stats.mean().unwrap() - 3.0).abs() < 0.001);
        assert_eq!(stats.min(), Some(1.0));
        assert_eq!(stats.max(), Some(5.0));
    }

    #[test]
    fn test_streaming_stats_variance() {
        let mut stats = StreamingStats::new();

        // Values: 2, 4, 4, 4, 5, 5, 7, 9
        for v in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
            stats.update(v);
        }

        assert_eq!(stats.count(), 8);
        assert!((stats.mean().unwrap() - 5.0).abs() < 0.001);

        // Sample variance should be 4.571428...
        let var = stats.variance().unwrap();
        assert!((var - 4.571428).abs() < 0.01);
    }

    #[test]
    fn test_streaming_stats_empty() {
        let stats = StreamingStats::new();

        assert_eq!(stats.count(), 0);
        assert!(stats.mean().is_none());
        assert!(stats.variance().is_none());
        assert!(stats.std_dev().is_none());
        assert!(stats.min().is_none());
        assert!(stats.max().is_none());
    }

    #[test]
    fn test_streaming_stats_single_value() {
        let mut stats = StreamingStats::new();
        stats.update(42.0);

        assert_eq!(stats.count(), 1);
        assert_eq!(stats.mean(), Some(42.0));
        assert!(stats.variance().is_none()); // Need 2+ values
        assert_eq!(stats.min(), Some(42.0));
        assert_eq!(stats.max(), Some(42.0));
    }

    // TopKTracker tests
    #[test]
    fn test_top_k_basic() {
        let mut tracker = TopKTracker::new(10);

        tracker.add("a");
        tracker.add("b");
        tracker.add("a");
        tracker.add("c");
        tracker.add("a");
        tracker.add("b");

        let top = tracker.top_k(3);
        assert_eq!(top[0], ("a".to_string(), 3));
        assert_eq!(top[1], ("b".to_string(), 2));
        assert_eq!(top[2], ("c".to_string(), 1));
    }

    #[test]
    fn test_top_k_overflow() {
        let mut tracker = TopKTracker::new(3);

        // Add 3 values
        tracker.add("a");
        tracker.add("b");
        tracker.add("c");

        // Add a 4th value - should replace the minimum
        tracker.add("d");

        assert_eq!(tracker.counters.len(), 3);
    }

    #[test]
    fn test_top_k_increment_existing() {
        let mut tracker = TopKTracker::new(3);

        tracker.add("a");
        tracker.add("a");
        tracker.add("a");

        assert_eq!(tracker.count("a"), 3);
    }

    // ColumnStats tests
    #[test]
    fn test_column_stats_numeric() {
        let detector = MissingDetector::default();
        let mut stats = ColumnStats::new("test".to_string(), 1000);

        stats.record("10", &detector);
        stats.record("20", &detector);
        stats.record("30", &detector);
        stats.record("NA", &detector);

        assert_eq!(stats.missing.total_count, 4);
        assert_eq!(stats.missing.missing_count, 1);
        assert_eq!(stats.inferred_type(), DataType::Integer);
        assert_eq!(stats.cardinality(), 3);
        assert!((stats.numeric_stats.mean().unwrap() - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_column_stats_string() {
        let detector = MissingDetector::default();
        let mut stats = ColumnStats::new("name".to_string(), 1000);

        stats.record("Alice", &detector);
        stats.record("Bob", &detector);
        stats.record("Charlie", &detector);

        assert_eq!(stats.inferred_type(), DataType::String);
        assert_eq!(stats.cardinality(), 3);
        assert_eq!(stats.min_length, Some(3)); // "Bob"
        assert_eq!(stats.max_length, Some(7)); // "Charlie"
    }

    #[test]
    fn test_column_stats_top_values() {
        let detector = MissingDetector::default();
        let mut stats = ColumnStats::new("status".to_string(), 1000);

        for _ in 0..100 {
            stats.record("active", &detector);
        }
        for _ in 0..50 {
            stats.record("pending", &detector);
        }
        for _ in 0..10 {
            stats.record("inactive", &detector);
        }

        let top = stats.top_values.top_k(3);
        assert_eq!(top[0].0, "active");
        assert_eq!(top[0].1, 100);
        assert_eq!(top[1].0, "pending");
        assert_eq!(top[1].1, 50);
    }

    #[test]
    fn test_column_stats_all_missing() {
        let detector = MissingDetector::default();
        let mut stats = ColumnStats::new("empty".to_string(), 1000);

        stats.record("NA", &detector);
        stats.record("NULL", &detector);
        stats.record("", &detector);

        assert_eq!(stats.missing.missing_count, 3);
        assert_eq!(stats.cardinality(), 0);
        assert_eq!(stats.inferred_type(), DataType::String);
    }

    #[test]
    fn test_welford_numerical_stability() {
        let mut stats = StreamingStats::new();

        // Add large values that could cause numerical instability
        let base = 1_000_000_000.0;
        for i in 0..100 {
            stats.update(base + i as f64);
        }

        // Mean should be approximately base + 49.5
        let expected_mean = base + 49.5;
        assert!((stats.mean().unwrap() - expected_mean).abs() < 0.001);
    }
}
