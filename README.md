# csvops

A fast CLI tool for CSV profiling and **data drift detection**, written in Rust.

## Why Drift Detection?

CSV profiling tools are common. What's missing is automated detection of *how your data changes over time*.

**Drift detection catches silent failures:**
- A vendor quietly changes their export format
- An upstream team stops populating a field
- A data pipeline starts dropping records
- Numeric distributions shift unexpectedly

These issues don't throw errors — they just make your dashboards wrong, your ML models degrade, and your reports misleading. By the time someone notices, the damage is done.

`csvops drift` analyzes your data across time buckets and alerts you when metrics change significantly — before problems compound.

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
./target/release/csvops --help
```

## Quick Start

```bash
# Profile a CSV file
csvops profile data.csv

# Detect drift over time (the key feature)
csvops drift data.csv --time-col created_at --grain week
```

## Usage

### Drift Command

Detect data drift over time by analyzing metrics across time buckets:

```bash
csvops drift data.csv --time-col created_at --grain week
```

Options:
- `--time-col <name>` - Column containing timestamps (required)
- `--grain <day|week|month>` - Time bucket granularity (default: day)
- `--json` - Output as JSON
- `--delimiter <char>` - Specify delimiter
- `--no-header` - Treat first row as data
- `--missing <tokens>` - Custom missing value tokens
- `--no-color` - Disable colored output

Drift warnings trigger when:
- Row count changes >50% between periods
- Missing rate changes >10 percentage points
- Numeric mean changes >20%

### Profile Command

Analyze a CSV file and display statistics for each column:

```bash
csvops profile data.csv
```

Options:
- `--json` - Output as JSON
- `--delimiter <char>` - Specify delimiter (auto-detected by default)
- `--no-header` - Treat first row as data, not header
- `--missing <tokens>` - Custom missing value tokens (comma-separated)
- `--sample-size <n>` - Sample size for statistics (default: 10000)
- `--no-color` - Disable colored output

Example:

```bash
csvops profile sales.csv --json
csvops profile data.tsv --delimiter $'\t'
csvops profile data.csv --missing "NA,N/A,MISSING"
```

## Features

### Drift Detection

- **Time bucketing**: Group data by day, week (ISO), or month
- **Metrics tracking**: Row counts, missing rates, numeric means per bucket
- **Automatic alerts**: Warnings when metrics deviate significantly between periods
- **Multiple formats**: Supports ISO 8601, US dates, and Unix timestamps

### Profiling

- **Auto-detection**: Automatically detects delimiter (comma, tab, pipe, semicolon)
- **Type inference**: Detects integer, float, boolean, datetime, and string types
- **Missing values**: Recognizes NA, N/A, NULL, None, -, NaN (configurable)
- **Statistics**: Mean, std dev, min, max, percentiles for numeric columns
- **Cardinality**: Exact count for ≤10,000 distinct values, HyperLogLog estimation for higher
- **Top values**: Tracks most frequent values using Space-Saving algorithm
- **Heuristics**: Detects ID columns, outliers, constant columns, mixed types

### Output

Terminal output with colored warnings:
- 🔴 **Critical**: All values missing, empty file
- 🟡 **Warning**: High missing rate, mixed types, outliers detected
- 🔵 **Info**: ID column detected, constant column, low/high cardinality

JSON output for programmatic use.

## Example Output

```
File Statistics
──────────────────────────────────────────────────
  File:       sales.csv
  Size:       1.25 MB
  Rows:       50,000
  Columns:    8
  Delimiter:  ','
  Header:     yes

Warnings
──────────────────────────────────────────────────
  WARNING [revenue] 5 potential outliers detected (>10×IQR from p1/p99)
  INFO [customer_id] Appears to be an identifier column
  INFO [status] Low cardinality (3 distinct): may be categorical

Column Details
────────────────────────────────────────────────────────────────────────────────
  revenue
    Type:         float
    Missing:      120 (0.2%)
    Cardinality:  48,532
    Mean:         1,234.56
    Std Dev:      567.89
    Min:          0.01
    Max:          99,999.99
```

## Supported Date Formats

- ISO 8601: `2024-01-15`, `2024-01-15T10:30:00`, `2024-01-15T10:30:00Z`
- US format: `01/15/2024`, `1/15/24`
- Unix epoch: `1705334400` (seconds), `1705334400000` (milliseconds)

## Performance

- Streaming processing - handles large files with constant memory
- Reservoir sampling for percentile estimation
- HyperLogLog for high-cardinality columns
- Single-pass statistics via Welford's algorithm

## License

MIT
