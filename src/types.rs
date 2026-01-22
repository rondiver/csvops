use regex::Regex;
use std::sync::LazyLock;

/// Inferred data types for CSV columns
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    Integer,
    Float,
    Boolean,
    Datetime,
    String,
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::Integer => write!(f, "integer"),
            DataType::Float => write!(f, "float"),
            DataType::Boolean => write!(f, "boolean"),
            DataType::Datetime => write!(f, "datetime"),
            DataType::String => write!(f, "string"),
        }
    }
}

// Regex patterns for type detection
static INTEGER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-?\d+$").unwrap()
});

static FLOAT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-?\d+\.\d+$|^-?\d+\.?[eE][+-]?\d+$|^-?\d+\.\d+[eE][+-]?\d+$").unwrap()
});

static BOOLEAN_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(true|false|yes|no|t|f|y|n|1|0)$").unwrap()
});

// ISO 8601: 2024-01-15, 2024-01-15T10:30:00, 2024-01-15T10:30:00Z, 2024-01-15T10:30:00+05:00
static ISO_DATETIME_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?(\.\d+)?(Z|[+-]\d{2}:?\d{2})?)?$").unwrap()
});

// US format: 01/15/2024, 1/15/2024, 01-15-2024
static US_DATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{1,2}[/-]\d{1,2}[/-]\d{2,4}$").unwrap()
});

// European format: 15/01/2024, 15-01-2024
static EU_DATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{1,2}[/-]\d{1,2}[/-]\d{2,4}$").unwrap()
});

// Unix epoch (seconds or milliseconds)
static UNIX_EPOCH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // 10 digits = seconds (1970-2100), 13 digits = milliseconds
    Regex::new(r"^1\d{9}$|^1\d{12}$").unwrap()
});

/// Detects the type of a single value
pub fn detect_type(value: &str) -> DataType {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return DataType::String;
    }

    // Check in order of specificity
    if BOOLEAN_PATTERN.is_match(trimmed) {
        return DataType::Boolean;
    }

    if INTEGER_PATTERN.is_match(trimmed) {
        // Could be unix epoch
        if UNIX_EPOCH_PATTERN.is_match(trimmed) {
            return DataType::Datetime;
        }
        return DataType::Integer;
    }

    if FLOAT_PATTERN.is_match(trimmed) {
        return DataType::Float;
    }

    if ISO_DATETIME_PATTERN.is_match(trimmed) {
        return DataType::Datetime;
    }

    // US/EU date patterns are ambiguous, but we detect them as datetime
    if US_DATE_PATTERN.is_match(trimmed) || EU_DATE_PATTERN.is_match(trimmed) {
        return DataType::Datetime;
    }

    DataType::String
}

/// Tracks type statistics for a column
#[derive(Debug, Clone, Default)]
pub struct TypeStats {
    pub integer_count: usize,
    pub float_count: usize,
    pub boolean_count: usize,
    pub datetime_count: usize,
    pub string_count: usize,
    pub total_count: usize,
}

