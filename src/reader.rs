use crate::delimiter::{detect_delimiter, DelimiterError};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Configuration for reading CSV files
#[derive(Debug, Clone)]
pub struct CsvReaderConfig {
    /// Delimiter character (None for auto-detect)
    pub delimiter: Option<char>,
    /// Whether the first row is a header
    pub has_header: bool,
}

impl Default for CsvReaderConfig {
    fn default() -> Self {
        Self {
            delimiter: None,
            has_header: true,
        }
    }
}

/// A streaming CSV reader with auto-detection capabilities
pub struct CsvReader<R> {
    reader: csv::Reader<R>,
    headers: Vec<String>,
    delimiter: char,
    row_count: usize,
    malformed_rows: Vec<MalformedRow>,
}

/// Information about a malformed row
#[derive(Debug, Clone)]
pub struct MalformedRow {
    pub line_number: usize,
    pub reason: String,
}

impl CsvReader<BufReader<File>> {
    /// Opens a CSV file with the given configuration
    pub fn open<P: AsRef<Path>>(path: P, config: CsvReaderConfig) -> Result<Self, ReaderError> {
        let file = File::open(path.as_ref())?;
        let mut buf_reader = BufReader::new(file);

        // Validate UTF-8 by reading a sample
        validate_utf8(&mut buf_reader)?;
        buf_reader.seek(std::io::SeekFrom::Start(0))?;

        // Detect delimiter if not specified
        let delimiter = match config.delimiter {
            Some(d) => d,
            None => detect_delimiter(&mut buf_reader)?,
        };

        Self::from_reader(buf_reader, delimiter, config.has_header)
    }
}

impl<R: Read> CsvReader<R> {
    /// Creates a CSV reader from any Read implementation
    pub fn from_reader(reader: R, delimiter: char, has_header: bool) -> Result<Self, ReaderError> {
        let mut csv_reader = csv::ReaderBuilder::new()
            .delimiter(delimiter as u8)
            .has_headers(has_header)
            .flexible(true) // Allow varying number of fields
            .from_reader(reader);

        let headers = if has_header {
            csv_reader
                .headers()
                .map(|h| h.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(Self {
            reader: csv_reader,
            headers,
            delimiter,
            row_count: 0,
            malformed_rows: Vec::new(),
        })
    }

    /// Returns the detected or configured delimiter
    pub fn delimiter(&self) -> char {
        self.delimiter
    }

    /// Returns the headers (empty if no header row)
    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    /// Returns the number of rows read so far
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// Returns information about malformed rows encountered
    pub fn malformed_rows(&self) -> &[MalformedRow] {
        &self.malformed_rows
    }

    /// Returns an iterator over the records
    pub fn records(&mut self) -> RecordIterator<'_, R> {
        RecordIterator { reader: self }
    }
}

/// Iterator over CSV records
pub struct RecordIterator<'a, R> {
    reader: &'a mut CsvReader<R>,
}

impl<'a, R: Read> Iterator for RecordIterator<'a, R> {
    type Item = Result<csv::StringRecord, ReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut record = csv::StringRecord::new();

        loop {
            match self.reader.reader.read_record(&mut record) {
                Ok(true) => {
                    self.reader.row_count += 1;
                    return Some(Ok(record));
                }
                Ok(false) => return None,
                Err(e) => {
                    // Handle malformed rows
                    let line_number = self.reader.row_count + 1;
                    if self.reader.headers.is_empty() {
                        // No header, so line number is row count + 1
                    } else {
                        // With header, line number is row count + 2
                    }

                    self.reader.malformed_rows.push(MalformedRow {
                        line_number: line_number + if self.reader.headers.is_empty() { 0 } else { 1 },
                        reason: e.to_string(),
                    });

                    // Log warning and continue to next row
                    eprintln!(
                        "Warning: Skipping malformed row {}: {}",
                        line_number, e
                    );

                    // Try to continue reading
                    self.reader.row_count += 1;
                    continue;
                }
            }
        }
    }
}

