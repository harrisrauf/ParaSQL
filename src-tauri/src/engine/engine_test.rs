use tempfile::tempdir;

use super::duckdb::DuckDbEngine;

fn write_sample_parquet(path: &std::path::Path) {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let sql = format!(
        r#"
        CREATE TABLE sample AS
        SELECT
            range AS id,
            'value_' || range::VARCHAR AS name,
            range * 1.5 AS score,
            CASE WHEN range % 2 = 0 THEN true ELSE false END AS flag,
            DATE '2020-01-01' + range::INTEGER AS day,
            TIMESTAMP '2020-01-01 00:00:00' + range::INTEGER * INTERVAL 1 MINUTE AS ts,
            CASE WHEN range % 3 = 0 THEN NULL ELSE range END AS nullable_col
        FROM range(1022);
        COPY sample TO '{}' (FORMAT PARQUET);
        "#,
        path.display().to_string().replace('\'', "''")
    );
    conn.execute_batch(&sql).unwrap();
}

#[test]
fn engine_opens_parquet_and_reads_all_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.parquet");
    write_sample_parquet(&path);

    let engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    assert_eq!(engine.file_path(), Some(path.as_path()));

    assert_eq!(engine.row_count(), 1022);

    let columns = engine.column_info().unwrap();
    assert_eq!(columns.len(), 7);
    assert!(columns.iter().all(|c| c.name != "_row_id"));

    let rows = engine.get_all_rows().unwrap();
    assert_eq!(rows.len(), 1022);

    let first = &rows[0];
    assert_eq!(first.row_id, 1);
    assert_eq!(first.values.len(), 7);
    assert_eq!(first.values[0], serde_json::json!(0));
    assert_eq!(first.values[1], serde_json::json!("value_0"));

    let last = rows.last().unwrap();
    assert_eq!(last.row_id, 1022);
    assert_eq!(last.values[0], serde_json::json!(1021));
}

#[test]
fn engine_page_loading_and_schema_dialects() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.parquet");
    write_sample_parquet(&path);

    let engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();

    let page = engine.get_page(1000, 100).unwrap();
    assert_eq!(page.len(), 22);
    assert_eq!(page[0].row_id, 1001);

    let duck = engine.generate_schema_sql("duckdb").unwrap();
    assert!(duck.contains("VARCHAR"));
    assert!(duck.contains("BOOLEAN"));

    let mysql = engine.generate_schema_sql("mysql").unwrap();
    assert!(mysql.contains("VARCHAR(255)"));
    assert!(mysql.contains("DATETIME"));

    let sqlite = engine.generate_schema_sql("sqlite").unwrap();
    assert!(sqlite.contains("TEXT"));
    assert!(sqlite.contains("INTEGER"));
}