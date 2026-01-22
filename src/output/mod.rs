pub mod json;
pub mod terminal;

use crate::heuristics::ColumnHeuristics;
use crate::stats::ColumnStats;
use crate::warnings::Warning;

/// Complete profile result for output
#[derive(Debug, Clone)]
pub struct ProfileResult {
    pub file_path: String,
    pub file_size_bytes: u64,
    pub row_count: usize,
    pub column_count: usize,
    pub delimiter: char,
    pub has_header: bool,
    pub columns: Vec<ColumnProfile>,
    pub warnings: Vec<Warning>,
}

/// Profile data for a single column
#[derive(Debug, Clone)]
pub struct ColumnProfile {
    pub name: String,
    pub stats: ColumnStats,
    pub heuristics: ColumnHeuristics,
}

impl ColumnProfile {
    pub fn new(name: String, stats: ColumnStats, heuristics: ColumnHeuristics) -> Self {
        Self { name, stats, heuristics }
    }
}
