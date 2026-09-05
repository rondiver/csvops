use crate::heuristics::ColumnHeuristics;
use crate::stats::ColumnStats;

/// Severity levels for warnings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Critical => write!(f, "critical"),
        }
    }
}

/// A warning about data quality issues
#[derive(Debug, Clone)]
pub struct Warning {
    pub severity: Severity,
    pub column: Option<String>,
    pub message: String,
    pub code: WarningCode,
}

/// Warning codes for categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    HighMissingRate,
    MixedTypes,
    ConstantColumn,
    HighCardinality,
    LowCardinality,
    OutliersDetected,
    PotentialIdColumn,
    EmptyColumn,
    AllMissing,
    MalformedRows,
    InvalidTimestamp,
    InsufficientBuckets,
    RowCountChanged,
    MissingRateChanged,
    MeanChanged,
    NonFiniteNumeric,
}

impl std::fmt::Display for WarningCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningCode::HighMissingRate => write!(f, "HIGH_MISSING_RATE"),
            WarningCode::MixedTypes => write!(f, "MIXED_TYPES"),
            WarningCode::ConstantColumn => write!(f, "CONSTANT_COLUMN"),
            WarningCode::HighCardinality => write!(f, "HIGH_CARDINALITY"),
            WarningCode::LowCardinality => write!(f, "LOW_CARDINALITY"),
            WarningCode::OutliersDetected => write!(f, "OUTLIERS_DETECTED"),
            WarningCode::PotentialIdColumn => write!(f, "POTENTIAL_ID_COLUMN"),
            WarningCode::EmptyColumn => write!(f, "EMPTY_COLUMN"),
            WarningCode::AllMissing => write!(f, "ALL_MISSING"),
            WarningCode::MalformedRows => write!(f, "MALFORMED_ROWS"),
            WarningCode::InvalidTimestamp => write!(f, "INVALID_TIMESTAMP"),
            WarningCode::InsufficientBuckets => write!(f, "INSUFFICIENT_BUCKETS"),
            WarningCode::RowCountChanged => write!(f, "ROW_COUNT_CHANGED"),
            WarningCode::MissingRateChanged => write!(f, "MISSING_RATE_CHANGED"),
            WarningCode::MeanChanged => write!(f, "MEAN_CHANGED"),
            WarningCode::NonFiniteNumeric => write!(f, "NON_FINITE_NUMERIC"),
        }
    }
}

impl Warning {
    pub fn new(
        severity: Severity,
        column: Option<String>,
        message: String,
        code: WarningCode,
    ) -> Self {
        Self {
            severity,
            column,
            message,
            code,
        }
    }

    /// Creates a column-specific warning
    pub fn for_column(
        severity: Severity,
        column: &str,
        message: String,
        code: WarningCode,
    ) -> Self {
        Self::new(severity, Some(column.to_string()), message, code)
    }

    /// Creates a file-level warning
    pub fn for_file(severity: Severity, message: String, code: WarningCode) -> Self {
        Self::new(severity, None, message, code)
    }
}

