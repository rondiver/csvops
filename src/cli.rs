use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// CSV profiling and drift detection CLI tool
#[derive(Parser, Debug)]
#[command(name = "csvops")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Profile a CSV file and display statistics
    Profile(ProfileArgs),
    /// Detect drift in a CSV file over time
    Drift(DriftArgs),
}

#[derive(Parser, Debug)]
pub struct ProfileArgs {
    /// Path to the CSV file to profile
    pub file: PathBuf,

    /// Output format as JSON
    #[arg(long)]
    pub json: bool,

    /// Delimiter character (auto-detected if not specified)
    #[arg(short, long)]
    pub delimiter: Option<char>,

    /// Treat first row as data, not header
    #[arg(long)]
    pub no_header: bool,

    /// Custom missing value tokens (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub missing: Option<Vec<String>>,

    /// Sample size for statistics (default: 10000)
    #[arg(long, default_value = "10000")]
    pub sample_size: usize,

    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,
}

#[derive(Parser, Debug)]
pub struct DriftArgs {
    /// Path to the CSV file to analyze
    pub file: PathBuf,

    /// Column containing timestamp for drift detection
    #[arg(long)]
    pub time_col: String,

    /// Time grain for bucketing (day, week, month)
    #[arg(long, default_value = "day")]
    pub grain: TimeGrain,

    /// Output format as JSON
    #[arg(long)]
    pub json: bool,

    /// Delimiter character (auto-detected if not specified)
    #[arg(short, long)]
    pub delimiter: Option<char>,

    /// Treat first row as data, not header
    #[arg(long)]
    pub no_header: bool,

    /// Custom missing value tokens (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub missing: Option<Vec<String>>,

    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeGrain {
    Day,
    Week,
    Month,
}

impl std::str::FromStr for TimeGrain {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "day" => Ok(TimeGrain::Day),
            "week" => Ok(TimeGrain::Week),
            "month" => Ok(TimeGrain::Month),
            _ => Err(format!("Invalid time grain: {}. Use day, week, or month.", s)),
        }
    }
}

/// Exit codes for the CLI
pub mod exit_codes {
    /// Success
    pub const SUCCESS: i32 = 0;
    /// Runtime error (file not found, parse error, etc.)
    pub const ERROR: i32 = 1;
    /// Invalid arguments
    pub const INVALID_ARGS: i32 = 2;
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parse_profile() {
        let cli = Cli::parse_from(["csvops", "profile", "test.csv"]);
        match cli.command {
            Command::Profile(args) => {
                assert_eq!(args.file, PathBuf::from("test.csv"));
                assert!(!args.json);
                assert!(!args.no_header);
            }
            _ => panic!("Expected Profile command"),
        }
    }

    #[test]
    fn test_cli_parse_profile_with_flags() {
        let cli = Cli::parse_from([
            "csvops", "profile", "data.csv",
            "--json",
            "--delimiter", ",",
            "--no-header",
            "--missing", "NA,N/A,NULL",
            "--sample-size", "5000",
            "--no-color",
        ]);
        match cli.command {
            Command::Profile(args) => {
                assert_eq!(args.file, PathBuf::from("data.csv"));
                assert!(args.json);
                assert_eq!(args.delimiter, Some(','));
                assert!(args.no_header);
                assert_eq!(args.missing, Some(vec!["NA".to_string(), "N/A".to_string(), "NULL".to_string()]));
                assert_eq!(args.sample_size, 5000);
                assert!(args.no_color);
            }
            _ => panic!("Expected Profile command"),
        }
    }

    #[test]
    fn test_cli_parse_drift() {
        let cli = Cli::parse_from([
            "csvops", "drift", "test.csv",
            "--time-col", "created_at",
        ]);
        match cli.command {
            Command::Drift(args) => {
                assert_eq!(args.file, PathBuf::from("test.csv"));
                assert_eq!(args.time_col, "created_at");
                assert_eq!(args.grain, TimeGrain::Day);
            }
            _ => panic!("Expected Drift command"),
        }
    }

    #[test]
    fn test_cli_parse_drift_with_grain() {
        let cli = Cli::parse_from([
            "csvops", "drift", "test.csv",
            "--time-col", "timestamp",
            "--grain", "week",
            "--json",
        ]);
        match cli.command {
            Command::Drift(args) => {
                assert_eq!(args.time_col, "timestamp");
                assert_eq!(args.grain, TimeGrain::Week);
                assert!(args.json);
            }
            _ => panic!("Expected Drift command"),
        }
    }

    #[test]
    fn test_time_grain_from_str() {
        assert_eq!("day".parse::<TimeGrain>().unwrap(), TimeGrain::Day);
        assert_eq!("week".parse::<TimeGrain>().unwrap(), TimeGrain::Week);
        assert_eq!("month".parse::<TimeGrain>().unwrap(), TimeGrain::Month);
        assert_eq!("DAY".parse::<TimeGrain>().unwrap(), TimeGrain::Day);
        assert!("invalid".parse::<TimeGrain>().is_err());
    }

    #[test]
    fn test_cli_help_output() {
        // Verify help can be generated without panic
        let cmd = Cli::command();
        let mut help = Vec::new();
        cmd.clone().write_help(&mut help).unwrap();
        let help_str = String::from_utf8(help).unwrap();
        assert!(help_str.contains("csvops"));
        assert!(help_str.contains("profile"));
        assert!(help_str.contains("drift"));
    }

    #[test]
    fn test_cli_version() {
        let cmd = Cli::command();
        assert!(cmd.get_version().is_some());
    }
}
