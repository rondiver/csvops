mod bucket;
mod cardinality;
mod cli;
mod delimiter;
mod drift;
mod heuristics;
mod missing;
mod output;
mod reader;
mod sampling;
mod stats;
mod types;
mod warnings;

use clap::Parser;
use cli::{exit_codes, Cli, Command};
use drift::DriftDetector;
use heuristics::ColumnHeuristics;
use missing::MissingDetector;
use output::{ColumnProfile, ProfileResult};
use reader::{CsvReader, CsvReaderConfig};
use stats::ColumnStats;
use std::fs;
use std::io::{self, Write};
use std::process;
use warnings::{
    generate_column_warnings, generate_file_warnings, sort_warnings, Severity, Warning, WarningCode,
};

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Profile(args) => run_profile(args),
        Command::Drift(args) => run_drift(args),
    };

    match result {
        Ok(()) => process::exit(exit_codes::SUCCESS),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(exit_codes::ERROR);
        }
    }
}

fn run_profile(args: cli::ProfileArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Validate file exists
    if !args.file.exists() {
        return Err(format!("File not found: {}", args.file.display()).into());
    }

    // Get file metadata
    let metadata = fs::metadata(&args.file)?;
    let file_size = metadata.len();

    // Configure reader
    let config = CsvReaderConfig {
        delimiter: args.delimiter,
        has_header: !args.no_header,
    };

    // Open and read CSV
    let mut reader = CsvReader::open(&args.file, config)?;

    // Setup missing detector
    let missing_detector = match args.missing {
        Some(tokens) => MissingDetector::with_tokens(tokens),
        None => MissingDetector::default(),
    };

    // Initialize column stats
    let headers = reader.headers().to_vec();
    let mut column_stats: Vec<ColumnStats> = headers
        .iter()
        .map(|name| ColumnStats::new(name.clone(), args.sample_size))
        .collect();

    // Process rows
    for record_result in reader.records() {
        let record = record_result?;
        for (stats, value) in column_stats.iter_mut().zip(&record) {
            stats.record(value, &missing_detector);
        }
    }
    let row_count = reader.row_count();

    // Generate heuristics and warnings for each column
    let mut all_warnings = Vec::new();
    let mut columns: Vec<ColumnProfile> = Vec::new();

    for mut stats in column_stats {
        if stats
            .numeric_stats
            .mean()
            .into_iter()
            .chain(stats.numeric_stats.std_dev())
            .any(|value| !value.is_finite())
        {
            return Err(format!("Numeric overflow in column '{}': values exceed the range supported by floating-point statistics", stats.name).into());
        }
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let col_warnings = generate_column_warnings(&stats, &heuristics);
        all_warnings.extend(col_warnings);

        columns.push(ColumnProfile::new(stats.name.clone(), stats, heuristics));
    }

    // Generate file-level warnings
    let total_missing: usize = columns.iter().map(|c| c.stats.missing.missing_count).sum();
    let total_cells = row_count * columns.len();
    let file_warnings =
        generate_file_warnings(row_count, columns.len(), total_missing, total_cells);
    all_warnings.extend(file_warnings);
    if let Some(warning) = malformed_warning(&reader) {
        all_warnings.push(warning);
    }

    // Sort warnings by severity
    sort_warnings(&mut all_warnings);

    // Build result
    let result = ProfileResult {
        file_path: args.file.display().to_string(),
        file_size_bytes: file_size,
        row_count,
        skipped_rows: reader.skipped_rows(),
        column_count: columns.len(),
        delimiter: reader.delimiter(),
        has_header: !args.no_header,
        columns,
        warnings: all_warnings,
    };

    // Output
    if args.json {
        let mut json = output::json::to_json(&result)?;
        json.push('\n');
        write_report(&json)?;
    } else {
        let output = output::terminal::render(&result, !args.no_color);
        write_report(&output)?;
    }

    Ok(())
}

fn run_drift(args: cli::DriftArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Validate file exists
    if !args.file.exists() {
        return Err(format!("File not found: {}", args.file.display()).into());
    }

    // Configure reader
    let config = CsvReaderConfig {
        delimiter: args.delimiter,
        has_header: !args.no_header,
    };

    // Open and read CSV
    let mut reader = CsvReader::open(&args.file, config)?;

    // Get headers and find time column index
    let headers = reader.headers().to_vec();
    let time_col_idx = headers
        .iter()
        .position(|h| h == &args.time_col)
        .ok_or_else(|| {
            format!(
                "Time column '{}' not found. Available columns: {:?}",
                args.time_col, headers
            )
        })?;

    // Setup missing detector
    let missing_detector = match args.missing {
        Some(tokens) => MissingDetector::with_tokens(tokens),
        None => MissingDetector::default(),
    };

    // Create drift detector
    let mut detector = DriftDetector::new(time_col_idx, args.grain, missing_detector, headers);

    // Process rows
    for record_result in reader.records() {
        let record = record_result?;
        let values: Vec<&str> = record.iter().collect();
        detector.record_row(&values);
    }

    // Analyze and output
    let mut result = detector.analyze();
    if result
        .buckets
        .iter()
        .flat_map(|bucket| &bucket.numeric_means)
        .flatten()
        .any(|mean| !mean.is_finite())
    {
        return Err("Numeric overflow while calculating bucket means: values exceed the supported floating-point range".into());
    }
    result.skipped_malformed_rows = reader.skipped_rows();
    if let Some(warning) = malformed_warning(&reader) {
        result.warnings.push(warning);
    }
    if result.analyzed_rows == 0 {
        return Err(format!(
            "No rows with a valid timestamp in '{}': {} invalid timestamps, {} malformed rows. Check --time-col and the documented date formats.",
            result.time_column, result.skipped_timestamp_rows, result.skipped_malformed_rows
        ).into());
    }

    if args.json {
        let mut json = drift::drift_to_json(&result)?;
        json.push('\n');
        write_report(&json)?;
    } else {
        let output = drift::render_drift(&result, !args.no_color);
        write_report(&output)?;
    }

    Ok(())
}

fn malformed_warning<R: std::io::Read>(reader: &CsvReader<R>) -> Option<Warning> {
    if reader.skipped_rows() == 0 {
        return None;
    }
    let examples = reader
        .malformed_rows()
        .iter()
        .map(|row| format!("line {}: {}", row.line_number, row.reason))
        .collect::<Vec<_>>()
        .join("; ");
    Some(Warning::for_file(
        Severity::Warning,
        format!(
            "Skipped {} rows with inconsistent field counts. {}",
            reader.skipped_rows(),
            examples
        ),
        WarningCode::MalformedRows,
    ))
}

fn write_report(report: &str) -> io::Result<()> {
    match io::stdout().lock().write_all(report.as_bytes()) {
        // A consumer such as `head` may finish before the full report is read.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}
