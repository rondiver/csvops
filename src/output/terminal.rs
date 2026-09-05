use colored::Colorize;

use crate::output::ProfileResult;
use crate::types::DataType;
use crate::warnings::Severity;

/// Renders the profile result to the terminal
pub fn render(result: &ProfileResult, use_color: bool) -> String {
    let mut output = String::new();

    // File stats section
    output.push_str(&render_file_stats(result, use_color));
    output.push('\n');

    // Warnings section (if any)
    if !result.warnings.is_empty() {
        output.push_str(&render_warnings(result, use_color));
        output.push('\n');
    }

    // Column details section
    output.push_str(&render_column_details(result, use_color));

    output
}

fn render_file_stats(result: &ProfileResult, use_color: bool) -> String {
    let mut output = String::new();

    let title = "File Statistics";
    let title_display = if use_color {
        title.bold().to_string()
    } else {
        title.to_string()
    };

    output.push_str(&format!("{}\n", title_display));
    output.push_str(&format!("{}\n", "─".repeat(50)));

    output.push_str(&format!("  File:       {}\n", result.file_path));
    output.push_str(&format!(
        "  Size:       {}\n",
        format_bytes(result.file_size_bytes)
    ));
    output.push_str(&format!(
        "  Rows:       {}\n",
        format_number(result.row_count)
    ));
    if result.skipped_rows > 0 {
        output.push_str(&format!(
            "  Skipped:    {} malformed rows\n",
            result.skipped_rows
        ));
    }
    output.push_str(&format!("  Columns:    {}\n", result.column_count));
    output.push_str(&format!(
        "  Delimiter:  '{}'\n",
        format_delimiter(result.delimiter)
    ));
    output.push_str(&format!(
        "  Header:     {}\n",
        if result.has_header { "yes" } else { "no" }
    ));

    output
}

fn render_warnings(result: &ProfileResult, use_color: bool) -> String {
    let mut output = String::new();

    let title = "Warnings";
    let title_display = if use_color {
        title.bold().to_string()
    } else {
        title.to_string()
    };

    output.push_str(&format!("{}\n", title_display));
    output.push_str(&format!("{}\n", "─".repeat(50)));

    for warning in &result.warnings {
        let severity_str = match warning.severity {
            Severity::Critical => {
                if use_color {
                    "CRITICAL".red().bold().to_string()
                } else {
                    "CRITICAL".to_string()
                }
            }
            Severity::Warning => {
                if use_color {
                    "WARNING".yellow().bold().to_string()
                } else {
                    "WARNING".to_string()
                }
            }
            Severity::Info => {
                if use_color {
                    "INFO".blue().to_string()
                } else {
                    "INFO".to_string()
                }
            }
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

    output
}

fn render_column_details(result: &ProfileResult, use_color: bool) -> String {
    let mut output = String::new();

    let title = "Column Details";
    let title_display = if use_color {
        title.bold().to_string()
    } else {
        title.to_string()
    };

    output.push_str(&format!("{}\n", title_display));
    output.push_str(&format!("{}\n", "─".repeat(80)));

    // Sort columns alphabetically for deterministic output
    let mut columns: Vec<_> = result.columns.iter().collect();
    columns.sort_by(|a, b| a.name.cmp(&b.name));

    for col in columns {
        output.push_str(&render_column(col, use_color));
        output.push('\n');
    }

    output
}

fn render_column(col: &crate::output::ColumnProfile, use_color: bool) -> String {
    let mut output = String::new();

    let name_display = if use_color {
        col.name.cyan().bold().to_string()
    } else {
        col.name.clone()
    };

    output.push_str(&format!("  {}\n", name_display));

    let stats = &col.stats;

    // Type
    let type_str = format!("{}", stats.inferred_type());
    let type_display = if use_color {
        type_str.green().to_string()
    } else {
        type_str
    };
    output.push_str(&format!("    Type:         {}\n", type_display));

    // Missing
    let missing_pct = stats.missing.missing_percentage();
    let missing_str = format!(
        "{} ({:.1}%)",
        format_number(stats.missing.missing_count),
        missing_pct
    );
    output.push_str(&format!("    Missing:      {}\n", missing_str));

    // Cardinality
    let cardinality = stats.cardinality();
    let cardinality_suffix = if stats.cardinality.is_exact() {
        ""
    } else {
        " (estimated)"
    };
    output.push_str(&format!(
        "    Cardinality:  {}{}\n",
        format_number(cardinality),
        cardinality_suffix
    ));

    // Numeric stats (if applicable)
    if matches!(stats.inferred_type(), DataType::Integer | DataType::Float) {
        if let Some(mean) = stats.numeric_stats.mean() {
            output.push_str(&format!("    Mean:         {:.4}\n", mean));
        }
        if let Some(std) = stats.numeric_stats.std_dev() {
            output.push_str(&format!("    Std Dev:      {:.4}\n", std));
        }
        if let Some(min) = stats.numeric_stats.min() {
            output.push_str(&format!("    Min:          {}\n", format_numeric(min)));
        }
        if let Some(max) = stats.numeric_stats.max() {
            output.push_str(&format!("    Max:          {}\n", format_numeric(max)));
        }
    }

    // Top values (for non-ID columns with low cardinality)
    if !col.heuristics.is_id_column && cardinality <= 20 {
        let top = stats.top_values.top_k(5);
        if !top.is_empty() {
            output.push_str("    Top values:\n");
            for (value, count) in top {
                let display_value = if value.chars().count() > 30 {
                    format!("{}...", value.chars().take(30).collect::<String>())
                } else {
                    value
                };
                output.push_str(&format!("      - {} ({})\n", display_value, count));
            }
        }
    }

    output
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*c);
    }

    result
}

