use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub dtype: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowData {
    pub row_id: u64,
    pub values: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<RowData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataJson {
    pub columns: Vec<ColumnInfo>,
    pub total_rows: usize,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FileMetadata {
    pub path: Option<std::path::PathBuf>,
    pub compression: CompressionPreset,
    pub row_group_size: usize,
    pub data_page_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum CompressionPreset {
    SNAPPY,
    GZIP,
    LZ4,
    ZSTD,
    NONE,
}

impl Default for CompressionPreset {
    fn default() -> Self {
        Self::SNAPPY
    }
}
