use std::collections::BTreeMap;

use crate::bucket::{get_bucket_key, parse_datetime, TimeBucket};
use crate::cli::TimeGrain;
use crate::missing::MissingDetector;
use crate::warnings::{Severity, Warning, WarningCode};

/// Thresholds for drift detection
pub mod thresholds {
    /// Missing rate change threshold (absolute)
    pub const MISSING_RATE_CHANGE: f64 = 0.10;
    /// Numeric mean change threshold (relative)
    pub const MEAN_CHANGE_RELATIVE: f64 = 0.20;
    /// Row count change threshold (relative)
    pub const ROW_COUNT_CHANGE: f64 = 0.50;
}

/// Drift analysis result
#[derive(Debug, Clone)]
pub struct DriftResult {
    pub time_column: String,
    pub grain: TimeGrain,
    pub buckets: Vec<BucketSummary>,
    pub warnings: Vec<Warning>,
    pub column_names: Vec<String>,
    pub analyzed_rows: usize,
    pub skipped_timestamp_rows: usize,
    pub skipped_malformed_rows: usize,
}

/// Summary of a single time bucket
#[derive(Debug, Clone)]
pub struct BucketSummary {
    pub key: String,
    pub row_count: usize,
    pub missing_rates: Vec<f64>,
    pub numeric_means: Vec<Option<f64>>,
}

/// Drift detector for analyzing time-series data
pub struct DriftDetector {
    time_col_idx: usize,
    grain: TimeGrain,
    missing_detector: MissingDetector,
    buckets: BTreeMap<String, TimeBucket>,
    column_names: Vec<String>,
    num_columns: usize,
    non_finite_values: usize,
    analyzed_rows: usize,
    skipped_timestamp_rows: usize,
}

impl DriftDetector {
    pub fn new(
        time_col_idx: usize,
        grain: TimeGrain,
        missing_detector: MissingDetector,
        column_names: Vec<String>,
    ) -> Self {
        let num_columns = column_names.len();
        Self {
            time_col_idx,
            grain,
            missing_detector,
            buckets: BTreeMap::new(),
            column_names,
            num_columns,
            non_finite_values: 0,
            analyzed_rows: 0,
            skipped_timestamp_rows: 0,
        }
    }

    /// Records a row of data
    pub fn record_row(&mut self, values: &[&str]) {
        // Get timestamp value
        let time_value = values.get(self.time_col_idx).copied().unwrap_or("");

        // Parse timestamp
        let timestamp = if self.missing_detector.is_missing(time_value) {
            None
        } else {
            parse_datetime(time_value)
        };
        let bucket_key = match timestamp {
            Some(dt) => get_bucket_key(dt, self.grain),
            None => {
                self.skipped_timestamp_rows += 1;
                return;
            }
        };
        self.analyzed_rows += 1;

        // Get or create bucket
        let bucket = self
            .buckets
            .entry(bucket_key.clone())
            .or_insert_with(|| TimeBucket::new(bucket_key, self.num_columns));

        bucket.row_count += 1;

        // Record column values
        for (col_idx, value) in values.iter().enumerate() {
            if col_idx == self.time_col_idx {
                continue;
            }

            let is_missing = self.missing_detector.is_missing(value);

            if is_missing {
                bucket.record_missing(col_idx);
            } else {
                // Try to parse as numeric
                if let Ok(num) = value.trim().parse::<f64>() {
                    if num.is_finite() {
                        bucket.record_numeric(col_idx, num);
                    } else {
                        self.non_finite_values += 1;
                    }
                }
            }
        }
    }

