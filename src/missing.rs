use std::collections::HashSet;

/// Default tokens that represent missing values
pub const DEFAULT_MISSING_TOKENS: &[&str] = &[
    "",     // empty string
    "NA",   // R-style
    "N/A",  // common
    "NULL", // SQL-style
    "None", // Python-style
    "-",    // dash placeholder
    "NaN",  // Not a Number
    "nan",  // lowercase nan
    "null", // lowercase null
    "none", // lowercase none
    "na",   // lowercase na
    "n/a",  // lowercase n/a
];

/// Detector for missing values in CSV data
#[derive(Debug, Clone)]
pub struct MissingDetector {
    tokens: HashSet<String>,
    case_insensitive: bool,
}

impl Default for MissingDetector {
    fn default() -> Self {
        Self::new(None)
    }
}

impl MissingDetector {
    /// Creates a new missing value detector
    ///
    /// If custom_tokens is provided, those are used instead of defaults.
    /// Otherwise, the default missing tokens are used.
    pub fn new(custom_tokens: Option<Vec<String>>) -> Self {
        let tokens: HashSet<String> = match custom_tokens {
            Some(tokens) => tokens.into_iter().collect(),
            None => DEFAULT_MISSING_TOKENS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };

        Self {
            tokens,
            case_insensitive: true,
        }
    }

    /// Creates a detector with specific tokens (case-insensitive matching)
    pub fn with_tokens(tokens: Vec<String>) -> Self {
        Self {
            tokens: tokens.into_iter().collect(),
            case_insensitive: true,
        }
    }

    /// Checks if a value is considered missing
    pub fn is_missing(&self, value: &str) -> bool {
        let trimmed = value.trim();

        if self.case_insensitive {
            let lower = trimmed.to_lowercase();
            // Check both the original and lowercase versions
            self.tokens.contains(trimmed) || self.tokens.iter().any(|t| t.to_lowercase() == lower)
        } else {
            self.tokens.contains(trimmed)
        }
    }
}

/// Statistics about missing values in a column
#[derive(Debug, Clone, Default)]
pub struct MissingStats {
    /// Total number of values seen
    pub total_count: usize,
    /// Number of missing values
    pub missing_count: usize,
}

impl MissingStats {
    /// Creates new missing stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a value, updating statistics
    pub fn record(&mut self, is_missing: bool) {
        self.total_count += 1;
        if is_missing {
            self.missing_count += 1;
        }
    }

    /// Returns the percentage of missing values (0.0 to 100.0)
    pub fn missing_percentage(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            (self.missing_count as f64 / self.total_count as f64) * 100.0
        }
    }

    /// Returns true if any missing values were found
    #[cfg(test)]
    pub fn has_missing(&self) -> bool {
        self.missing_count > 0
    }
}

/// Tracks missing value statistics for multiple columns
#[derive(Debug, Clone)]
#[cfg(test)]
pub struct ColumnMissingStats {
    detector: MissingDetector,
    stats: Vec<MissingStats>,
}

#[cfg(test)]
impl ColumnMissingStats {
    /// Creates a new tracker for the specified number of columns
    pub fn new(num_columns: usize, detector: MissingDetector) -> Self {
        Self {
            detector,
            stats: vec![MissingStats::new(); num_columns],
        }
    }

    /// Records values for all columns in a row
    pub fn record_row(&mut self, values: &[&str]) {
        for (i, value) in values.iter().enumerate() {
            if i < self.stats.len() {
                let is_missing = self.detector.is_missing(value);
                self.stats[i].record(is_missing);
            }
        }
    }