fn format_numeric(n: f64) -> String {
    if n.abs() >= 1_000_000.0 {
        format!("{:.2e}", n)
    } else if n.fract() == 0.0 {
        format!("{:.0}", n)
    } else {
        format!("{:.4}", n)
    }
}

fn format_delimiter(d: char) -> String {
    match d {
        '\t' => "\\t (tab)".to_string(),
        ',' => ",".to_string(),
        '|' => "|".to_string(),
        ';' => ";".to_string(),
        _ => format!("{}", d),
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
        stats.record("NA", &detector);

        let mut heuristics = ColumnHeuristics::analyze(&mut stats);
        // Reset for deterministic test
        heuristics.is_id_column = false;

        ProfileResult {
            file_path: "test.csv".to_string(),
            file_size_bytes: 1024,
            row_count: 100,
            skipped_rows: 0,
            column_count: 1,
            delimiter: ',',
            has_header: true,
            columns: vec![ColumnProfile::new(
                "test_col".to_string(),
                stats,
                heuristics,
            )],
            warnings: vec![],
        }
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 bytes");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(100), "100");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1000000), "1,000,000");
    }

    #[test]
    fn test_format_delimiter() {
        assert_eq!(format_delimiter(','), ",");
        assert_eq!(format_delimiter('\t'), "\\t (tab)");
        assert_eq!(format_delimiter('|'), "|");
    }

    #[test]
    fn test_render_file_stats() {
        let result = create_test_result();
        let output = render_file_stats(&result, false);

        assert!(output.contains("File Statistics"));
        assert!(output.contains("test.csv"));
        assert!(output.contains("1.00 KB"));
        assert!(output.contains("100"));
    }

    #[test]
    fn test_render_with_warnings() {
        let mut result = create_test_result();
        result.warnings = vec![Warning::for_column(
            Severity::Warning,
            "col1",
            "Test warning".to_string(),
            WarningCode::MixedTypes,
        )];

        let output = render(&result, false);
        assert!(output.contains("Warnings"));
        assert!(output.contains("WARNING"));
        assert!(output.contains("Test warning"));
    }

    #[test]
    fn test_render_column_details() {
        let result = create_test_result();
        let output = render_column_details(&result, false);

        assert!(output.contains("Column Details"));
        assert!(output.contains("test_col"));
        assert!(output.contains("Type:"));
        assert!(output.contains("Missing:"));
        assert!(output.contains("Cardinality:"));
    }

    #[test]
    fn test_render_deterministic() {
        let result = create_test_result();

        let output1 = render(&result, false);
        let output2 = render(&result, false);

        assert_eq!(output1, output2);
    }

    #[test]
    fn test_format_numeric() {
        assert_eq!(format_numeric(42.0), "42");
        assert_eq!(format_numeric(2.3456), "2.3456");
        assert!(format_numeric(1_000_000.0).contains("e"));
    }
}
