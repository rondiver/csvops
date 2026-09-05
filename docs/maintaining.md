# Maintaining csvops

Keep changes focused on inspecting one local CSV file or comparing its time buckets. Start with a reproducible data problem and a fixture whose expected result can be worked out independently.

## Source map

- `src/main.rs`: command execution and report construction.
- `src/cli.rs`: arguments and validation.
- `src/reader.rs`, `src/delimiter.rs`: streaming input, header handling, and row accounting.
- `src/types.rs`, `src/missing.rs`, `src/stats.rs`, `src/cardinality.rs`, `src/sampling.rs`: per-column measurements and bounded sketches.
- `src/heuristics.rs`, `src/warnings.rs`: diagnostic rules.
- `src/bucket.rs`, `src/drift.rs`: date parsing, bucket accumulation, comparisons, and drift output.
- `src/output/`: terminal and JSON profiles.
- `tests/cli.rs`: tests that invoke the built binary on synthetic data.

The [README](../README.md) introduces the tool; the [command reference](reference.md) describes supported behavior and limits. `PRD.md` and `claude.md` contain earlier planning material, including proposed layouts, thresholds, and performance targets that do not all match the implementation. Treat those as historical design proposals when evaluating a change.

## Local checks

CI uses Rust 1.94.1 and the committed Cargo lockfile. Run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Use `cargo fmt --all` when changing Rust source. Keep meaningful regressions at the CLI boundary for input handling, output contracts, and failures. Unit tests cover algorithms and heuristics. Add property tests or benchmarks when a concrete gap warrants them; a passing unit suite alone does not establish performance.

After committing a release candidate, verify the source package and an isolated installation:

```sh
cargo package --locked --target-dir target/package-check
cargo install --path . --locked --root /tmp/csvops-install-check
/tmp/csvops-install-check/bin/csvops profile examples/jobs.csv --json
/tmp/csvops-install-check/bin/csvops drift examples/jobs.csv --time-col created_at --grain week
```

The separate package target directory keeps package verification from replacing the source checkout's build outputs. The isolated-install example uses a Unix temporary path. On Windows choose a temporary directory and run its `bin/csvops.exe`.

## Release preparation

1. Resolve the intended scope and describe user-visible changes in `CHANGELOG.md`. Update the version and lockfile together.
2. Run the checks above. Inspect generated example reports and review the source-package file list with `cargo package --list --locked`.
3. Keep synthetic fixtures free of customer data. Avoid including local planning documents, build outputs, or workstation configuration in a release archive.
4. Record which platforms were actually tested. CI configuration alone is not a passing cross-platform result.
5. When publication is authorized, push the reviewed branch, obtain passing hosted checks, merge into the default branch, and create the intended tag/release. Apply the reviewed issue updates after the corresponding changes reach that branch.

No workflow publishes packages or creates commits automatically. Binary distribution can follow verified builds on the target platforms; do not label a Linux executable as portable to macOS or Windows.

[Cargo's lockfile guidance](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html) explains the reproducible dependency selection. The CI checkout action is pinned to the verified v7 commit of [actions/checkout](https://github.com/actions/checkout).