    /// Returns statistics for a specific column
    pub fn column_stats(&self, column: usize) -> Option<&MissingStats> {
        self.stats.get(column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_missing_tokens() {
        let detector = MissingDetector::default();

        // Test all default tokens
        assert!(detector.is_missing(""));
        assert!(detector.is_missing("NA"));
        assert!(detector.is_missing("N/A"));
        assert!(detector.is_missing("NULL"));
        assert!(detector.is_missing("None"));
        assert!(detector.is_missing("-"));
        assert!(detector.is_missing("NaN"));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let detector = MissingDetector::default();

        // Test case variations
        assert!(detector.is_missing("na"));
        assert!(detector.is_missing("NA"));
        assert!(detector.is_missing("Na"));
        assert!(detector.is_missing("nA"));

        assert!(detector.is_missing("null"));
        assert!(detector.is_missing("NULL"));
        assert!(detector.is_missing("Null"));

        assert!(detector.is_missing("none"));
        assert!(detector.is_missing("NONE"));
        assert!(detector.is_missing("None"));

        assert!(detector.is_missing("nan"));
        assert!(detector.is_missing("NAN"));
        assert!(detector.is_missing("NaN"));

        assert!(detector.is_missing("n/a"));
        assert!(detector.is_missing("N/A"));
        assert!(detector.is_missing("N/a"));
    }

    #[test]
    fn test_non_missing_values() {
        let detector = MissingDetector::default();

        assert!(!detector.is_missing("hello"));
        assert!(!detector.is_missing("123"));
        assert!(!detector.is_missing("0"));
        assert!(!detector.is_missing("false"));
        assert!(!detector.is_missing("N"));
        assert!(!detector.is_missing("NAH"));
    }

    #[test]
    fn test_whitespace_handling() {
        let detector = MissingDetector::default();

        // Whitespace should be trimmed
        assert!(detector.is_missing("  NA  "));
        assert!(detector.is_missing("\tNULL\t"));
        assert!(detector.is_missing(" ")); // Just whitespace = empty = missing
        assert!(detector.is_missing("  "));
    }

    #[test]
    fn test_custom_tokens() {
        let detector = MissingDetector::with_tokens(vec![
            "MISSING".to_string(),
            "?".to_string(),
            "unknown".to_string(),
        ]);

        assert!(detector.is_missing("MISSING"));
        assert!(detector.is_missing("missing"));
        assert!(detector.is_missing("?"));
        assert!(detector.is_missing("unknown"));
        assert!(detector.is_missing("UNKNOWN"));

        // Default tokens should not match
        assert!(!detector.is_missing("NA"));
        assert!(!detector.is_missing("NULL"));
    }

    #[test]
    fn test_missing_stats() {
        let mut stats = MissingStats::new();

        stats.record(false);
        stats.record(false);
        stats.record(true);
        stats.record(false);
        stats.record(true);

        assert_eq!(stats.total_count, 5);
        assert_eq!(stats.missing_count, 2);
        assert!((stats.missing_percentage() - 40.0).abs() < 0.001);
        assert!(stats.has_missing());
    }

    #[test]
    fn test_missing_stats_no_missing() {
        let mut stats = MissingStats::new();

        stats.record(false);
        stats.record(false);
        stats.record(false);

        assert_eq!(stats.total_count, 3);
        assert_eq!(stats.missing_count, 0);
        assert!((stats.missing_percentage() - 0.0).abs() < 0.001);
        assert!(!stats.has_missing());
    }

    #[test]
    fn test_missing_stats_all_missing() {
        let mut stats = MissingStats::new();

        stats.record(true);
        stats.record(true);
        stats.record(true);

        assert_eq!(stats.total_count, 3);
        assert_eq!(stats.missing_count, 3);
        assert!((stats.missing_percentage() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_missing_stats_empty() {
        let stats = MissingStats::new();

        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.missing_count, 0);
        assert!((stats.missing_percentage() - 0.0).abs() < 0.001);
        assert!(!stats.has_missing());
    }

    #[test]
    fn test_column_missing_stats() {
        let detector = MissingDetector::default();
        let mut tracker = ColumnMissingStats::new(3, detector);

        tracker.record_row(&["hello", "NA", "world"]);
        tracker.record_row(&["foo", "bar", "NULL"]);
        tracker.record_row(&["", "baz", "qux"]);

        let col0 = tracker.column_stats(0).unwrap();
        assert_eq!(col0.total_count, 3);
        assert_eq!(col0.missing_count, 1);

        let col1 = tracker.column_stats(1).unwrap();
        assert_eq!(col1.total_count, 3);
        assert_eq!(col1.missing_count, 1);

        let col2 = tracker.column_stats(2).unwrap();
        assert_eq!(col2.total_count, 3);
        assert_eq!(col2.missing_count, 1);
    }

    #[test]
    fn test_percentage_calculation() {
        let mut stats = MissingStats::new();

        // 1 out of 4 = 25%
        stats.record(true);
        stats.record(false);
        stats.record(false);
        stats.record(false);

        assert!((stats.missing_percentage() - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_percentage_edge_cases() {
        // Test rounding/precision
        let mut stats = MissingStats::new();

        // 1 out of 3 = 33.333...%
        stats.record(true);
        stats.record(false);
        stats.record(false);

        let pct = stats.missing_percentage();
        assert!(pct > 33.0 && pct < 34.0);
    }
}