/// Generates warnings based on column statistics and heuristics
pub fn generate_column_warnings(
    stats: &ColumnStats,
    heuristics: &ColumnHeuristics,
) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let col_name = &stats.name;

    // Check for all missing values
    if stats.missing.total_count > 0 && stats.missing.missing_count == stats.missing.total_count {
        warnings.push(Warning::for_column(
            Severity::Critical,
            col_name,
            "All values are missing".to_string(),
            WarningCode::AllMissing,
        ));
        return warnings; // No other warnings apply
    }

    // Check for mostly missing (>50%)
    if stats.non_finite_count > 0 {
        warnings.push(Warning::for_column(
            Severity::Warning,
            col_name,
            format!(
                "Excluded {} non-finite values from numeric statistics",
                stats.non_finite_count
            ),
            WarningCode::NonFiniteNumeric,
        ));
    }

    if heuristics.is_mostly_missing {
        let pct = stats.missing.missing_percentage();
        warnings.push(Warning::for_column(
            Severity::Warning,
            col_name,
            format!("{:.1}% of values are missing", pct),
            WarningCode::HighMissingRate,
        ));
    }

    // Check for mixed types
    if heuristics.has_mixed_types {
        let dist = stats.types.type_distribution();
        let types_str: Vec<_> = dist
            .iter()
            .filter(|(_, pct)| *pct > 1.0)
            .map(|(t, pct)| format!("{}:{:.0}%", t, pct))
            .collect();
        warnings.push(Warning::for_column(
            Severity::Warning,
            col_name,
            format!("Mixed types detected: {}", types_str.join(", ")),
            WarningCode::MixedTypes,
        ));
    }

    // Check for constant column
    if heuristics.is_constant {
        let top = stats.top_values.top_k(1);
        let value = top.first().map(|(v, _)| v.as_str()).unwrap_or("(unknown)");
        let display_value = if value.chars().count() > 20 {
            format!("{}...", value.chars().take(20).collect::<String>())
        } else {
            value.to_string()
        };
        warnings.push(Warning::for_column(
            Severity::Info,
            col_name,
            format!("Constant value: '{}'", display_value),
            WarningCode::ConstantColumn,
        ));
    }

    // Check for high cardinality
    if heuristics.has_high_cardinality && !heuristics.is_id_column {
        warnings.push(Warning::for_column(
            Severity::Info,
            col_name,
            format!(
                "High cardinality: {} distinct values ({:.1}% unique)",
                stats.cardinality(),
                (stats.cardinality() as f64 / stats.missing.total_count as f64) * 100.0
            ),
            WarningCode::HighCardinality,
        ));
    }

    // Check for potential ID column (info only)
    if heuristics.is_id_column {
        warnings.push(Warning::for_column(
            Severity::Info,
            col_name,
            "Appears to be an identifier column".to_string(),
            WarningCode::PotentialIdColumn,
        ));
    }

    // Check for outliers
    if heuristics.has_outliers {
        warnings.push(Warning::for_column(
            Severity::Warning,
            col_name,
            format!(
                "{} potential outliers detected (>10×IQR from p1/p99)",
                heuristics.outlier_count
            ),
            WarningCode::OutliersDetected,
        ));
    }

    // Check for low cardinality (potential categorical)
    if heuristics.has_low_cardinality && !heuristics.is_constant {
        let top = stats.top_values.top_k(5);
        let values: Vec<_> = top.iter().map(|(v, _)| v.as_str()).collect();
        warnings.push(Warning::for_column(
            Severity::Info,
            col_name,
            format!(
                "Low cardinality ({} distinct): may be categorical. Top values: {}",
                stats.cardinality(),
                values.join(", ")
            ),
            WarningCode::LowCardinality,
        ));
    }

    warnings
}

/// Generates file-level warnings
pub fn generate_file_warnings(
    row_count: usize,
    _column_count: usize,
    total_missing: usize,
    total_cells: usize,
) -> Vec<Warning> {
    let mut warnings = Vec::new();

    // Check for empty file
    if row_count == 0 {
        warnings.push(Warning::for_file(
            Severity::Critical,
            "File contains no data rows".to_string(),
            WarningCode::EmptyColumn,
        ));
    }

    // Check overall missing rate
    if total_cells > 0 {
        let missing_pct = (total_missing as f64 / total_cells as f64) * 100.0;
        if missing_pct > 20.0 {
            warnings.push(Warning::for_file(
                Severity::Warning,
                format!("{:.1}% of all cells are missing", missing_pct),
                WarningCode::HighMissingRate,
            ));
        }
    }

    warnings
}

