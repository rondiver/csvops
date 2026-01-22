# csvops

A fast, local CLI for inspecting CSV files and surfacing structural issues. Designed for operators who need answers immediately — no schemas, notebooks, or setup.

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
# Inspect a CSV file
csvops profile data.csv

# Output as JSON for scripting
csvops profile data.csv --json
```

## What It Does

**One command. Immediate answers.**

- **Delimiter detection** — Reads first 8KB, scores consistency across comma, tab, pipe, semicolon
- **Type inference** — Classifies each column as integer, float, boolean, datetime, or string based on value patterns
- **Missing value detection** — Counts empties, NA, N/A, NULL, None, NaN; reports percentage per column
- **Numeric statistics** — Min, max, mean, standard deviation, percentiles (p25, p50, p75, p99) via streaming algorithms
- **Cardinality estimation** — Exact distinct counts up to 10K; HyperLogLog approximation above that
- **Top values** — Most frequent values per column, useful for spotting unexpected categories or data entry errors
- **ID column detection** — Flags columns that look like identifiers (high uniqueness, UUID patterns, naming conventions)
- **Constant detection** — Flags columns where every row has the same value
- **Mixed-type warnings** — Alerts when a column contains multiple data types (e.g., integers mixed with strings)
- **Outlier flagging** — Surfaces numeric values far outside the normal range

No configuration files. No database connections. No cloud accounts.

## Usage

### Profile Command

```bash
csvops profile data.csv
```

Options:
- `--json` — Output as JSON
- `--delimiter <char>` — Specify delimiter (auto-detected by default)
- `--no-header` — Treat first row as data
- `--missing <tokens>` — Custom missing value tokens (comma-separated)
- `--sample-size <n>` — Sample size for statistics (default: 10000)
- `--no-color` — Disable colored output

### Drift Command

For files with a time column, surface how data changes across time periods:

```bash
csvops drift data.csv --time-col created_at --grain week
```

This groups rows by time bucket and highlights shifts in row counts, missing rates, and numeric distributions.

Options:
- `--time-col <name>` — Column containing timestamps (required)
- `--grain <day|week|month>` — Bucket granularity (default: day)
- `--json` — Output as JSON

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
  WARNING [revenue] Potential outliers detected
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

## Supported Formats

**Delimiters:** comma, tab, pipe, semicolon (auto-detected)

**Date formats:** ISO 8601, US (MM/DD/YYYY), Unix timestamps

**Missing values:** empty, NA, N/A, NULL, None, -, NaN (configurable)

## Performance

- Streaming — handles large files with constant memory
- Reservoir sampling for percentile estimation
- HyperLogLog for high-cardinality columns
- Single-pass statistics

## License

MIT
