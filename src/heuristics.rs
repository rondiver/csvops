use regex::Regex;
use std::sync::LazyLock;

use crate::stats::ColumnStats;
use crate::types::DataType;

// UUID pattern
static UUID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .unwrap()
});

// Common ID column name patterns
static ID_NAME_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(id|_id|.*_id|.*id|uuid|guid|key|.*_key|pk|.*_pk)$").unwrap()
});

/// Checks if a column appears to be an ID field
pub fn is_likely_id_column(stats: &ColumnStats) -> bool {
    // Check 1: Column name pattern
    let name_suggests_id = ID_NAME_PATTERN.is_match(&stats.name);

    // Check 2: High uniqueness ratio (cardinality close to row count)
    let total_non_missing = stats.missing.total_count - stats.missing.missing_count;
    let uniqueness_ratio = if total_non_missing > 0 {
        stats.cardinality() as f64 / total_non_missing as f64
    } else {
        0.0
    };
    let is_highly_unique = uniqueness_ratio > 0.95;

    // Check 3: UUID pattern in values (check via type and pattern)
    // We can check top values for UUID pattern
    let top_values = stats.top_values.top_k(5);
    let has_uuid_pattern = top_values
        .iter()
        .any(|(v, _)| UUID_PATTERN.is_match(v));

    // Check 4: Sequential integers
    let is_sequential_int = stats.inferred_type() == DataType::Integer && is_highly_unique;

    // Combine heuristics
    (name_suggests_id && is_highly_unique)
        || has_uuid_pattern
        || (is_sequential_int && name_suggests_id)
}

/// Checks if a value is an outlier using IQR method
/// Outlier if value > p99 + 10*IQR or value < p1 - 10*IQR
pub fn is_outlier(value: f64, p1: f64, p99: f64, iqr: f64) -> bool {
    let lower_bound = p1 - 10.0 * iqr;
    let upper_bound = p99 + 10.0 * iqr;
    value < lower_bound || value > upper_bound
}

/// Calculates the outlier count for a column
pub fn count_outliers(stats: &mut ColumnStats) -> usize {
    if stats.numeric_sampler.sample_count() == 0 {
        return 0;
    }

    let p1 = match stats.numeric_sampler.percentile(1.0) {
        Some(v) => v,
        None => return 0,
    };
    let p99 = match stats.numeric_sampler.percentile(99.0) {
        Some(v) => v,
        None => return 0,
    };
    let iqr = match stats.numeric_sampler.iqr() {
        Some(v) => v,
        None => return 0,
    };

    if iqr == 0.0 {
        return 0; // All values are the same, no outliers
    }

    stats
        .numeric_sampler
        .samples()
        .iter()
        .filter(|&&v| is_outlier(v, p1, p99, iqr))
        .count()
}

/// Detects potential data quality issues in a column
#[derive(Debug, Clone)]
pub struct ColumnHeuristics {
    pub is_id_column: bool,
    pub has_outliers: bool,
    pub outlier_count: usize,
    pub is_constant: bool,
    pub is_mostly_missing: bool,
    pub has_mixed_types: bool,
    pub has_high_cardinality: bool,
    pub has_low_cardinality: bool,
}

impl ColumnHeuristics {
    pub fn analyze(stats: &mut ColumnStats) -> Self {
        let total = stats.missing.total_count;
        let missing_pct = stats.missing.missing_percentage();

        // Count outliers for numeric columns
        let outlier_count = if matches!(
            stats.inferred_type(),
            DataType::Integer | DataType::Float
        ) {
            count_outliers(stats)
        } else {
            0
        };

        let cardinality = stats.cardinality();
        let non_missing = total.saturating_sub(stats.missing.missing_count);

        Self {
            is_id_column: is_likely_id_column(stats),
            has_outliers: outlier_count > 0,
            outlier_count,
            is_constant: cardinality == 1 && non_missing > 1,
            is_mostly_missing: missing_pct > 50.0,
            has_mixed_types: stats.types.is_mixed_type(),
            has_high_cardinality: non_missing > 0 && cardinality as f64 / non_missing as f64 > 0.9,
            has_low_cardinality: non_missing > 100 && cardinality <= 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::missing::MissingDetector;

    fn create_test_stats(name: &str) -> ColumnStats {
        ColumnStats::new(name.to_string(), 1000)
    }

    #[test]
    fn test_id_column_by_name() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("user_id");

        // Add unique values
        for i in 0..100 {
            stats.record(&i.to_string(), &detector);
        }

        assert!(is_likely_id_column(&stats));
    }

    #[test]
    fn test_id_column_uuid() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("identifier");

        // Add UUID values
        stats.record("550e8400-e29b-41d4-a716-446655440000", &detector);
        stats.record("6ba7b810-9dad-11d1-80b4-00c04fd430c8", &detector);
        stats.record("6ba7b811-9dad-11d1-80b4-00c04fd430c8", &detector);

        assert!(is_likely_id_column(&stats));
    }

