use crate::delimiter::{detect_delimiter, DelimiterError};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
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
    skipped_rows: usize,
    malformed_rows: Vec<MalformedRow>,
    finished: bool,
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
        if !delimiter.is_ascii() || matches!(delimiter, '\0' | '\r' | '\n' | '"') {
            return Err(ReaderError::InvalidDelimiter(delimiter));
        }
        let mut csv_reader = csv::ReaderBuilder::new()
            .delimiter(delimiter as u8)
            .has_headers(has_header)
            .flexible(false)
            .from_reader(reader);

        // csv keeps this first record as data when has_headers is false.
        // Validate UTF-8 as records are read, including the header; a byte
        // sample can split a valid multi-byte character at its boundary.
        let first_record = csv_reader.headers()?;
        if first_record.is_empty() {
            return Err(DelimiterError::EmptyFile.into());
        }
        let headers = if has_header {
            let mut seen = HashSet::new();
            for name in first_record {
                if name.trim().is_empty() {
                    return Err(ReaderError::EmptyHeader);
                }
                if !seen.insert(name) {
                    return Err(ReaderError::DuplicateHeader(name.to_owned()));
                }
            }
            first_record.iter().map(str::to_owned).collect()
        } else {
            (0..first_record.len())
                .map(|i| format!("column_{}", i))
                .collect()
        };

        Ok(Self {
            reader: csv_reader,
            headers,
            delimiter,
            row_count: 0,
            skipped_rows: 0,
            malformed_rows: Vec::new(),
            finished: false,
        })
    }

    /// Returns the detected or configured delimiter
    pub fn delimiter(&self) -> char {
        self.delimiter
    }

    /// Returns header names, generated as column_0, column_1, ... without a header.
    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    /// Returns the number of rows read so far
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    pub fn skipped_rows(&self) -> usize {
        self.skipped_rows
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
        if self.reader.finished {
            return None;
        }
        let mut record = csv::StringRecord::new();

        loop {
            match self.reader.reader.read_record(&mut record) {
                Ok(true) => {
                    self.reader.row_count += 1;
                    return Some(Ok(record));
                }
                Ok(false) => {
                    self.reader.finished = true;
                    return None;
                }
                Err(e) if matches!(e.kind(), csv::ErrorKind::UnequalLengths { .. }) => {
                    self.reader.skipped_rows += 1;
                    // Bound diagnostics independently of the number of bad rows.
                    if self.reader.malformed_rows.len() < 5 {
                        self.reader.malformed_rows.push(MalformedRow {
                            line_number: e.position().map_or(0, |p| p.line() as usize),
                            reason: e.to_string(),
                        });
                    }
                }
                Err(e) => {
                    // Encoding and I/O failures must not produce a successful,
                    // partial report or an endless retry loop.
                    self.reader.finished = true;
                    return Some(Err(e.into()));
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV parse error: {0}")]
    CsvParse(#[from] csv::Error),

    #[error("Delimiter detection failed: {0}")]
    DelimiterDetection(#[from] DelimiterError),

    #[error("Invalid delimiter {0:?}: use an ASCII character other than a quote, NUL, or newline")]
    InvalidDelimiter(char),

    #[error("Duplicate header {0:?}: give each column a unique name")]
    DuplicateHeader(String),

    #[error("Empty column header: name each column, or use --no-header for headerless data")]
    EmptyHeader,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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

        let reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();

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

        assert_eq!(reader.headers(), &["column_0", "column_1", "column_2"]);

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

        let reader = CsvReader::open(file.path(), config).unwrap();

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
        assert!(matches!(result, Err(ReaderError::CsvParse(_))));
    }

    #[test]
    fn test_utf8_with_bom() {
        let mut file = NamedTempFile::new().unwrap();
        // Write UTF-8 BOM followed by valid content
        file.write_all(&[0xEF, 0xBB, 0xBF]).unwrap();
        file.write_all(b"a,b,c\n1,2,3\n").unwrap();
        file.flush().unwrap();

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();
        assert_eq!(reader.headers(), &["a", "b", "c"]);
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn test_malformed_field_counts_are_skipped() {
        let content = "a,b,c\n1,2,3\n4,5\n6,7,8,9\n";
        let file = create_temp_csv(content);

        let mut reader = CsvReader::open(file.path(), CsvReaderConfig::default()).unwrap();

        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        assert_eq!(reader.skipped_rows(), 2);
        assert_eq!(reader.malformed_rows()[0].line_number, 3);
        assert_eq!(reader.malformed_rows()[1].line_number, 4);
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
