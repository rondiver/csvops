\# Product Requirements Document



\*\*Project:\*\* CSV Ops

\*\*Working Name:\*\* csvops

\*\*Type:\*\* Local CLI Tool

\*\*Language:\*\* Rust

\*\*Version:\*\* 1.0

\*\*Last Updated:\*\* January 2026



---



\## 1. One-Line Description



CSV Ops is a command-line tool that profiles messy CSV files and detects temporal drift, answering operator-relevant questions quickly without spreadsheets, notebooks, or prior schema knowledge.



---



\## 2. Problem Statement



CSV files remain the default interchange format for operational data. In production systems, CSVs are large, inconsistent, poorly documented, and time-variant.



Operators and senior engineers frequently need fast answers to high-impact questions:



\- Which columns are mostly empty?

\- Which columns explode in cardinality?

\- Which columns look like IDs vs attributes?

\- Are types stable or mixed?

\- Are values drifting over time?

\- Where does the data look suspicious or broken?



Current solutions (Excel, ad-hoc scripts, guesswork) are slow, error-prone, and do not scale.



CSV Ops answers these questions locally, deterministically, and fast using a single CLI command.



---



\## 3. Target Users



\*\*Primary Users:\*\*

\- Operators managing production data pipelines

\- Senior engineers debugging data issues

\- Technical leads reviewing data quality

\- Analysts working close to production systems



\*\*Secondary Users:\*\*

\- Anyone inheriting unfamiliar datasets

\- Teams doing pre-ingestion sanity checks



\*\*Explicitly Not Targeted:\*\*

\- Data scientists doing modeling

\- BI users needing dashboards

\- ETL orchestration systems

\- Schema enforcement or governance tools



---



\## 4. Design Principles



| Principle | Description |

|-----------|-------------|

| Operator-first | Only answer questions operators actually ask |

| Streaming | Handle large files without loading into memory |

| Opinionated | Minimal flags, sensible defaults |

| Readable output | Human-readable terminal output by default |

| Local and offline | No cloud, no uploads |

| Fast | Sub-second on small files, predictable on large files |

| Deterministic | Same input produces same output |



---



\## 5. Core Use Cases



\*\*UC-1: First look at an unknown CSV\*\*

```

csvops profile data.csv

```

User wants to understand structure, health, and risks in one pass.



\*\*UC-2: Detect time-based drift\*\*

```

csvops drift data.csv --time-col created\\\_at --grain week

```

User wants to detect silent upstream changes over time.



\*\*UC-3: Machine-readable output\*\*

```

csvops profile data.csv --json

```

User wants to pipe results into other tools.



---



\## 6. Input Specification



\### File Requirements

\- One CSV file per command invocation

\- UTF-8 encoding required (error on other encodings with clear message)

\- Configurable delimiter: comma (default), tab, pipe, semicolon

\- Optional header row (auto-detected, overridable)

\- Maximum tested file size: 10GB



\### Delimiter Detection

When `--delimiter auto` (default):

1\. Read first 8KB of file

2\. Count occurrences of: `,` `\\\\t` `|` `;`

3\. Select delimiter with highest consistent count across lines

4\. Fall back to comma if ambiguous



\### Assumptions

\- CSVs may be malformed (unquoted fields with delimiters, inconsistent column counts)

\- Schemas may drift mid-file

\- Types may change row to row

\- Missing values may be implicit or explicit



\### Error Handling

| Condition | Behavior |

|-----------|----------|

| File not found | Exit 1, message: "Error: File not found: {path}" |

| Empty file | Exit 0, message: "Empty file: no data to profile" |

| Encoding error | Exit 1, message: "Error: Non-UTF-8 encoding detected. Convert to UTF-8 first." |

| Malformed row | Warn and continue: "Warning: Row {n} has {x} columns, expected {y}. Skipping." |

| >1% malformed rows | Summary warning at end: "Warning: {n} rows ({p}%) were malformed and skipped." |