    /// Analyzes the collected data for drift
    pub fn analyze(&self) -> DriftResult {
        let mut warnings = Vec::new();
        if self.non_finite_values > 0 {
            warnings.push(Warning::for_file(
                Severity::Warning,
                format!(
                    "Excluded {} non-finite values from numeric means",
                    self.non_finite_values
                ),
                WarningCode::NonFiniteNumeric,
            ));
        }
        if self.skipped_timestamp_rows > 0 {
            warnings.push(Warning::for_column(
                Severity::Warning,
                &self.column_names[self.time_col_idx],
                format!(
                    "Skipped {} rows with missing or invalid timestamps",
                    self.skipped_timestamp_rows
                ),
                WarningCode::InvalidTimestamp,
            ));
        }
        if self.buckets.len() < 2 {
            warnings.push(Warning::for_file(
                Severity::Warning,
                "At least two populated time buckets are needed to compare drift".to_owned(),
                WarningCode::InsufficientBuckets,
            ));
        }

        // Convert buckets to summaries
        let summaries: Vec<BucketSummary> = self
            .buckets
            .values()
            .map(|bucket| BucketSummary {
                key: bucket.key.clone(),
                row_count: bucket.row_count,
                missing_rates: (0..self.num_columns)
                    .map(|i| bucket.missing_rate(i))
                    .collect(),
                numeric_means: (0..self.num_columns)
                    .map(|i| bucket.numeric_mean(i))
                    .collect(),
            })
            .collect();

        // Detect drift between consecutive buckets
        if summaries.len() >= 2 {
            for i in 1..summaries.len() {
                let prev = &summaries[i - 1];
                let curr = &summaries[i];

                // Check row count drift
                if prev.row_count > 0 {
                    let change = (curr.row_count as f64 - prev.row_count as f64).abs()
                        / prev.row_count as f64;
                    if change > thresholds::ROW_COUNT_CHANGE {
                        warnings.push(Warning::for_file(
                            Severity::Warning,
                            format!(
                                "Row count changed {:.0}% between {} and {} ({} -> {})",
                                change * 100.0,
                                prev.key,
                                curr.key,
                                prev.row_count,
                                curr.row_count
                            ),
                            WarningCode::RowCountChanged,
                        ));
                    }
                }

                // Check per-column drift
                for col_idx in 0..self.num_columns {
                    if col_idx == self.time_col_idx {
                        continue;
                    }

                    let col_name = self
                        .column_names
                        .get(col_idx)
                        .map(|s| s.as_str())
                        .unwrap_or("unknown");

                    // Missing rate drift
                    let prev_missing = prev.missing_rates.get(col_idx).copied().unwrap_or(0.0);
                    let curr_missing = curr.missing_rates.get(col_idx).copied().unwrap_or(0.0);
                    let missing_change = (curr_missing - prev_missing).abs();

                    if missing_change > thresholds::MISSING_RATE_CHANGE {
                        warnings.push(Warning::for_column(
                            Severity::Warning,
                            col_name,
                            format!(
                                "Missing rate changed from {:.1}% to {:.1}% between {} and {}",
                                prev_missing * 100.0,
                                curr_missing * 100.0,
                                prev.key,
                                curr.key
                            ),
                            WarningCode::MissingRateChanged,
                        ));
                    }

                    // Numeric mean drift
                    if let (Some(prev_mean), Some(curr_mean)) = (
                        prev.numeric_means.get(col_idx).and_then(|m| *m),
                        curr.numeric_means.get(col_idx).and_then(|m| *m),
                    ) {
                        if prev_mean != 0.0 {
                            let relative_change = (curr_mean - prev_mean).abs() / prev_mean.abs();
                            if relative_change > thresholds::MEAN_CHANGE_RELATIVE {
                                warnings.push(Warning::for_column(
                                    Severity::Warning,
                                    col_name,
                                    format!(
                                        "Mean changed {:.1}% between {} and {} ({:.2} -> {:.2})",
                                        relative_change * 100.0,
                                        prev.key,
                                        curr.key,
                                        prev_mean,
                                        curr_mean
                                    ),
                                    WarningCode::MeanChanged,
                                ));
                            }
                        } else if curr_mean != 0.0 {
                            warnings.push(Warning::for_column(
                                Severity::Warning,
                                col_name,
                                format!(
                                    "Mean changed from zero between {} and {} (0.00 -> {:.2})",
                                    prev.key, curr.key, curr_mean
                                ),
                                WarningCode::MeanChanged,
                            ));
                        }
                    }
                }
            }
        }

        DriftResult {
            time_column: self
                .column_names
                .get(self.time_col_idx)
                .cloned()
                .unwrap_or_default(),
            grain: self.grain,
            buckets: summaries,
            warnings,
            column_names: self.column_names.clone(),
            analyzed_rows: self.analyzed_rows,
            skipped_timestamp_rows: self.skipped_timestamp_rows,
            skipped_malformed_rows: 0,
        }
    }
}

