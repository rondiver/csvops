# csvops

Find missing fields, mixed types, and changes over time in a CSV export.

`csvops` is a small Rust command-line tool for inspecting operational data locally. It reads a file, prints a report, and leaves the source alone. Use it for a first look at an unfamiliar job export, sales file, or incoming dataset.

## Try it

You need Git and [Rust](https://www.rust-lang.org/tools/install) 1.94 or newer. CI uses Rust 1.94.1. Installation downloads build dependencies; the installed tool runs offline.

```sh
git clone https://github.com/rondiver/csvops.git
cd csvops
cargo install --path . --locked

csvops profile examples/jobs.csv
csvops drift examples/jobs.csv --time-col created_at --grain week
```

The example contains **12 invented job records**. In the second week, quantities start going missing even though the export still looks structurally valid:

```text
Drift Warnings
────────────────────────────────────────────────────────────
  WARNING Row count changed 100% between 2026-W35 and 2026-W36 (4 -> 8)
  WARNING [quantity] Missing rate changed from 0.0% to 75.0% between 2026-W35 and 2026-W36
  WARNING [quoted_total] Mean changed 100.0% between 2026-W35 and 2026-W36 (100.00 -> 200.00)
```

[Sample data and complete reports](examples/README.md). These are inspection clues: the file alone cannot explain why quantities are missing or whether the change in quoted totals is desirable.

To try it without installing a command:

```sh
cargo run --locked -- profile examples/jobs.csv
cargo run --locked -- drift examples/jobs.csv --time-col created_at --grain week --json
```

## Profile a file

```sh
csvops profile export.csv
csvops profile export.csv --json > profile.json
```

The profile reports accepted row counts, missing values, inferred types, distinct values, frequent values, numeric summaries, and heuristics for identifiers, constant columns, mixed types, and outliers. JSON includes warning codes and identifies approximate cardinality. Numeric summaries include the number of numeric values and the percentile sample size.

| Option | Behavior |
| --- | --- |
| `--json` | Write JSON to stdout. Fatal errors go to stderr. |
| `--delimiter ';'` | Override delimiter detection with one ASCII separator. |
| `--no-header` | Keep the first row as data; name columns `column_0`, `column_1`, and so on. |
| `--missing 'NA,NULL'` | Replace the default missing-value tokens. Matching ignores case and surrounding whitespace. Include an empty token to count blank cells. |
| `--sample-size 10000` | Set the positive number of numeric values retained per column for percentiles. |
| `--no-color` | Disable ANSI colors. `NO_COLOR` is also respected. |

Default missing tokens are blank cells, `NA`, `N/A`, `NULL`, `None`, `-`, and `NaN`, ignoring case and surrounding whitespace. First-row headers must be nonempty and unique. Input values are preserved; type inference does not rewrite the file.

## Compare time periods

```sh
csvops drift export.csv --time-col created_at --grain week
csvops drift export.csv --time-col created_at --grain month --json > drift.json
csvops drift headerless.csv --no-header --time-col column_0 --grain day
```

`--time-col` selects the timestamp column. `--grain` accepts `day` (the default), `week` (ISO week), or `month`. Drift also accepts `--delimiter`, `--no-header`, `--missing`, `--json`, and `--no-color`.

Each pair of consecutive **populated** buckets is compared using these rules:

| Signal | Warning threshold |
| --- | --- |
| Row count | More than 50% change from the prior bucket. |
| Missing-value rate | More than 10 percentage points of change. |
| Numeric mean | More than 20% change relative to the absolute prior mean. A change from zero to a nonzero mean is also flagged. |

Missing calendar periods are not filled with zero-row buckets. With fewer than two populated buckets, the report warns that there is insufficient data to compare. Means describe the values that parse as numbers; changes in type or which values are missing can affect them. Categorical drift and statistical significance tests are not implemented.

Dates accepted by drift include `YYYY-MM-DD`, full ISO timestamps with seconds and optional fractional seconds, RFC 3339 offsets, `YYYY-MM-DD HH:MM:SS`, `MM/DD/YYYY`, and Unix seconds or 13-digit milliseconds. Two-digit US years use 1951–2050. Offset-bearing timestamps are converted to UTC before bucketing; timestamps without an offset are treated as UTC. Prefer ISO dates when exchanging data.

## Input errors and limits

- UTF-8 files are supported, including a UTF-8 BOM, quoted separators, and quoted newlines. Delimiter detection samples the first 8 KiB and considers comma, tab, pipe, and semicolon; use `--delimiter` when the guess is wrong. A file with no detected separator is treated as one column.
- Rows with inconsistent field counts are skipped and counted in the report. Up to five diagnostic examples identify affected lines. Statistics cover accepted rows. Encoding and I/O errors fail the command rather than returning a partial report.
- Drift counts missing or invalid timestamps separately from malformed rows. If no valid timestamps remain, it exits with an error.
- Type labels and ID/outlier warnings are heuristics. For example, numeric identifiers and `0`/`1` values can be ambiguous. Profile date labels are pattern-based; drift validates dates. Currency symbols and thousands separators are not stripped automatically.
- Numeric calculations use floating point. Non-finite values are excluded from numeric statistics and flagged; aggregate overflow is an error. This is not exact decimal arithmetic.
- Distinct counts are exact through 10,000 distinct nonmissing values, then estimated with HyperLogLog. Frequent-value counts use a bounded Space-Saving sketch and may overestimate after eviction. Percentiles and outlier counts use a reservoir sample; outlier counts describe that sample, not the whole file.
- Profiling streams rows and keeps bounded counts of values per column, but memory also depends on field lengths, column count, and sample size. Drift retains aggregates for each populated bucket. There is no fixed memory or runtime guarantee.

Repeated runs with the same input order, options, and tool version use deterministic sampling and ordering.

Exit codes: **0** means analysis completed, including reports with warnings; **1** means an input or runtime error; **2** means invalid command-line arguments. A successful exit does not mean the data is clean. This is an early CLI; JSON may evolve in future versions.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

The tests include CLI regressions for row accounting, Unicode and encoding, date parsing, time zones, reproducibility, and numeric failures. CI is configured to run on Linux, macOS, and Windows. [Maintaining and releasing](docs/maintaining.md) · [Issue review](docs/issue-review.md) · [Changes](CHANGELOG.md).

## License

[MIT](LICENSE). Built by [Ron Diver](https://rondiver.com/).