---



\## 7. Sampling Strategy



CSV Ops uses streaming with bounded memory for large files.



\### Memory Budget

\- Target: <500MB RAM for any file size

\- Per-column overhead: ~10KB for statistics + cardinality sketch



\### Cardinality Estimation

\- Exact count for columns with ≤10,000 distinct values

\- HyperLogLog sketch (precision 14, ~16KB per column) for higher cardinality

\- Output indicates when estimate is used: "~1.2M distinct (estimated)"



\### Numeric Statistics

\- Streaming calculation using Welford's algorithm for mean/variance

\- Reservoir sampling (size 10,000) for percentile estimation

\- Min/max tracked exactly



\### Top-K Tracking

\- Space-Saving algorithm with k=1000 candidates

\- Report top 5 by default

\- Guaranteed accurate for values appearing in >0.1% of rows



---



\## 8. Output Specification



\### Default Output: Human-Readable Terminal



Structured sections, sorted by significance. Color-coded warnings (red for critical, yellow for notable).



\### JSON Output



Enabled with `--json` flag. Complete metrics in structured format for programmatic consumption.



\### Output Rules

\- No charts or visualizations

\- No interactive UI

\- No narrative explanations

\- No AI summarization

\- Deterministic ordering (alphabetical within priority tiers)



---



\## 9. Profile Command Specification



\### 9.1 Missingness Analysis



For each column, report:



| Metric | Description |

|--------|-------------|

| total\_values | Number of rows |

| missing\_count | Count of missing values |

| missing\_pct | Percentage missing (1 decimal) |



\*\*Missing Value Tokens (case-insensitive):\*\*

\- Empty string

\- `NA`, `N/A`, `na`, `n/a`

\- `NULL`, `null`, `Null`

\- `None`, `none`, `NONE`

\- `-` (single dash)

\- `NaN`, `nan`



\### 9.2 Cardinality Analysis



For each column, report:



| Metric | Description |

|--------|-------------|

| distinct\_count | Number of unique values (or estimate) |

| uniqueness\_ratio | distinct\_count / non\_missing\_count |

| is\_estimated | Boolean, true if HyperLogLog used |



\*\*ID Field Heuristics:\*\*



A column is flagged as "likely ID" if ALL of the following are true:

\- Uniqueness ratio ≥ 0.90

\- No missing values OR missing < 1%

\- At least one of:

  - Matches UUID pattern: `^\\\[0-9a-f]{8}-\\\[0-9a-f]{4}-\\\[0-9a-f]{4}-\\\[0-9a-f]{4}-\\\[0-9a-f]{12}$`

  - Matches integer sequence: all values are integers with low variance in length

  - String length standard deviation < 2 characters

  - Column name contains: `id`, `\\\_id`, `Id`, `ID`, `key`, `uuid`, `guid`



\### 9.3 Type Inference



For each column, report distribution of observed types:



| Type | Detection Rule |

|------|----------------|

| integer | Matches `^-?\\\[0-9]+$` |

| float | Matches `^-?\\\[0-9]\\\*\\\\.\\\[0-9]+$` or scientific notation |

| boolean | Matches `true`, `false`, `1`, `0`, `yes`, `no`, `t`, `f` (case-insensitive) |

| datetime | Parseable by ISO 8601, RFC 2822, or common formats (see below) |

| string | Everything else |



\*\*Datetime Formats Recognized:\*\*

\- ISO 8601: `2024-01-15`, `2024-01-15T10:30:00`, `2024-01-15T10:30:00Z`

\- US format: `01/15/2024`, `1/15/24`

\- European format: `15-01-2024`, `15/01/2024`

\- Timestamp: Unix epoch (integer >946684800 and <2147483647)



\*\*Mixed-Type Detection:\*\*



A column is flagged as "mixed type" if:

\- More than one type represents >5% of non-missing values