/// Sorts warnings by severity (critical first, then warning, then info)
pub fn sort_warnings(warnings: &mut [Warning]) {
    warnings.sort_by(|a, b| b.severity.cmp(&a.severity));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heuristics::ColumnHeuristics;
    use crate::missing::MissingDetector;
    use crate::stats::ColumnStats;

    fn create_stats_with_values(name: &str, values: &[&str]) -> ColumnStats {
        let detector = MissingDetector::default();
        let mut stats = ColumnStats::new(name.to_string(), 1000);
        for v in values {
            stats.record(v, &detector);
        }
        stats
    }

    #[test]
    fn test_warning_severity_order() {
        assert!(Severity::Critical > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }

    #[test]
    fn test_all_missing_warning() {
        let mut stats = create_stats_with_values("col", &["NA", "NULL", ""]);
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let warnings = generate_column_warnings(&stats, &heuristics);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, WarningCode::AllMissing);
        assert_eq!(warnings[0].severity, Severity::Critical);
    }

    #[test]
    fn test_high_missing_warning() {
        let mut values = vec!["value"; 40];
        values.extend(vec!["NA"; 60]);

        let mut stats = create_stats_with_values("col", &values);
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let warnings = generate_column_warnings(&stats, &heuristics);

        assert!(warnings
            .iter()
            .any(|w| w.code == WarningCode::HighMissingRate));
    }

    #[test]
    fn test_mixed_types_warning() {
        let mut values: Vec<&str> = (0..80).map(|_| "123").collect();
        values.extend(vec!["text"; 20]);

        let mut stats = create_stats_with_values("col", &values);
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let warnings = generate_column_warnings(&stats, &heuristics);

        assert!(warnings.iter().any(|w| w.code == WarningCode::MixedTypes));
    }

    #[test]
    fn test_constant_column_warning() {
        let values = vec!["same"; 100];
        let mut stats = create_stats_with_values("col", &values);
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let warnings = generate_column_warnings(&stats, &heuristics);

        assert!(warnings
            .iter()
            .any(|w| w.code == WarningCode::ConstantColumn));
    }

    #[test]
    fn test_id_column_warning() {
        let values: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        let values_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();

        let mut stats = create_stats_with_values("user_id", &values_refs);
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let warnings = generate_column_warnings(&stats, &heuristics);

        assert!(warnings
            .iter()
            .any(|w| w.code == WarningCode::PotentialIdColumn));
    }

    #[test]
    fn test_low_cardinality_warning() {
        let values: Vec<String> = (0..200).map(|i| format!("cat_{}", i % 5)).collect();
        let values_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();

        let mut stats = create_stats_with_values("category", &values_refs);
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let warnings = generate_column_warnings(&stats, &heuristics);

        assert!(warnings
            .iter()
            .any(|w| w.code == WarningCode::LowCardinality));
    }

    #[test]
    fn test_outlier_warning() {
        let mut values: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        values.push("10000".to_string()); // Outlier

        let values_refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();

        let mut stats = create_stats_with_values("values", &values_refs);
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let warnings = generate_column_warnings(&stats, &heuristics);

        assert!(warnings
            .iter()
            .any(|w| w.code == WarningCode::OutliersDetected));
    }

    #[test]
    fn test_file_warnings_empty() {
        let warnings = generate_file_warnings(0, 5, 0, 0);
        assert!(warnings.iter().any(|w| w.severity == Severity::Critical));
    }

    #[test]
    fn test_file_warnings_high_missing() {
        let warnings = generate_file_warnings(100, 5, 150, 500);
        assert!(warnings
            .iter()
            .any(|w| w.code == WarningCode::HighMissingRate));
    }

    #[test]
    fn test_sort_warnings() {
        let mut warnings = vec![
            Warning::for_file(
                Severity::Info,
                "info".to_string(),
                WarningCode::LowCardinality,
            ),
            Warning::for_file(
                Severity::Critical,
                "critical".to_string(),
                WarningCode::AllMissing,
            ),
            Warning::for_file(
                Severity::Warning,
                "warning".to_string(),
                WarningCode::MixedTypes,
            ),
        ];

        sort_warnings(&mut warnings);

        assert_eq!(warnings[0].severity, Severity::Critical);
        assert_eq!(warnings[1].severity, Severity::Warning);
        assert_eq!(warnings[2].severity, Severity::Info);
    }

    #[test]
    fn test_warning_code_display() {
        assert_eq!(
            format!("{}", WarningCode::HighMissingRate),
            "HIGH_MISSING_RATE"
        );
        assert_eq!(format!("{}", WarningCode::MixedTypes), "MIXED_TYPES");
    }
}