    #[test]
    fn test_not_id_column() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("status");

        // Add repeated values
        for _ in 0..50 {
            stats.record("active", &detector);
        }
        for _ in 0..50 {
            stats.record("inactive", &detector);
        }

        assert!(!is_likely_id_column(&stats));
    }

    #[test]
    fn test_is_outlier() {
        let p1 = 0.0;
        let p99 = 100.0;
        let iqr = 10.0; // p75 - p25

        // Normal values
        assert!(!is_outlier(50.0, p1, p99, iqr));
        assert!(!is_outlier(0.0, p1, p99, iqr));
        assert!(!is_outlier(100.0, p1, p99, iqr));

        // Outliers (beyond p99 + 10*IQR = 200 or below p1 - 10*IQR = -100)
        assert!(is_outlier(201.0, p1, p99, iqr));
        assert!(is_outlier(-101.0, p1, p99, iqr));
    }

    #[test]
    fn test_heuristics_constant_column() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("constant");

        for _ in 0..100 {
            stats.record("same_value", &detector);
        }

        let heuristics = ColumnHeuristics::analyze(&mut stats);
        assert!(heuristics.is_constant);
    }

    #[test]
    fn test_heuristics_mostly_missing() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("sparse");

        for _ in 0..10 {
            stats.record("value", &detector);
        }
        for _ in 0..90 {
            stats.record("NA", &detector);
        }

        let heuristics = ColumnHeuristics::analyze(&mut stats);
        assert!(heuristics.is_mostly_missing);
    }

    #[test]
    fn test_heuristics_mixed_types() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("mixed");

        // 80 integers
        for i in 0..80 {
            stats.record(&i.to_string(), &detector);
        }
        // 20 strings (>5% threshold)
        for _ in 0..20 {
            stats.record("text", &detector);
        }

        let heuristics = ColumnHeuristics::analyze(&mut stats);
        assert!(heuristics.has_mixed_types);
    }

    #[test]
    fn test_heuristics_high_cardinality() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("unique");

        for i in 0..100 {
            stats.record(&format!("value_{}", i), &detector);
        }

        let heuristics = ColumnHeuristics::analyze(&mut stats);
        assert!(heuristics.has_high_cardinality);
    }

    #[test]
    fn test_heuristics_low_cardinality() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("category");

        for i in 0..200 {
            stats.record(&format!("cat_{}", i % 5), &detector);
        }

        let heuristics = ColumnHeuristics::analyze(&mut stats);
        assert!(heuristics.has_low_cardinality);
    }

    #[test]
    fn test_outlier_detection_numeric() {
        let detector = MissingDetector::default();
        let mut stats = create_test_stats("values");

        // Add normal values
        for i in 0..100 {
            stats.record(&i.to_string(), &detector);
        }
        // Add outlier
        stats.record("10000", &detector);

        let heuristics = ColumnHeuristics::analyze(&mut stats);
        assert!(heuristics.has_outliers);
        assert!(heuristics.outlier_count > 0);
    }

    #[test]
    fn test_uuid_pattern() {
        assert!(UUID_PATTERN.is_match("550e8400-e29b-41d4-a716-446655440000"));
        assert!(UUID_PATTERN.is_match("6BA7B810-9DAD-11D1-80B4-00C04FD430C8"));
        assert!(!UUID_PATTERN.is_match("not-a-uuid"));
        assert!(!UUID_PATTERN.is_match("550e8400e29b41d4a716446655440000")); // Missing dashes
    }

    #[test]
    fn test_id_name_pattern() {
        assert!(ID_NAME_PATTERN.is_match("id"));
        assert!(ID_NAME_PATTERN.is_match("ID"));
        assert!(ID_NAME_PATTERN.is_match("user_id"));
        assert!(ID_NAME_PATTERN.is_match("userId"));
        assert!(ID_NAME_PATTERN.is_match("uuid"));
        assert!(ID_NAME_PATTERN.is_match("primary_key"));
        assert!(!ID_NAME_PATTERN.is_match("name"));
        assert!(!ID_NAME_PATTERN.is_match("email"));
    }
}
