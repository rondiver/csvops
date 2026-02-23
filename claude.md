\# CLAUDE.md



> Project-specific context for AI assistance with CSV Ops development.



\## Project Overview



CSV Ops (`csvops`) is a command-line tool for profiling messy CSV files. It answers operator-relevant questions about data quality, structure, and temporal drift without requiring spreadsheets, notebooks, or prior schema knowledge.



\*\*Language:\*\* Rust  

\*\*Target:\*\* Single binary, cross-platform (Linux, macOS, Windows)  

\*\*Philosophy:\*\* Operator-first, streaming, opinionated, fast



\## Quick Reference



```bash

\# Build

cargo build --release



\# Run tests

cargo test



\# Run with example

cargo run -- profile test\_data/orders.csv

cargo run -- drift test\_data/orders.csv --time-col created\_at --grain week



\# Lint and format

cargo fmt

cargo clippy -- -D warnings

```



\## Architecture



```

src/

├── main.rs              # CLI entry point, argument parsing

├── lib.rs               # Public API surface

├── profile/

│   ├── mod.rs           # Profile command orchestration

│   ├── reader.rs        # Streaming CSV reader with delimiter detection

│   ├── column.rs        # Per-column statistics accumulator

│   ├── types.rs         # Type inference logic

│   ├── cardinality.rs   # Exact + HyperLogLog cardinality

│   ├── numeric.rs       # Streaming numeric stats (Welford's)

│   ├── categorical.rs   # Top-K tracking (Space-Saving)

│   └── warnings.rs      # Warning generation logic

├── drift/

│   ├── mod.rs           # Drift command orchestration

│   ├── bucketing.rs     # Time bucketing logic

│   └── detection.rs     # Drift threshold detection

├── output/

│   ├── mod.rs           # Output formatting

│   ├── terminal.rs      # Human-readable terminal output

│   └── json.rs          # JSON serialization

└── error.rs             # Error types and handling

```



\## Key Design Decisions



\### Streaming Architecture

\- Single-pass processing for profile command

\- Memory budget: <500MB regardless of file size

\- Use `csv` crate with streaming reader

\- Per-column accumulators updated row-by-row



\### Cardinality Estimation

\- Exact counting up to 10,000 distinct values per column

\- Switch to HyperLogLog (precision 14) above threshold

\- Use `hyperloglog` crate or implement from scratch

\- Always indicate in output when estimate is used



\### Type Inference

\- Order of precedence: integer → float → boolean → datetime → string

\- Check patterns in order, first match wins

\- Track distribution across all rows, report percentages

\- Mixed type = more than one type >5% of values



\### Missing Value Detection

Built-in tokens (case-insensitive):

\- Empty string, `NA`, `N/A`, `NULL`, `null`, `None`, `-`, `NaN`



\### Numeric Statistics

\- Welford's online algorithm for mean/variance

\- Reservoir sampling (k=10,000) for percentiles

\- Exact min/max tracking



\### Top-K Categorical

\- Space-Saving algorithm with 1000 candidates

\- Report top 5 in output

\- Guaranteed accuracy for values >0.1% frequency



\## Coding Standards



\### Error Handling

\- Use `thiserror` for error types

\- Return `Result<T, CsvOpsError>` from all fallible functions

\- User-facing errors must be actionable: include file path, line number, suggestion

\- Internal errors can panic with `unreachable!()` for impossible states



\### Testing

\- Unit tests in same file as implementation (`#\[cfg(test)]` module)

\- Integration tests in `tests/` directory

\- Property-based tests with `proptest` for parsing logic

\- Test files in `test\_data/` with known characteristics



\### Performance

\- Profile with `cargo flamegraph` before optimizing

\- Benchmark with `criterion` for hot paths

\- Target: <1 second for 100MB file on commodity hardware

\- No allocations in inner parsing loop



\### Documentation

\- Doc comments on all public items

\- Examples in doc comments that compile (`cargo test --doc`)

\- README.md with installation and usage



\## Dependencies (Preferred)