impl TypeStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a value's type
    pub fn record(&mut self, value: &str, is_missing: bool) {
        self.total_count += 1;

        if is_missing {
            // Don't count missing values towards type inference
            return;
        }

        match detect_type(value) {
            DataType::Integer => self.integer_count += 1,
            DataType::Float => self.float_count += 1,
            DataType::Boolean => self.boolean_count += 1,
            DataType::Datetime => self.datetime_count += 1,
            DataType::String => self.string_count += 1,
        }
    }

    /// Returns the inferred type for this column
    ///
    /// Uses a threshold of 5% to determine if a column has mixed types.
    /// If more than 5% of non-missing values don't match the dominant type,
    /// it's considered a string column.
    pub fn inferred_type(&self) -> DataType {
        let non_missing = self.integer_count + self.float_count +
            self.boolean_count + self.datetime_count + self.string_count;

        if non_missing == 0 {
            return DataType::String;
        }

        // Find the dominant type
        let counts = [
            (DataType::Integer, self.integer_count),
            (DataType::Float, self.float_count),
            (DataType::Boolean, self.boolean_count),
            (DataType::Datetime, self.datetime_count),
            (DataType::String, self.string_count),
        ];

        let (dominant_type, dominant_count) = counts
            .iter()
            .max_by_key(|(_, count)| count)
            .unwrap();

        // Check if mixed: more than 5% of non-missing values are different type
        let other_count = non_missing - dominant_count;
        let threshold = (non_missing as f64 * 0.05).ceil() as usize;

        if other_count > threshold {
            // Consider integers as compatible with floats
            if *dominant_type == DataType::Float && self.integer_count > 0 {
                let numeric_count = self.integer_count + self.float_count;
                let numeric_other = non_missing - numeric_count;
                if numeric_other <= threshold {
                    return DataType::Float;
                }
            }
            if *dominant_type == DataType::Integer && self.float_count > 0 {
                let numeric_count = self.integer_count + self.float_count;
                let numeric_other = non_missing - numeric_count;
                if numeric_other <= threshold {
                    return DataType::Float;
                }
            }

            return DataType::String;
        }

        *dominant_type
    }

    /// Returns true if the column has mixed types (>5% threshold)
    pub fn is_mixed_type(&self) -> bool {
        let non_missing = self.integer_count + self.float_count +
            self.boolean_count + self.datetime_count + self.string_count;

        if non_missing == 0 {
            return false;
        }

        let counts = [
            self.integer_count,
            self.float_count,
            self.boolean_count,
            self.datetime_count,
            self.string_count,
        ];

        let dominant_count = *counts.iter().max().unwrap();
        let other_count = non_missing - dominant_count;
        let threshold = (non_missing as f64 * 0.05).ceil() as usize;

        // Special case: integers and floats together are not "mixed"
        let numeric_count = self.integer_count + self.float_count;
        if numeric_count == non_missing {
            return false;
        }

        other_count > threshold
    }

    /// Returns the type distribution as percentages
    pub fn type_distribution(&self) -> Vec<(DataType, f64)> {
        let non_missing = self.integer_count + self.float_count +
            self.boolean_count + self.datetime_count + self.string_count;

        if non_missing == 0 {
            return vec![];
        }

        let total = non_missing as f64;
        vec![
            (DataType::Integer, self.integer_count as f64 / total * 100.0),
            (DataType::Float, self.float_count as f64 / total * 100.0),
            (DataType::Boolean, self.boolean_count as f64 / total * 100.0),
            (DataType::Datetime, self.datetime_count as f64 / total * 100.0),
            (DataType::String, self.string_count as f64 / total * 100.0),
        ]
        .into_iter()
        .filter(|(_, pct)| *pct > 0.0)
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integer tests
    #[test]
    fn test_detect_integer() {
        assert_eq!(detect_type("123"), DataType::Integer);
        assert_eq!(detect_type("-456"), DataType::Integer);
        assert_eq!(detect_type("0"), DataType::Boolean); // 0 matches boolean
        assert_eq!(detect_type("1"), DataType::Boolean); // 1 matches boolean
        assert_eq!(detect_type("42"), DataType::Integer);
    }

    #[test]
    fn test_detect_large_integer() {
        assert_eq!(detect_type("999999999"), DataType::Integer);
        assert_eq!(detect_type("-999999999"), DataType::Integer);
    }

    // Float tests
    #[test]
    fn test_detect_float() {
        assert_eq!(detect_type("3.14"), DataType::Float);
        assert_eq!(detect_type("-2.5"), DataType::Float);
        assert_eq!(detect_type("0.0"), DataType::Float);
        assert_eq!(detect_type("123.456"), DataType::Float);
    }

    #[test]
    fn test_detect_scientific_notation() {
        assert_eq!(detect_type("1e10"), DataType::Float);
        assert_eq!(detect_type("1E10"), DataType::Float);
        assert_eq!(detect_type("1.5e-3"), DataType::Float);
        assert_eq!(detect_type("-2.5E+10"), DataType::Float);
    }

    // Boolean tests
    #[test]
    fn test_detect_boolean() {
        assert_eq!(detect_type("true"), DataType::Boolean);
        assert_eq!(detect_type("false"), DataType::Boolean);
        assert_eq!(detect_type("TRUE"), DataType::Boolean);
        assert_eq!(detect_type("FALSE"), DataType::Boolean);
        assert_eq!(detect_type("True"), DataType::Boolean);
        assert_eq!(detect_type("False"), DataType::Boolean);
    }

    #[test]
    fn test_detect_boolean_yes_no() {
        assert_eq!(detect_type("yes"), DataType::Boolean);
        assert_eq!(detect_type("no"), DataType::Boolean);
        assert_eq!(detect_type("YES"), DataType::Boolean);
        assert_eq!(detect_type("NO"), DataType::Boolean);
        assert_eq!(detect_type("y"), DataType::Boolean);
        assert_eq!(detect_type("n"), DataType::Boolean);
        assert_eq!(detect_type("Y"), DataType::Boolean);
        assert_eq!(detect_type("N"), DataType::Boolean);
    }

    #[test]
    fn test_detect_boolean_tf() {
        assert_eq!(detect_type("t"), DataType::Boolean);
        assert_eq!(detect_type("f"), DataType::Boolean);
        assert_eq!(detect_type("T"), DataType::Boolean);
        assert_eq!(detect_type("F"), DataType::Boolean);
        assert_eq!(detect_type("0"), DataType::Boolean);
        assert_eq!(detect_type("1"), DataType::Boolean);
    }

    // Datetime tests
    #[test]
    fn test_detect_iso_datetime() {
        assert_eq!(detect_type("2024-01-15"), DataType::Datetime);
        assert_eq!(detect_type("2024-01-15T10:30:00"), DataType::Datetime);
        assert_eq!(detect_type("2024-01-15T10:30:00Z"), DataType::Datetime);
        assert_eq!(detect_type("2024-01-15T10:30:00+05:00"), DataType::Datetime);
        assert_eq!(detect_type("2024-01-15T10:30:00.123Z"), DataType::Datetime);
    }

    #[test]
    fn test_detect_us_date() {
        assert_eq!(detect_type("01/15/2024"), DataType::Datetime);
        assert_eq!(detect_type("1/15/2024"), DataType::Datetime);
        assert_eq!(detect_type("01-15-2024"), DataType::Datetime);
        assert_eq!(detect_type("12/31/24"), DataType::Datetime);
    }

    #[test]
    fn test_detect_eu_date() {
        assert_eq!(detect_type("15/01/2024"), DataType::Datetime);
        assert_eq!(detect_type("15-01-2024"), DataType::Datetime);
    }

    #[test]
    fn test_detect_unix_epoch() {
        // Seconds (10 digits starting with 1)
        assert_eq!(detect_type("1705334400"), DataType::Datetime);
        // Milliseconds (13 digits starting with 1)
        assert_eq!(detect_type("1705334400000"), DataType::Datetime);
    }

    // String tests
    #[test]
    fn test_detect_string() {
        assert_eq!(detect_type("hello"), DataType::String);
        assert_eq!(detect_type("hello world"), DataType::String);
        assert_eq!(detect_type("abc123"), DataType::String);
        assert_eq!(detect_type("foo@bar.com"), DataType::String);
    }

    #[test]
    fn test_detect_empty_string() {
        assert_eq!(detect_type(""), DataType::String);
        assert_eq!(detect_type("  "), DataType::String);
    }

    // Edge cases
    #[test]
    fn test_whitespace_handling() {
        assert_eq!(detect_type("  123  "), DataType::Integer);
        assert_eq!(detect_type("\ttrue\t"), DataType::Boolean);
        assert_eq!(detect_type(" 2024-01-15 "), DataType::Datetime);
    }

    // TypeStats tests
    #[test]
    fn test_type_stats_integer_column() {
        let mut stats = TypeStats::new();
        stats.record("10", false);
        stats.record("20", false);
        stats.record("30", false);
        stats.record("40", false);

        assert_eq!(stats.inferred_type(), DataType::Integer);
        assert!(!stats.is_mixed_type());
    }

    #[test]
    fn test_type_stats_float_column() {
        let mut stats = TypeStats::new();
        stats.record("1.1", false);
        stats.record("2.2", false);
        stats.record("3.3", false);

        assert_eq!(stats.inferred_type(), DataType::Float);
    }

    #[test]
    fn test_type_stats_mixed_int_float() {
        let mut stats = TypeStats::new();
        stats.record("10", false);
        stats.record("2.5", false);
        stats.record("30", false);
        stats.record("4.5", false);

        // Integers and floats together should be float
        assert_eq!(stats.inferred_type(), DataType::Float);
        assert!(!stats.is_mixed_type()); // int+float is not "mixed"
    }

    #[test]
    fn test_type_stats_mixed_type_threshold() {
        let mut stats = TypeStats::new();

        // 95 integers
        for _ in 0..95 {
            stats.record("42", false);
        }
        // 5 strings (exactly 5%)
        for _ in 0..5 {
            stats.record("hello", false);
        }

        // At exactly 5%, should still be integer
        assert_eq!(stats.inferred_type(), DataType::Integer);
    }

    #[test]
    fn test_type_stats_mixed_over_threshold() {
        let mut stats = TypeStats::new();

        // 90 integers
        for _ in 0..90 {
            stats.record("42", false);
        }
        // 10 strings (10% > 5%)
        for _ in 0..10 {
            stats.record("hello", false);
        }

        // Over 5% other types, should be string
        assert_eq!(stats.inferred_type(), DataType::String);
        assert!(stats.is_mixed_type());
    }

    #[test]
    fn test_type_stats_with_missing() {
        let mut stats = TypeStats::new();
        stats.record("10", false);
        stats.record("NA", true);  // missing, should not affect inference
        stats.record("20", false);
        stats.record("NULL", true);  // missing
        stats.record("30", false);

        assert_eq!(stats.inferred_type(), DataType::Integer);
        assert_eq!(stats.total_count, 5);
        assert_eq!(stats.integer_count, 3);
    }

    #[test]
    fn test_type_stats_all_missing() {
        let mut stats = TypeStats::new();
        stats.record("NA", true);
        stats.record("NULL", true);
        stats.record("", true);

        // All missing should default to string
        assert_eq!(stats.inferred_type(), DataType::String);
    }

    #[test]
    fn test_type_distribution() {
        let mut stats = TypeStats::new();
        stats.record("10", false);
        stats.record("20", false);
        stats.record("hello", false);
        stats.record("world", false);

        let dist = stats.type_distribution();
        assert!(dist.iter().any(|(t, _)| *t == DataType::Integer));
        assert!(dist.iter().any(|(t, _)| *t == DataType::String));
    }
}
