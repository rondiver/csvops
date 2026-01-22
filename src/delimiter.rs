use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

/// Supported delimiters for CSV files
pub const DELIMITERS: [char; 4] = [',', '\t', '|', ';'];

/// Size of the sample to read for delimiter detection (8KB)
const SAMPLE_SIZE: usize = 8 * 1024;

/// Detects the most likely delimiter from a sample of the file.
///
/// Uses a scoring algorithm that considers:
/// 1. Consistency of field counts across lines
/// 2. Frequency of the delimiter
/// 3. Preference for common delimiters (comma > tab > pipe > semicolon)
pub fn detect_delimiter<R: Read + Seek>(reader: &mut R) -> Result<char, DelimiterError> {
    // Read sample
    let mut sample = vec![0u8; SAMPLE_SIZE];
    let bytes_read = reader.read(&mut sample)?;
    sample.truncate(bytes_read);

    // Reset reader position
    reader.seek(SeekFrom::Start(0))?;

    // Convert to string, handling UTF-8
    let sample_str = String::from_utf8_lossy(&sample);

    // Get lines from sample
    let lines: Vec<&str> = sample_str.lines().take(100).collect();

    if lines.is_empty() {
        return Err(DelimiterError::EmptyFile);
    }

    // Score each delimiter
    let mut scores: HashMap<char, f64> = HashMap::new();

    for &delim in &DELIMITERS {
        if let Some(score) = score_delimiter(&lines, delim) {
            scores.insert(delim, score);
        }
    }

    // Find best delimiter
    scores
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(delim, _)| delim)
        .ok_or(DelimiterError::NoDelimiterFound)
}

/// Scores a delimiter based on consistency and frequency
fn score_delimiter(lines: &[&str], delim: char) -> Option<f64> {
    if lines.is_empty() {
        return None;
    }

    // Count fields per line
    let field_counts: Vec<usize> = lines
        .iter()
        .map(|line| count_fields(line, delim))
        .collect();

    // If no line has more than 1 field, this delimiter is not useful
    if field_counts.iter().all(|&c| c <= 1) {
        return None;
    }

    // Calculate consistency score (how many lines have the same field count as the first)
    let first_count = field_counts[0];
    let consistent_lines = field_counts.iter().filter(|&&c| c == first_count).count();
    let consistency_score = consistent_lines as f64 / field_counts.len() as f64;

    // Calculate average field count (more fields = more likely correct delimiter)
    let avg_fields: f64 = field_counts.iter().sum::<usize>() as f64 / field_counts.len() as f64;

    // Preference bonus for common delimiters
    let preference_bonus = match delim {
        ',' => 1.2,
        '\t' => 1.1,
        '|' => 1.0,
        ';' => 0.9,
        _ => 0.8,
    };

    // Combined score
    Some(consistency_score * avg_fields * preference_bonus)
}

/// Counts the number of fields in a line, respecting quoted fields
fn count_fields(line: &str, delim: char) -> usize {
    let mut count = 1;
    let mut in_quotes = false;

    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c == delim && !in_quotes => count += 1,
            _ => {}
        }
    }

    count
}

#[derive(Debug, thiserror::Error)]
pub enum DelimiterError {
    #[error("File is empty")]
    EmptyFile,
    #[error("Could not detect delimiter")]
    NoDelimiterFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_detect_comma_delimiter() {
        let data = "a,b,c\n1,2,3\n4,5,6\n";
        let mut cursor = Cursor::new(data);
        assert_eq!(detect_delimiter(&mut cursor).unwrap(), ',');
    }

    #[test]
    fn test_detect_tab_delimiter() {
        let data = "a\tb\tc\n1\t2\t3\n4\t5\t6\n";
        let mut cursor = Cursor::new(data);
        assert_eq!(detect_delimiter(&mut cursor).unwrap(), '\t');
    }

    #[test]
    fn test_detect_pipe_delimiter() {
        let data = "a|b|c\n1|2|3\n4|5|6\n";
        let mut cursor = Cursor::new(data);
        assert_eq!(detect_delimiter(&mut cursor).unwrap(), '|');
    }

    #[test]
    fn test_detect_semicolon_delimiter() {
        let data = "a;b;c\n1;2;3\n4;5;6\n";
        let mut cursor = Cursor::new(data);
        assert_eq!(detect_delimiter(&mut cursor).unwrap(), ';');
    }

    #[test]
    fn test_detect_with_quoted_fields() {
        let data = "\"a,b\",c,d\n\"1,2\",3,4\n";
        let mut cursor = Cursor::new(data);
        assert_eq!(detect_delimiter(&mut cursor).unwrap(), ',');
    }

    #[test]
    fn test_comma_preferred_over_semicolon() {
        // Both could work, but comma should be preferred
        let data = "a,b;c\n1,2;3\n";
        let mut cursor = Cursor::new(data);
        assert_eq!(detect_delimiter(&mut cursor).unwrap(), ',');
    }

    #[test]
    fn test_empty_file_error() {
        let data = "";
        let mut cursor = Cursor::new(data);
        assert!(matches!(
            detect_delimiter(&mut cursor),
            Err(DelimiterError::EmptyFile)
        ));
    }

    #[test]
    fn test_count_fields_simple() {
        assert_eq!(count_fields("a,b,c", ','), 3);
        assert_eq!(count_fields("a\tb\tc", '\t'), 3);
    }

    #[test]
    fn test_count_fields_with_quotes() {
        assert_eq!(count_fields("\"a,b\",c,d", ','), 3);
        assert_eq!(count_fields("\"a,b,c\"", ','), 1);
    }

    #[test]
    fn test_consistency_scoring() {
        // Consistent field counts should score higher
        let consistent = "a,b,c\n1,2,3\n4,5,6\n";
        let inconsistent = "a,b,c\n1,2\n4,5,6,7\n";

        let mut cursor1 = Cursor::new(consistent);
        let mut cursor2 = Cursor::new(inconsistent);

        // Both should detect comma, but we're testing the detection works
        assert_eq!(detect_delimiter(&mut cursor1).unwrap(), ',');
        assert_eq!(detect_delimiter(&mut cursor2).unwrap(), ',');
    }
}
