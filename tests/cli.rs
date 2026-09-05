use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn run_file(command: &str, path: &Path, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_csvops"))
        .arg(command)
        .arg(path)
        .args(flags)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

fn run(command: &str, input: &[u8], flags: &[&str]) -> Output {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("input.csv");
    fs::write(&path, input).unwrap();
    run_file(command, &path, flags)
}

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn sample_jobs_profile_has_known_missing_quantities() {
    let output = run(
        "profile",
        include_bytes!("../examples/jobs.csv"),
        &["--json"],
    );
    let result = json(&output);
    assert_eq!(result["file"]["row_count"], 12);
    let quantity = result["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|column| column["name"] == "quantity")
        .unwrap();
    assert_eq!(quantity["missing_count"], 6);
    assert_eq!(quantity["missing_percentage"], 50.0);
    assert_eq!(quantity["numeric_stats"]["mean"], 1000.0);
}

#[test]
fn sample_jobs_drift_reports_the_known_changes() {
    let result = json(&run(
        "drift",
        include_bytes!("../examples/jobs.csv"),
        &["--time-col", "created_at", "--grain", "week", "--json"],
    ));
    assert_eq!(result["analyzed_rows"], 12);
    assert_eq!(result["buckets"][0]["row_count"], 4);
    assert_eq!(result["buckets"][1]["row_count"], 8);
    assert_eq!(result["buckets"][1]["missing_rates"]["quantity"], 0.75);
    assert!(result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |warning| warning["code"] == "MISSING_RATE_CHANGED" && warning["column"] == "quantity"
        ));
}

#[test]
fn malformed_widths_are_reported_and_excluded_from_statistics() {
    let result = json(&run(
        "profile",
        b"name,quantity,total\ngood,10,20\nshort,10\nlong,10,20,30\ngood,20,40\n",
        &["--json"],
    ));
    assert_eq!(result["file"]["row_count"], 2);
    assert_eq!(result["file"]["skipped_rows"], 2);
    assert!(result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "MALFORMED_ROWS"));
}

#[test]
fn unicode_values_render_without_panicking() {
    let input = format!(
        "service,quantity\n{},10\n{},20\n",
        "綴".repeat(15),
        "綴".repeat(15)
    );
    let output = run("profile", input.as_bytes(), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.contains(&0x1b));
}

#[test]
fn utf8_character_at_sample_boundary_is_valid() {
    let input = format!(
        "notes,value\n{}é,10\n",
        "a".repeat(65535 - "notes,value\n".len())
    );
    let result = json(&run("profile", input.as_bytes(), &["--json"]));
    assert_eq!(result["file"]["row_count"], 1);
}

#[test]
fn invalid_utf8_after_the_initial_sample_is_an_error() {
    let mut input = b"name,value\n".to_vec();
    input.extend_from_slice("ok,10\n".repeat(12000).as_bytes());
    input.extend_from_slice(b"bad,\xff\n");
    let output = run("profile", &input, &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
}

#[test]
fn invalid_header_encoding_is_an_error() {
    let output = run("profile", b"name,\xff\nok,10\n", &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn truncated_datetime_fails_cleanly_instead_of_panicking() {
    let output = run(
        "drift",
        b"date,value\n2026-09-04T,10\n",
        &["--time-col", "date", "--json"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("No rows with a valid timestamp"));
}

#[test]
fn drift_reports_invalid_dates_and_normalizes_offsets_to_utc() {
    let result = json(&run("drift", b"date,value\n2026-09-04T23:30:00-07:00,10\n2026-09-05T06:30:00Z,20\n2026-09-04garbage,30\n2026-09-04T,40\n",
        &["--time-col", "date", "--json"]));
    assert_eq!(result["analyzed_rows"], 2);
    assert_eq!(result["skipped_timestamp_rows"], 2);
    assert_eq!(result["bucket_count"], 1);
    assert_eq!(result["buckets"][0]["key"], "2026-09-05");
    assert_eq!(result["buckets"][0]["numeric_means"]["value"], 15.0);
}

#[test]
fn headerless_drift_keeps_first_row_and_uses_generated_column_names() {
    let result = json(&run(
        "drift",
        b"2026-09-03,10\n2026-09-04,20\n",
        &["--no-header", "--time-col", "column_0", "--json"],
    ));
    assert_eq!(result["analyzed_rows"], 2);
    assert_eq!(result["bucket_count"], 2);
}

#[test]
fn duplicate_headers_do_not_silently_overwrite_drift_columns() {
    let output = run(
        "drift",
        b"date,value,value\n2026-09-04,10,20\n",
        &["--time-col", "date", "--json"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Duplicate header"));
}

#[test]
fn invalid_cli_values_return_argument_errors() {
    for flags in [&["--delimiter", "é"][..], &["--sample-size", "0"][..]] {
        let output = run("profile", b"name,value\nok,10\n", flags);
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn profile_output_is_repeatable_with_sampling_and_top_value_eviction() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sample.csv");
    let mut input = String::from("name,amount\n");
    for i in 0..11000 {
        input.push_str(&format!("item-{i:05},{}\n", i + 10));
    }
    fs::write(&path, input).unwrap();
    let first = run_file("profile", &path, &["--json"]);
    let second = run_file("profile", &path, &["--json"]);
    json(&first);
    json(&second);
    assert!(
        first.stdout == second.stdout,
        "Repeated invocations must produce the same JSON"
    );
}

#[test]
fn non_finite_numbers_are_excluded_and_reported() {
    let profile = json(&run("profile", b"value\n10\n1e999\n", &["--json"]));
    assert_eq!(profile["columns"][0]["numeric_stats"]["mean"], 10.0);
    assert_eq!(profile["columns"][0]["non_finite_count"], 1);
    let drift = json(&run(
        "drift",
        b"date,value\n2026-09-04,10\n2026-09-04,1e999\n",
        &["--time-col", "date", "--json"],
    ));
    assert_eq!(drift["buckets"][0]["numeric_means"]["value"], 10.0);
    assert!(drift["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "NON_FINITE_NUMERIC"));
}

#[test]
fn numeric_overflow_is_an_error_instead_of_null_statistics() {
    for (command, flags, input) in [
        ("profile", &["--json"][..], &b"value\n1e308\n-1e308\n"[..]),
        (
            "drift",
            &["--time-col", "date", "--json"][..],
            &b"date,value\n2026-09-04,1e308\n2026-09-04,1e308\n"[..],
        ),
    ] {
        let output = run(command, input, flags);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Numeric overflow"));
    }
}

#[test]
fn drift_flags_a_mean_change_from_zero() {
    let result = json(&run(
        "drift",
        b"date,value\n2026-09-03,0\n2026-09-04,10\n",
        &["--time-col", "date", "--json"],
    ));
    assert!(result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "MEAN_CHANGED" && warning["column"] == "value"));
}

#[test]
fn closing_the_output_pipe_early_does_not_panic() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("wide.csv");
    let headers = (0..200)
        .map(|i| format!("column_{i}"))
        .collect::<Vec<_>>()
        .join(",");
    fs::write(&path, format!("{headers}\n{}\n", vec!["10"; 200].join(","))).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_csvops"))
        .args(["profile", "--json"])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}