\- Report: "Mixed types: 82% integer, 15% string, 3% float"



\### 9.4 Value Distributions



\*\*Numeric Columns (integer or float >90% of values):\*\*



| Metric | Description |

|--------|-------------|

| min | Minimum value |

| max | Maximum value |

| mean | Arithmetic mean |

| p50 | Median (from reservoir sample) |

| p99 | 99th percentile (from reservoir sample) |



\*\*Outlier Detection:\*\*



A value is flagged as an outlier if:

\- Value > p99 + (10 × IQR) OR value < p01 - (10 × IQR)

\- Where IQR = p75 - p25



Report: "Outliers detected: {n} values exceed 10×IQR beyond p99"



\*\*Categorical Columns (string or boolean, <1000 distinct values):\*\*



| Metric | Description |

|--------|-------------|

| top\_values | Top 5 most frequent values with counts and percentages |

| coverage | Percentage of rows covered by top 5 |



\### 9.5 Data Validity Warnings



Warnings are concrete, short, and actionable.



| Condition | Warning Text |

|-----------|--------------|

| >50% missing | "⚠ {col}: {pct}% missing values" |

| >95% missing | "🔴 {col}: {pct}% missing - consider dropping" |

| Mixed types | "⚠ {col}: Mixed types ({breakdown})" |

| Likely ID | "ℹ {col}: Likely ID field (uniqueness: {pct}%)" |

| High cardinality | "ℹ {col}: High cardinality ({n} distinct)" |

| Outliers | "⚠ {col}: {n} outliers detected (>{threshold})" |

| Constant | "ℹ {col}: Constant value ({value})" |

| Date parse failures | "⚠ {col}: {n} unparseable dates in datetime column" |



---



\## 10. Drift Command Specification



\### Purpose

Detect silent upstream system changes by analyzing how data characteristics change over time.



\### Requirements

\- Requires `--time-col` flag specifying a datetime column

\- Time column must be parseable as datetime (see 9.3)



\### Grain Options

| Option | Grouping |

|--------|----------|

| day | Calendar day |

| week | ISO week (Monday start) |

| month | Calendar month |



Default: `week`



\### Metrics Tracked Per Time Bucket



| Metric | Description |

|--------|-------------|

| row\_count | Number of rows in bucket |

| missing\_pct | Per-column missing percentage |

| numeric\_mean | Per-numeric-column mean |

| numeric\_range | Per-numeric-column \[min, max] |

| top\_category | Most frequent value per categorical column |

| top\_category\_pct | Percentage of most frequent value |



\### Drift Detection Rules



A column is flagged for drift if comparing most recent bucket to historical baseline (all prior buckets):



| Condition | Threshold | Warning |

|-----------|-----------|---------|

| Missing rate change | >10 percentage points | "Missing rate changed: {old}% → {new}%" |

| Numeric mean shift | >2 standard deviations | "Mean shifted: {old} → {new} (>{n}σ)" |

| Range expansion | New min < historical min OR new max > historical max | "Range expanded: \[{old\_min}, {old\_max}] → \[{new\_min}, {new\_max}]" |

| Category dominance change | Top category changed | "Top category changed: {old} → {new}" |

| Row count anomaly | >50% change from prior bucket | "Row count anomaly: {old} → {new} ({pct}% change)" |



---



\## 11. CLI Interface



\### Commands



```

csvops profile <file>       Profile a CSV file

csvops drift <file>         Detect temporal drift in a CSV file

csvops --version            Show version

csvops --help               Show help

```



\### Profile Flags



| Flag | Default | Description |

|------|---------|-------------|

| --json | false | Output JSON instead of terminal format |

| --delimiter | auto | Field delimiter: auto, comma, tab, pipe, semicolon |

| --no-header | false | First row is data, not headers |

| --missing | (built-in) | Additional missing value tokens (comma-separated) |



\### Drift Flags