```toml

\[dependencies]

clap = { version = "4", features = \["derive"] }  # CLI parsing

csv = "1.3"                                       # CSV parsing

serde = { version = "1", features = \["derive"] } # Serialization

serde\_json = "1"                                  # JSON output

chrono = "0.4"                                    # Datetime parsing

thiserror = "1"                                   # Error handling

colored = "2"                                     # Terminal colors



\[dev-dependencies]

criterion = "0.5"                                 # Benchmarks

proptest = "1"                                    # Property testing

tempfile = "3"                                    # Test file handling

```



Minimize dependencies. Prefer standard library where possible. Vendor small algorithms (HyperLogLog, Space-Saving) rather than adding crates for single use.



\## Heuristics Reference



\### ID Field Detection

A column is flagged as "likely ID" if ALL conditions met:

1\. Uniqueness ratio ≥ 0.90

2\. Missing < 1%

3\. At least one of:

&nbsp;  - UUID pattern match

&nbsp;  - Integer sequence with consistent length

&nbsp;  - String length std dev < 2

&nbsp;  - Column name contains: id, \_id, key, uuid, guid



\### Outlier Detection

```

outlier if: value > p99 + 10×IQR  OR  value < p01 - 10×IQR

where IQR = p75 - p25

```



\### Mixed Type Threshold

Mixed if: more than one type represents >5% of non-missing values



\### Drift Detection

| Metric | Threshold |

|--------|-----------|

| Missing rate change | ±10 percentage points |

| Numeric mean shift | ±2 standard deviations |

| Row count change | ±50% from prior bucket |



\## Common Tasks



\### Adding a New Warning Type

1\. Add variant to `WarningKind` enum in `warnings.rs`

2\. Implement detection logic in `ColumnStats::generate\_warnings()`

3\. Add terminal formatting in `terminal.rs`

4\. Add JSON field in `json.rs`

5\. Add test case with CSV that triggers warning



\### Adding a New Type Detection

1\. Add variant to `InferredType` enum in `types.rs`

2\. Add regex/parser in `TypeInference::infer()`

3\. Order matters: place before `string` fallback

4\. Update type distribution tracking in `ColumnStats`

5\. Add test cases for edge cases



\### Modifying Output Format

\- Terminal output: `output/terminal.rs`

\- JSON structure: `output/json.rs` with serde attributes

\- Both must represent same data, formatting differs



\## Known Edge Cases



\### CSV Parsing

\- Quoted fields containing delimiters or newlines

\- BOM at file start (strip UTF-8 BOM)

\- Inconsistent column counts (warn and skip row)

\- Very long fields (>1MB) — truncate with warning



\### Type Inference

\- Numbers with leading zeros (treat as string: zip codes)

\- Dates in ambiguous formats (01/02/03) — prefer ISO 8601

\- Boolean-like but not boolean ("Yes", "No" vs 1/0)

\- Currency symbols ($100.00) — strip and parse as float



\### Datetime Parsing

\- Unix timestamps: only integers in range 946684800–2147483647

\- Timezone handling: parse but normalize to UTC for comparison

\- Partial dates (2024-01) — treat as first of month



\## What Not To Do



\- Don't add interactive prompts

\- Don't add progress bars (breaks piping)

\- Don't add color configuration (respect NO\_COLOR env var)

\- Don't add config files

\- Don't add plugin system

\- Don't load entire file into memory

\- Don't add visualization/charts

\- Don't add AI/ML features

\- Don't add multi-file comparison (future version)



\## Testing Data



Test files should cover:

\- `clean.csv` — Well-formed, single types, no missing

\- `messy.csv` — Mixed types, missing values, malformed rows

\- `large.csv` — 1M+ rows for performance testing

\- `drift.csv` — Time series with known drift points

\- `edge\_cases.csv` — BOM, quoted delimiters, long fields

\- `empty.csv` — Empty file (0 bytes)

\- `header\_only.csv` — Header row, no data



\## Release Checklist



1\. `cargo fmt \&\& cargo clippy -- -D warnings`

2\. `cargo test`

3\. Update version in `Cargo.toml`

4\. Update CHANGELOG.md

5\. `cargo build --release`

6\. Test binary on sample files

7\. Tag release: `git tag -a v1.0.0 -m "Release 1.0.0"`

