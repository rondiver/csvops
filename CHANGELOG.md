# Changes

## 0.1.1 — unreleased

Prepared as a first maintained release of the existing CSV profiler and drift CLI.

- Count and report rows with inconsistent field counts; exclude them from statistics.
- Validate UTF-8 while reading the complete file. Propagate encoding and I/O errors, including invalid headers, instead of producing a successful partial report.
- Handle Unicode text and truncated timestamps without panicking.
- Exit cleanly when a downstream process closes the output pipe early.
- Reject blank or duplicate headers; support generated names and the first data row in headerless drift analysis.
- Normalize offset-bearing timestamps to UTC and reject invalid date suffixes. Report skipped timestamps and insufficient comparison data.
- Give drift warnings distinct JSON codes. Flag mean changes from zero.
- Use deterministic reservoir sampling, frequent-value tie handling, and drift JSON key ordering.
- Flag non-finite numeric values excluded from calculations; reject aggregate overflow instead of serializing invalid statistics.
- Validate delimiter and sample-size arguments. Support single-column files through automatic delimiter fallback.
- Add synthetic job data, generated example reports, CLI regression tests, installation instructions, package metadata, an MIT license, and a tracked dependency lockfile.
- Rework the README around a visual example, reproducible commands, JSON recipes, and a separate command reference.
- Configure formatting, strict lint, tests, and source-package verification on Linux, macOS, and Windows.

Behavior changes to review when updating: statistics now exclude rows with inconsistent field counts; duplicate/blank headers fail; drift uses UTC for offset timestamps; extra row-accounting and numeric fields appear in JSON; drift warnings have dedicated codes. Existing output order and percentile estimates may change once as deterministic behavior is introduced.
