#[cfg(test)]
mod tests {
    use super::super::types::*;

    #[test]
    fn test_column_info() {
        let col = ColumnInfo {
            name: "test_col".to_string(),
            dtype: "INT64".to_string(),
            nullable: true,
        };
        assert_eq!(col.name, "test_col");
        assert_eq!(col.dtype, "INT64");
        assert!(col.nullable);
    }

    #[test]
    fn test_metadata_json() {
        let meta = MetadataJson {
            columns: vec![ColumnInfo {
                name: "id".to_string(),
                dtype: "INT32".to_string(),
                nullable: false,
            }],
            total_rows: 100,
            file_path: Some("/test/file.parquet".to_string()),
        };
        assert_eq!(meta.total_rows, 100);
        assert!(meta.file_path.is_some());
    }

    #[test]
    fn test_query_result() {
        let result = QueryResult {
            columns: vec![ColumnInfo {
                name: "col1".to_string(),
                dtype: "VARCHAR".to_string(),
                nullable: true,
            }],
            rows: vec![RowData {
                row_id: 1,
                values: vec![serde_json::Value::String("hello".into())],
            }],
        };
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].row_id, 1);
    }
}