/// Renders drift result to terminal
pub fn render_drift(result: &DriftResult, use_color: bool) -> String {
    use colored::Colorize;

    let mut output = String::new();

    // Header
    let title = "Drift Analysis";
    let title_display = if use_color {
        title.bold().to_string()
    } else {
        title.to_string()
    };

    output.push_str(&format!("{}\n", title_display));
    output.push_str(&format!("{}\n", "─".repeat(60)));

    output.push_str(&format!("  Time Column: {}\n", result.time_column));
    output.push_str(&format!("  Grain:       {:?}\n", result.grain));
    output.push_str(&format!("  Buckets:     {}\n", result.buckets.len()));
    output.push_str(&format!("  Analyzed:    {} rows\n", result.analyzed_rows));
    output.push_str(&format!(
        "  Skipped:     {} invalid timestamps, {} malformed rows\n",
        result.skipped_timestamp_rows, result.skipped_malformed_rows
    ));
    output.push('\n');

    // Warnings
    if !result.warnings.is_empty() {
        let warnings_title = "Drift Warnings";
        let warnings_display = if use_color {
            warnings_title.bold().to_string()
        } else {
            warnings_title.to_string()
        };

        output.push_str(&format!("{}\n", warnings_display));
        output.push_str(&format!("{}\n", "─".repeat(60)));

        for warning in &result.warnings {
            let severity_str = if use_color {
                "WARNING".yellow().bold().to_string()
            } else {
                "WARNING".to_string()
            };

            let col_str = warning
                .column
                .as_ref()
                .map(|c| format!("[{}] ", c))
                .unwrap_or_default();

            output.push_str(&format!(
                "  {} {}{}\n",
                severity_str, col_str, warning.message
            ));
        }
        output.push('\n');
    }

    // Bucket summary table
    let table_title = "Bucket Summary";
    let table_display = if use_color {
        table_title.bold().to_string()
    } else {
        table_title.to_string()
    };

    output.push_str(&format!("{}\n", table_display));
    output.push_str(&format!("{}\n", "─".repeat(60)));

    // Table header
    output.push_str(&format!("  {:15} {:>10}\n", "Bucket", "Rows"));
    output.push_str(&format!(
        "  {:15} {:>10}\n",
        "───────────────", "──────────"
    ));

    for bucket in &result.buckets {
        output.push_str(&format!("  {:15} {:>10}\n", bucket.key, bucket.row_count));
    }

    output
}

