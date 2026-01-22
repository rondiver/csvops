use serde::Serialize;
use serde_json;

use crate::output::ProfileResult;
use crate::types::DataType;
use crate::warnings::Severity;

/// JSON output structure for profile results
#[derive(Debug, Serialize)]
pub struct JsonProfile {
    pub file: FileInfo,
    pub warnings: Vec<JsonWarning>,
    pub columns: Vec<JsonColumn>,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub path: String,
    pub size_bytes: u64,
    pub row_count: usize,
    pub column_count: usize,
    pub delimiter: String,
    pub has_header: bool,
}

#[derive(Debug, Serialize)]
pub struct JsonWarning {
    pub severity: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct JsonColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    pub missing_count: usize,
    pub missing_percentage: f64,
    pub cardinality: usize,
    pub cardinality_exact: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric_stats: Option<JsonNumericStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string_stats: Option<JsonStringStats>,
    pub top_values: Vec<JsonTopValue>,
    pub heuristics: JsonHeuristics,
}

#[derive(Debug, Serialize)]
pub struct JsonNumericStats {
    pub mean: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub std_dev: Option<f64>,
    pub min: f64,
    pub max: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p25: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p75: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p99: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct JsonStringStats {
    pub min_length: usize,
    pub max_length: usize,
}

#[derive(Debug, Serialize)]
pub struct JsonTopValue {
    pub value: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct JsonHeuristics {
    pub is_id_column: bool,
    pub is_constant: bool,
    pub is_mostly_missing: bool,
    pub has_mixed_types: bool,
    pub has_outliers: bool,
    pub outlier_count: usize,
}

/// Converts a ProfileResult to JSON format
pub fn to_json(result: &ProfileResult) -> Result<String, serde_json::Error> {
    let json_profile = convert_to_json_profile(result);
    serde_json::to_string_pretty(&json_profile)
}

/// Converts a ProfileResult to compact JSON
pub fn to_json_compact(result: &ProfileResult) -> Result<String, serde_json::Error> {
    let json_profile = convert_to_json_profile(result);
    serde_json::to_string(&json_profile)
}

fn convert_to_json_profile(result: &ProfileResult) -> JsonProfile {
    let file = FileInfo {
        path: result.file_path.clone(),
        size_bytes: result.file_size_bytes,
        row_count: result.row_count,
        column_count: result.column_count,
        delimiter: format_delimiter(result.delimiter),
        has_header: result.has_header,
    };

    let warnings: Vec<JsonWarning> = result
        .warnings
        .iter()
        .map(|w| JsonWarning {
            severity: format!("{}", w.severity),
            code: format!("{}", w.code),
            column: w.column.clone(),
            message: w.message.clone(),
        })
        .collect();

    // Sort columns alphabetically for deterministic output
    let mut columns: Vec<_> = result.columns.iter().collect();
    columns.sort_by(|a, b| a.name.cmp(&b.name));

    let columns: Vec<JsonColumn> = columns
        .iter()
        .map(|col| convert_column(col))
        .collect();

    JsonProfile {
        file,
        warnings,
        columns,
    }
}

fn convert_column(col: &crate::output::ColumnProfile) -> JsonColumn {
    let stats = &col.stats;
    let heuristics = &col.heuristics;

    let numeric_stats = if matches!(stats.inferred_type(), DataType::Integer | DataType::Float) {
        stats.numeric_stats.mean().map(|mean| {
            let mut sampler = col.stats.numeric_sampler.clone();
            JsonNumericStats {
                mean,
                std_dev: stats.numeric_stats.std_dev(),
                min: stats.numeric_stats.min().unwrap_or(0.0),
                max: stats.numeric_stats.max().unwrap_or(0.0),
                p25: sampler.percentile(25.0),
                p50: sampler.percentile(50.0),
                p75: sampler.percentile(75.0),
                p99: sampler.percentile(99.0),
            }
        })
    } else {
        None
    };

    let string_stats = if stats.min_length.is_some() {
        Some(JsonStringStats {
            min_length: stats.min_length.unwrap_or(0),
            max_length: stats.max_length.unwrap_or(0),
        })
    } else {
        None
    };

    let top_values: Vec<JsonTopValue> = stats
        .top_values
        .top_k(10)
        .into_iter()
        .map(|(value, count)| JsonTopValue { value, count })
        .collect();

    JsonColumn {
        name: col.name.clone(),
        data_type: format!("{}", stats.inferred_type()),
        missing_count: stats.missing.missing_count,
        missing_percentage: stats.missing.missing_percentage(),
        cardinality: stats.cardinality(),
        cardinality_exact: stats.cardinality.is_exact(),
        numeric_stats,
        string_stats,
        top_values,
        heuristics: JsonHeuristics {
            is_id_column: heuristics.is_id_column,
            is_constant: heuristics.is_constant,
            is_mostly_missing: heuristics.is_mostly_missing,
            has_mixed_types: heuristics.has_mixed_types,
            has_outliers: heuristics.has_outliers,
            outlier_count: heuristics.outlier_count,
        },
    }
}

fn format_delimiter(d: char) -> String {
    match d {
        '\t' => "\\t".to_string(),
        _ => d.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heuristics::ColumnHeuristics;
    use crate::missing::MissingDetector;
    use crate::output::ColumnProfile;
    use crate::stats::ColumnStats;
    use crate::warnings::{Warning, WarningCode};

    fn create_test_result() -> ProfileResult {
        let detector = MissingDetector::default();
        let mut stats = ColumnStats::new("test_col".to_string(), 1000);

        for i in 0..100 {
            stats.record(&i.to_string(), &detector);
        }

        let heuristics = ColumnHeuristics::analyze(&mut stats);

        ProfileResult {
            file_path: "test.csv".to_string(),
            file_size_bytes: 1024,
            row_count: 100,
            column_count: 1,
            delimiter: ',',
            has_header: true,
            columns: vec![ColumnProfile::new("test_col".to_string(), stats, heuristics)],
            warnings: vec![],
        }
    }

    #[test]
    fn test_to_json() {
        let result = create_test_result();
        let json = to_json(&result).unwrap();

        assert!(json.contains("\"path\": \"test.csv\""));
        assert!(json.contains("\"row_count\": 100"));
        assert!(json.contains("\"test_col\""));
    }

    #[test]
    fn test_json_structure() {
        let result = create_test_result();
        let json = to_json(&result).unwrap();

        // Parse back and verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["file"].is_object());
        assert!(parsed["warnings"].is_array());
        assert!(parsed["columns"].is_array());
    }

    #[test]
    fn test_json_with_warnings() {
        let mut result = create_test_result();
        result.warnings = vec![
            Warning::for_column(
                Severity::Warning,
                "col1",
                "Test warning".to_string(),
                WarningCode::MixedTypes,
            ),
        ];

        let json = to_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["warnings"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["warnings"][0]["severity"], "warning");
    }

    #[test]
    fn test_json_numeric_stats() {
        let detector = MissingDetector::default();
        let mut stats = ColumnStats::new("numbers".to_string(), 1000);

        for i in 1..=100 {
            stats.record(&i.to_string(), &detector);
        }

        let heuristics = ColumnHeuristics::analyze(&mut stats);

        let result = ProfileResult {
            file_path: "test.csv".to_string(),
            file_size_bytes: 1024,
            row_count: 100,
            column_count: 1,
            delimiter: ',',
            has_header: true,
            columns: vec![ColumnProfile::new("numbers".to_string(), stats, heuristics)],
            warnings: vec![],
        };

        let json = to_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let num_stats = &parsed["columns"][0]["numeric_stats"];
        assert!(num_stats["mean"].is_number());
        assert!(num_stats["min"].is_number());
        assert!(num_stats["max"].is_number());
    }

    #[test]
    fn test_json_deterministic() {
        let result = create_test_result();

        let json1 = to_json(&result).unwrap();
        let json2 = to_json(&result).unwrap();

        assert_eq!(json1, json2);
    }

    #[test]
    fn test_json_delimiter_formatting() {
        assert_eq!(format_delimiter(','), ",");
        assert_eq!(format_delimiter('\t'), "\\t");
        assert_eq!(format_delimiter('|'), "|");
    }

    #[test]
    fn test_json_compact() {
        let result = create_test_result();
        let json = to_json_compact(&result).unwrap();

        // Compact JSON should not have newlines (except possibly in string values)
        assert!(!json.contains("\n  ")); // No indentation
    }
}