| Flag | Default | Description |

|------|---------|-------------|

| --time-col | (required) | Column name containing timestamps |

| --grain | week | Time bucket: day, week, month |

| --json | false | Output JSON instead of terminal format |

| --delimiter | auto | Field delimiter |



\### Exit Codes



| Code | Meaning |

|------|---------|

| 0 | Success |

| 1 | Error (file not found, parse error, invalid arguments) |

| 2 | Warnings present but completed successfully |



---



\## 12. Example Output



\### Terminal Output (csvops profile orders.csv)



```

CSV Ops v1.0 — orders.csv

══════════════════════════════════════════════════════════════



File Statistics

───────────────────────────────────────────────────────────────

Rows:           1,247,832

Columns:        14

File size:      287 MB

Parse time:     4.2s

Malformed rows: 23 (0.002%)



Warnings

───────────────────────────────────────────────────────────────

🔴 legacy\\\_code: 98.2% missing - consider dropping

⚠  shipping\\\_cost: Mixed types (94% float, 6% string)

⚠  order\\\_total: 847 outliers detected (>$52,340)

⚠  customer\\\_notes: 67.3% missing values

ℹ  order\\\_id: Likely ID field (uniqueness: 100.0%)

ℹ  customer\\\_id: Likely ID field (uniqueness: 94.2%)



Column Details

───────────────────────────────────────────────────────────────



order\\\_id

\&nbsp; Type:        string (100.0%)

\&nbsp; Missing:     0 (0.0%)

\&nbsp; Distinct:    1,247,832 (exact)

\&nbsp; Uniqueness:  100.0%

\&nbsp; Likely ID:   yes (UUID pattern)



customer\\\_id

\&nbsp; Type:        integer (100.0%)

\&nbsp; Missing:     0 (0.0%)

\&nbsp; Distinct:    1,172,445 (exact)

\&nbsp; Uniqueness:  94.2%

\&nbsp; Likely ID:   yes (high uniqueness + naming)



order\\\_date

\&nbsp; Type:        datetime (100.0%)

\&nbsp; Missing:     0 (0.0%)

\&nbsp; Range:       2023-01-01 to 2024-12-31



status

\&nbsp; Type:        string (100.0%)

\&nbsp; Missing:     0 (0.0%)

\&nbsp; Distinct:    5 (exact)

\&nbsp; Top values:

\&nbsp;   completed    847,234  (67.9%)

\&nbsp;   shipped      287,492  (23.0%)

\&nbsp;   pending       78,453  (6.3%)

\&nbsp;   cancelled     31,298  (2.5%)

\&nbsp;   refunded       3,355  (0.3%)



order\\\_total

\&nbsp; Type:        float (100.0%)

\&nbsp; Missing:     0 (0.0%)

\&nbsp; Min:         0.01

\&nbsp; Max:         847,293.42

\&nbsp; Mean:        127.84

\&nbsp; Median:      89.50

\&nbsp; P99:         523.40

\&nbsp; Outliers:    847 values > $52,340 (10×IQR)



shipping\\\_cost

\&nbsp; Type:        mixed

\&nbsp; Missing:     12,847 (1.0%)

\&nbsp; Breakdown:   float 94.1%, string 5.9%

\&nbsp; String samples: "FREE", "TBD", "INCLUDED"



legacy\\\_code

\&nbsp; Type:        string (100.0%)

\&nbsp; Missing:     1,225,732 (98.2%)

\&nbsp; Distinct:    142 (exact)



\\\[... remaining columns ...]

```



\### JSON Output Structure