/// Converts drift result to JSON
pub fn drift_to_json(result: &DriftResult) -> Result<String, serde_json::Error> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct JsonDriftResult {
        time_column: String,
        grain: String,
        bucket_count: usize,
        analyzed_rows: usize,
        skipped_timestamp_rows: usize,
        skipped_malformed_rows: usize,
        warnings: Vec<JsonDriftWarning>,
        buckets: Vec<JsonBucket>,
    }

    #[derive(Serialize)]
    struct JsonDriftWarning {
        severity: String,
        code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        column: Option<String>,
        message: String,
    }

    #[derive(Serialize)]
    struct JsonBucket {
        key: String,
        row_count: usize,
        missing_rates: BTreeMap<String, f64>,
        numeric_means: BTreeMap<String, f64>,
    }

    let json_result = JsonDriftResult {
        time_column: result.time_column.clone(),
        grain: format!("{:?}", result.grain).to_lowercase(),
        bucket_count: result.buckets.len(),
        analyzed_rows: result.analyzed_rows,
        skipped_timestamp_rows: result.skipped_timestamp_rows,
        skipped_malformed_rows: result.skipped_malformed_rows,
        warnings: result
            .warnings
            .iter()
            .map(|w| JsonDriftWarning {
                severity: format!("{}", w.severity),
                code: w.code.to_string(),
                column: w.column.clone(),
                message: w.message.clone(),
            })
            .collect(),
        buckets: result
            .buckets
            .iter()
            .map(|b| {
                let mut missing_rates = BTreeMap::new();
                let mut numeric_means = BTreeMap::new();

                for (i, name) in result.column_names.iter().enumerate() {
                    if let Some(&rate) = b.missing_rates.get(i) {
                        if rate > 0.0 {
                            missing_rates.insert(name.clone(), rate);
                        }
                    }
                    if let Some(Some(mean)) = b.numeric_means.get(i) {
                        numeric_means.insert(name.clone(), *mean);
                    }
                }

                JsonBucket {
                    key: b.key.clone(),
                    row_count: b.row_count,
                    missing_rates,
                    numeric_means,
                }
            })
            .collect(),
    };

    serde_json::to_string_pretty(&json_result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_detector() -> DriftDetector {
        DriftDetector::new(
            2, // time column at index 2
            TimeGrain::Day,
            MissingDetector::default(),
            vec!["id".to_string(), "value".to_string(), "date".to_string()],
        )
    }

    #[test]
    fn test_drift_detector_basic() {
        let mut detector = create_detector();

        // Day 1
        detector.record_row(&["1", "10", "2024-01-15"]);
        detector.record_row(&["2", "20", "2024-01-15"]);

        // Day 2
        detector.record_row(&["3", "30", "2024-01-16"]);
        detector.record_row(&["4", "40", "2024-01-16"]);

        let result = detector.analyze();
        assert_eq!(result.buckets.len(), 2);
        assert_eq!(result.buckets[0].row_count, 2);
        assert_eq!(result.buckets[1].row_count, 2);
    }

    #[test]
    fn test_drift_detector_missing() {
        let mut detector = create_detector();

        detector.record_row(&["1", "10", "2024-01-15"]);
        detector.record_row(&["2", "NA", "2024-01-15"]);

        let result = detector.analyze();
        assert_eq!(result.buckets.len(), 1);

        // Check missing rate for "value" column (index 1)
        let missing_rate = result.buckets[0].missing_rates[1];
        assert!((missing_rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_drift_detector_numeric_mean() {
        let mut detector = create_detector();

        detector.record_row(&["1", "10", "2024-01-15"]);
        detector.record_row(&["2", "20", "2024-01-15"]);
        detector.record_row(&["3", "30", "2024-01-15"]);

        let result = detector.analyze();

        // Mean of "value" column should be 20
        let mean = result.buckets[0].numeric_means[1].unwrap();
        assert!((mean - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_drift_detection_row_count() {
        let mut detector = create_detector();

        // Day 1: 10 rows
        for i in 0..10 {
            detector.record_row(&[&i.to_string(), "10", "2024-01-15"]);
        }

        // Day 2: 2 rows (80% decrease)
        for i in 0..2 {
            detector.record_row(&[&i.to_string(), "10", "2024-01-16"]);
        }

        let result = detector.analyze();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Row count")));
    }

    #[test]
    fn test_drift_detection_missing_rate() {
        let mut detector = create_detector();

        // Day 1: 0% missing
        for i in 0..10 {
            detector.record_row(&[&i.to_string(), &i.to_string(), "2024-01-15"]);
        }

        // Day 2: 50% missing
        for i in 0..5 {
            detector.record_row(&[&i.to_string(), &i.to_string(), "2024-01-16"]);
        }
        for i in 5..10 {
            detector.record_row(&[&i.to_string(), "NA", "2024-01-16"]);
        }

        let result = detector.analyze();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Missing rate")));
    }

    #[test]
    fn test_drift_detection_mean_change() {
        let mut detector = create_detector();

        // Day 1: mean = 10
        for _ in 0..10 {
            detector.record_row(&["1", "10", "2024-01-15"]);
        }

        // Day 2: mean = 100 (900% increase)
        for _ in 0..10 {
            detector.record_row(&["1", "100", "2024-01-16"]);
        }

        let result = detector.analyze();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("Mean changed")));
    }

    #[test]
    fn test_render_drift() {
        let mut detector = create_detector();

        detector.record_row(&["1", "10", "2024-01-15"]);
        detector.record_row(&["2", "20", "2024-01-16"]);

        let result = detector.analyze();
        let output = render_drift(&result, false);

        assert!(output.contains("Drift Analysis"));
        assert!(output.contains("date"));
        assert!(output.contains("2024-01-15"));
    }

    #[test]
    fn test_drift_to_json() {
        let mut detector = create_detector();

        detector.record_row(&["1", "10", "2024-01-15"]);
        detector.record_row(&["2", "20", "2024-01-16"]);

        let result = detector.analyze();
        let json = drift_to_json(&result).unwrap();

        assert!(json.contains("\"time_column\""));
        assert!(json.contains("\"grain\""));
        assert!(json.contains("\"buckets\""));
    }

    #[test]
    fn test_invalid_timestamps_skipped() {
        let mut detector = create_detector();

        detector.record_row(&["1", "10", "2024-01-15"]);
        detector.record_row(&["2", "20", "not-a-date"]);
        detector.record_row(&["3", "30", "2024-01-15"]);

        let result = detector.analyze();
        // Only one bucket, invalid date row skipped
        assert_eq!(result.buckets.len(), 1);
        assert_eq!(result.buckets[0].row_count, 2);
    }
}
