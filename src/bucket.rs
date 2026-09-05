use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use std::sync::LazyLock;

use crate::cli::TimeGrain;

// Date parsing patterns
static ISO_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})-(\d{2})-(\d{2})$").unwrap());

static US_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,2})/(\d{1,2})/(\d{2}|\d{4})$").unwrap());

static UNIX_EPOCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(-?\d{1,10}|-?\d{13})$").unwrap());

/// Parses a datetime string into a NaiveDateTime
pub fn parse_datetime(value: &str) -> Option<NaiveDateTime> {
    let trimmed = value.trim();

    // Offset-bearing timestamps use UTC buckets. Never slice an unvalidated
    // date string: truncated timestamps and Unicode suffixes are valid inputs
    // to reject, not reasons to panic.
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.naive_utc());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Some(dt);
        }
    }

    // Try ISO 8601 format first
    if let Some(caps) = ISO_DATE.captures(trimmed) {
        let year: i32 = caps.get(1)?.as_str().parse().ok()?;
        let month: u32 = caps.get(2)?.as_str().parse().ok()?;
        let day: u32 = caps.get(3)?.as_str().parse().ok()?;

        let date = NaiveDate::from_ymd_opt(year, month, day)?;

        return date.and_hms_opt(0, 0, 0);
    }

    // Try US date format (MM/DD/YYYY)
    if let Some(caps) = US_DATE.captures(trimmed) {
        let month: u32 = caps.get(1)?.as_str().parse().ok()?;
        let day: u32 = caps.get(2)?.as_str().parse().ok()?;
        let mut year: i32 = caps.get(3)?.as_str().parse().ok()?;

        // Handle 2-digit years
        if year < 100 {
            year += if year > 50 { 1900 } else { 2000 };
        }

        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        return date.and_hms_opt(0, 0, 0);
    }

    // Try Unix epoch
    if let Some(caps) = UNIX_EPOCH.captures(trimmed) {
        let timestamp: i64 = caps.get(1)?.as_str().parse().ok()?;

        // Determine if seconds or milliseconds
        return if trimmed.trim_start_matches('-').len() == 13 {
            Utc.timestamp_millis_opt(timestamp).single()
        } else {
            Utc.timestamp_opt(timestamp, 0).single()
        }
        .map(|dt| dt.naive_utc());
    }

    None
}

/// Gets the bucket key for a datetime based on the time grain
pub fn get_bucket_key(dt: NaiveDateTime, grain: TimeGrain) -> String {
    match grain {
        TimeGrain::Day => dt.format("%Y-%m-%d").to_string(),
        TimeGrain::Week => {
            // ISO week: YYYY-Www
            let iso_week = dt.iso_week();
            format!("{}-W{:02}", iso_week.year(), iso_week.week())
        }
        TimeGrain::Month => dt.format("%Y-%m").to_string(),
    }
}

/// Bucket for tracking metrics over a time period
#[derive(Debug, Clone, Default)]
pub struct TimeBucket {
    pub key: String,
    pub row_count: usize,
    pub missing_counts: Vec<usize>,
    pub numeric_sums: Vec<f64>,
    pub numeric_counts: Vec<usize>,
}

impl TimeBucket {
    pub fn new(key: String, num_columns: usize) -> Self {
        Self {
            key,
            row_count: 0,
            missing_counts: vec![0; num_columns],
            numeric_sums: vec![0.0; num_columns],
            numeric_counts: vec![0; num_columns],
        }
    }

    pub fn record_missing(&mut self, col_idx: usize) {
        if col_idx < self.missing_counts.len() {
            self.missing_counts[col_idx] += 1;
        }
    }

    pub fn record_numeric(&mut self, col_idx: usize, value: f64) {
        if col_idx < self.numeric_sums.len() {
            self.numeric_sums[col_idx] += value;
            self.numeric_counts[col_idx] += 1;
        }
    }

    pub fn missing_rate(&self, col_idx: usize) -> f64 {
        if self.row_count == 0 {
            return 0.0;
        }
        self.missing_counts.get(col_idx).copied().unwrap_or(0) as f64 / self.row_count as f64
    }

    pub fn numeric_mean(&self, col_idx: usize) -> Option<f64> {
        let count = self.numeric_counts.get(col_idx).copied().unwrap_or(0);
        if count == 0 {
            return None;
        }
        let sum = self.numeric_sums.get(col_idx).copied().unwrap_or(0.0);
        Some(sum / count as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn test_parse_iso_date() {
        let dt = parse_datetime("2024-01-15").unwrap();
        assert_eq!(dt.date().year(), 2024);
        assert_eq!(dt.date().month(), 1);
        assert_eq!(dt.date().day(), 15);
    }

    #[test]
    fn test_parse_iso_datetime() {
        let dt = parse_datetime("2024-01-15T10:30:00").unwrap();
        assert_eq!(dt.date().year(), 2024);
        assert_eq!(dt.time().hour(), 10);
        assert_eq!(dt.time().minute(), 30);
    }

    #[test]
    fn test_parse_iso_datetime_with_tz() {
        let dt = parse_datetime("2024-01-15T10:30:00Z").unwrap();
        assert_eq!(dt.date().year(), 2024);
    }

    #[test]
    fn test_parse_us_date() {
        let dt = parse_datetime("01/15/2024").unwrap();
        assert_eq!(dt.date().year(), 2024);
        assert_eq!(dt.date().month(), 1);
        assert_eq!(dt.date().day(), 15);
    }

    #[test]
    fn test_parse_us_date_short_year() {
        let dt = parse_datetime("01/15/24").unwrap();
        assert_eq!(dt.date().year(), 2024);
    }

    #[test]
    fn test_parse_unix_epoch_seconds() {
        let dt = parse_datetime("1705334400").unwrap();
        // 2024-01-15 12:00:00 UTC
        assert_eq!(dt.date().year(), 2024);
        assert_eq!(dt.date().month(), 1);
    }

    #[test]
    fn test_parse_unix_epoch_millis() {
        let dt = parse_datetime("1705334400000").unwrap();
        assert_eq!(dt.date().year(), 2024);
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_datetime("not a date").is_none());
        assert!(parse_datetime("").is_none());
    }

    #[test]
    fn test_bucket_key_day() {
        let dt = NaiveDate::from_ymd_opt(2024, 1, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(get_bucket_key(dt, TimeGrain::Day), "2024-01-15");
    }

    #[test]
    fn test_bucket_key_week() {
        let dt = NaiveDate::from_ymd_opt(2024, 1, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let key = get_bucket_key(dt, TimeGrain::Week);
        assert!(key.starts_with("2024-W"));
    }

    #[test]
    fn test_bucket_key_month() {
        let dt = NaiveDate::from_ymd_opt(2024, 1, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(get_bucket_key(dt, TimeGrain::Month), "2024-01");
    }

    #[test]
    fn test_time_bucket_missing_rate() {
        let mut bucket = TimeBucket::new("2024-01".to_string(), 3);
        bucket.row_count = 10;
        bucket.missing_counts[0] = 2;

        assert!((bucket.missing_rate(0) - 0.2).abs() < 0.001);
        assert!((bucket.missing_rate(1) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_time_bucket_numeric_mean() {
        let mut bucket = TimeBucket::new("2024-01".to_string(), 3);
        bucket.record_numeric(0, 10.0);
        bucket.record_numeric(0, 20.0);
        bucket.record_numeric(0, 30.0);

        assert!((bucket.numeric_mean(0).unwrap() - 20.0).abs() < 0.001);
        assert!(bucket.numeric_mean(1).is_none());
    }
}