/// Validates that the file contains valid UTF-8
fn validate_utf8<R: Read + Seek>(reader: &mut R) -> Result<(), ReaderError> {
    const SAMPLE_SIZE: usize = 64 * 1024; // 64KB sample

    let mut sample = vec![0u8; SAMPLE_SIZE];
    let bytes_read = reader.read(&mut sample)?;
    sample.truncate(bytes_read);

    // Check for BOM and skip if present
    let content = if sample.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &sample[3..]
    } else {
        &sample[..]
    };

    // Validate UTF-8
    std::str::from_utf8(content).map_err(|e| ReaderError::InvalidUtf8 {
        byte_offset: e.valid_up_to(),
    })?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV parse error: {0}")]
    CsvParse(#[from] csv::Error),

    #[error("Delimiter detection failed: {0}")]
    DelimiterDetection(#[from] DelimiterError),

    #[error("Invalid UTF-8 encoding at byte offset {byte_offset}")]
    InvalidUtf8 { byte_offset: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn create_temp_csv(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_read_simple_csv() {
        let content = "a,b,c\n1,2,3\n4,5,6\n";
        let file = create_temp_csv(content);

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();

        assert_eq!(reader.headers(), &["a", "b", "c"]);
        assert_eq!(reader.delimiter(), ',');

        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].as_ref().unwrap().get(0), Some("1"));
    }

    #[test]
    fn test_read_tab_delimited() {
        let content = "a\tb\tc\n1\t2\t3\n";
        let file = create_temp_csv(content);

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();

        assert_eq!(reader.delimiter(), '\t');
        assert_eq!(reader.headers(), &["a", "b", "c"]);
    }

    #[test]
    fn test_read_no_header() {
        let content = "1,2,3\n4,5,6\n";
        let file = create_temp_csv(content);

        let config = CsvReaderConfig {
            delimiter: None,
            has_header: false,
        };

        let mut reader = CsvReader::open(file.path(), config).unwrap();

        assert!(reader.headers().is_empty());

        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].as_ref().unwrap().get(0), Some("1"));
    }

    #[test]
    fn test_explicit_delimiter() {
        let content = "a;b;c\n1;2;3\n";
        let file = create_temp_csv(content);

        let config = CsvReaderConfig {
            delimiter: Some(';'),
            has_header: true,
        };

        let mut reader = CsvReader::open(file.path(), config).unwrap();

        assert_eq!(reader.delimiter(), ';');
        assert_eq!(reader.headers(), &["a", "b", "c"]);
    }

    #[test]
    fn test_invalid_utf8() {
        let mut file = NamedTempFile::new().unwrap();
        // Write invalid UTF-8 sequence
        file.write_all(&[0x80, 0x81, 0x82]).unwrap();
        file.flush().unwrap();

        let result = CsvReader::open(file.path(), CsvReaderConfig::default());
        assert!(matches!(result, Err(ReaderError::InvalidUtf8 { .. })));
    }

    #[test]
    fn test_utf8_with_bom() {
        let mut file = NamedTempFile::new().unwrap();
        // Write UTF-8 BOM followed by valid content
        file.write_all(&[0xEF, 0xBB, 0xBF]).unwrap();
        file.write_all(b"a,b,c\n1,2,3\n").unwrap();
        file.flush().unwrap();

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();
        // Note: csv crate may include BOM in first header, this is expected behavior
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn test_flexible_field_count() {
        let content = "a,b,c\n1,2,3\n4,5\n6,7,8,9\n";
        let file = create_temp_csv(content);

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();

        let records: Vec<_> = reader.records().collect();
        // All rows should be read due to flexible mode
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn test_quoted_fields() {
        let content = "name,value\n\"hello, world\",123\n";
        let file = create_temp_csv(content);

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();

        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].as_ref().unwrap().get(0), Some("hello, world"));
    }

    #[test]
    fn test_empty_file() {
        let content = "";
        let file = create_temp_csv(content);

        let result = CsvReader::open(file.path(), CsvReaderConfig::default());
        // Empty file should fail delimiter detection
        assert!(result.is_err());
    }

    #[test]
    fn test_row_count() {
        let content = "a,b\n1,2\n3,4\n5,6\n";
        let file = create_temp_csv(content);

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();

        let _: Vec<_> = reader.records().collect();
        assert_eq!(reader.row_count(), 3);
    }
}
