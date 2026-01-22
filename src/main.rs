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
use std::process;
use warnings::{generate_column_warnings, generate_file_warnings, sort_warnings};

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
    let num_columns = if headers.is_empty() {
        // We'll determine this from first row
        0
    } else {
        headers.len()
    };

    let mut column_stats: Vec<ColumnStats> = if num_columns > 0 {
        headers
            .iter()
            .map(|name| ColumnStats::new(name.clone(), args.sample_size))
            .collect()
    } else {
        Vec::new()
    };

    // Process rows
    let mut row_count = 0;
    for record_result in reader.records() {
        let record = record_result?;

        // Initialize column stats from first row if no header
        if column_stats.is_empty() {
            column_stats = (0..record.len())
                .map(|i| ColumnStats::new(format!("column_{}", i), args.sample_size))
                .collect();
        }

        // Record values for each column
        for (i, value) in record.iter().enumerate() {
            if i < column_stats.len() {
                column_stats[i].record(value, &missing_detector);
            }
        }

        row_count += 1;
    }

    // Generate heuristics and warnings for each column
    let mut all_warnings = Vec::new();
    let mut columns: Vec<ColumnProfile> = Vec::new();

    for mut stats in column_stats {
        let heuristics = ColumnHeuristics::analyze(&mut stats);
        let col_warnings = generate_column_warnings(&stats, &heuristics);
        all_warnings.extend(col_warnings);

        columns.push(ColumnProfile::new(stats.name.clone(), stats, heuristics));
    }

    // Generate file-level warnings
    let total_missing: usize = columns.iter().map(|c| c.stats.missing.missing_count).sum();
    let total_cells = row_count * columns.len();
    let file_warnings = generate_file_warnings(row_count, columns.len(), total_missing, total_cells);
    all_warnings.extend(file_warnings);

    // Sort warnings by severity
    sort_warnings(&mut all_warnings);

    // Build result
    let result = ProfileResult {
        file_path: args.file.display().to_string(),
        file_size_bytes: file_size,
        row_count,
        column_count: columns.len(),
        delimiter: reader.delimiter(),
        has_header: !args.no_header,
        columns,
        warnings: all_warnings,
    };

    // Output
    if args.json {
        let json = output::json::to_json(&result)?;
        println!("{}", json);
    } else {
        let output = output::terminal::render(&result, !args.no_color);
        print!("{}", output);
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
        .ok_or_else(|| format!("Time column '{}' not found. Available columns: {:?}", args.time_col, headers))?;

    // Setup missing detector
    let missing_detector = match args.missing {
        Some(tokens) => MissingDetector::with_tokens(tokens),
        None => MissingDetector::default(),
    };

    // Create drift detector
    let mut detector = DriftDetector::new(
        time_col_idx,
        args.grain,
        missing_detector,
        headers,
    );

    // Process rows
    for record_result in reader.records() {
        let record = record_result?;
        let values: Vec<&str> = record.iter().collect();
        detector.record_row(&values);
    }

    // Analyze and output
    let result = detector.analyze();

    if args.json {
        let json = drift::drift_to_json(&result)?;
        println!("{}", json);
    } else {
        let output = drift::render_drift(&result, !args.no_color);
        print!("{}", output);
    }

    Ok(())
}