```json

{

\&nbsp; "file": "orders.csv",

\&nbsp; "version": "1.0",

\&nbsp; "generated\\\_at": "2026-01-15T10:30:00Z",

\&nbsp; "summary": {

\&nbsp;   "rows": 1247832,

\&nbsp;   "columns": 14,

\&nbsp;   "file\\\_bytes": 300941312,

\&nbsp;   "parse\\\_seconds": 4.2,

\&nbsp;   "malformed\\\_rows": 23

\&nbsp; },

\&nbsp; "warnings": \\\[

\&nbsp;   {

\&nbsp;     "severity": "critical",

\&nbsp;     "column": "legacy\\\_code",

\&nbsp;     "code": "HIGH\\\_MISSING",

\&nbsp;     "message": "98.2% missing - consider dropping"

\&nbsp;   }

\&nbsp; ],

\&nbsp; "columns": {

\&nbsp;   "order\\\_id": {

\&nbsp;     "position": 0,

\&nbsp;     "types": {"string": 1.0},

\&nbsp;     "missing": {"count": 0, "pct": 0.0},

\&nbsp;     "cardinality": {

\&nbsp;       "distinct": 1247832,

\&nbsp;       "is\\\_estimated": false,

\&nbsp;       "uniqueness": 1.0

\&nbsp;     },

\&nbsp;     "likely\\\_id": true,

\&nbsp;     "id\\\_reason": "UUID pattern match"

\&nbsp;   }

\&nbsp; }

}

```



---



\## 13. Non-Goals (Strict)



CSV Ops is \*\*not\*\*:

\- A database loader

\- A schema validator or enforcer

\- A data quality enforcement system

\- A visualization tool

\- An AI explainer or summarizer

\- A replacement for pandas, DuckDB, or Spark

\- A multi-file comparison tool

\- A data transformation tool



---



\## 14. Technical Constraints



| Constraint | Specification |

|------------|---------------|

| Memory | <500MB for any file size |

| Streaming | Single-pass where possible |

| Dependencies | Minimal, vendored where possible |

| Configuration | No config files, CLI flags only |

| Output | Deterministic (same input = same output) |

| Platform | Linux, macOS, Windows |



---



\## 15. Success Criteria



CSV Ops succeeds if:

\- A senior engineer runs it once and understands the dataset

\- It surfaces at least one non-obvious issue in a real CSV

\- It replaces Excel for first inspection tasks

\- It completes in <10 seconds for files under 1GB



CSV Ops fails if:

\- Output feels generic or verbose

\- Configuration becomes necessary to get value

\- Memory usage scales linearly with file size

\- False positive warnings exceed 20% of total warnings



---



\## 16. Future Considerations (Not v1)



The following are explicitly out of scope for v1 but may be considered later:

\- Multi-file comparison and schema diff

\- Gzip/compression support

\- Plugin system for custom heuristics

\- Configuration file support

\- Parquet/JSON input formats

\- Sampling mode for extremely large files (>10GB)

\- Column correlation detection



---



\## Appendix A: Heuristic Definitions



\### A.1 ID Field Detection



```

is\\\_likely\\\_id(column) =

\&nbsp; uniqueness\\\_ratio >= 0.90 AND

\&nbsp; missing\\\_pct < 0.01 AND

\&nbsp; (

\&nbsp;   matches\\\_uuid\\\_pattern OR

\&nbsp;   matches\\\_integer\\\_sequence OR

\&nbsp;   string\\\_length\\\_stddev < 2 OR

\&nbsp;   name\\\_contains\\\_id\\\_keyword

\&nbsp; )

```



\### A.2 Outlier Detection



```

is\\\_outlier(value, column) =

\&nbsp; value > p99 + (10 × IQR) OR

\&nbsp; value < p01 - (10 × IQR)



where IQR = p75 - p25

```



\### A.3 Mixed Type Threshold



```

is\\\_mixed\\\_type(column) =

\&nbsp; count(types where pct > 0.05) > 1

```



\### A.4 Drift Detection Thresholds



| Metric | Threshold |

|--------|-----------|

| Missing rate | ±10 percentage points |

| Numeric mean | ±2 standard deviations |

| Row count | ±50% from prior bucket |

