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
