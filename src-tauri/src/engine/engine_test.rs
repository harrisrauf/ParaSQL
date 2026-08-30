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

fn write_empty_parquet(path: &std::path::Path) {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let sql = format!(
        "CREATE TABLE empty (a INT, b VARCHAR); COPY empty TO '{}' (FORMAT PARQUET);",
        path.display().to_string().replace('\'', "''")
    );
    conn.execute_batch(&sql).unwrap();
}

#[test]
fn insert_row_returns_row_aligned_with_columns() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.parquet");
    write_sample_parquet(&path);

    let mut engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    let columns = engine.column_info().unwrap();

    let result = engine.insert_row().unwrap();
    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0];

    assert_eq!(
        row.values.len(),
        columns.len(),
        "inserted row values must align with data columns (RETURNING must not duplicate _row_id)"
    );
    assert_eq!(row.row_id, 1023);
    // Pattern-aware defaults: id continues 0..1021 -> 1022, name "value_1022"
    assert_eq!(row.values[0], serde_json::json!(1022), "BIGINT pattern default");
    assert_eq!(row.values[1], serde_json::json!("value_1022"), "VARCHAR pattern default");

    let rows = engine.get_all_rows().unwrap();
    assert_eq!(rows.len(), 1023);
}

#[test]
fn alter_table_add_drop_column_is_supported() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE t (a INT); INSERT INTO t VALUES (1), (2);").unwrap();
    conn.execute_batch("ALTER TABLE t ADD COLUMN b INT DEFAULT 0;").unwrap();
    conn.execute_batch("UPDATE t SET b = 7;").unwrap();
    conn.execute_batch("ALTER TABLE t DROP COLUMN b;").unwrap();
    let cols: Vec<String> = conn
        .prepare("SELECT column_name FROM information_schema.columns WHERE table_name = 't'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(cols, vec!["a"]);
}

#[test]
fn create_index_on_row_id_is_supported() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE t (_row_id BIGINT, a INT); \
         INSERT INTO t SELECT range, range FROM range(1000); \
         CREATE INDEX idx_rowid ON t(_row_id);",
    )
    .unwrap();
}

#[test]
fn open_parquet_with_glob_metachars_in_filename() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data[1].parquet");
    write_sample_parquet(&path);

    let engine = DuckDbEngine::open_parquet(path.to_str().unwrap())
        .unwrap_or_else(|e| panic!("opening a literal file named data[1].parquet failed: {}", e));
    assert_eq!(engine.row_count(), 1022);
}

#[test]
fn column_name_cannot_inject_sql() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("evil.parquet");
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let evil_name = "x\" = 5 WHERE _row_id = 1; UPDATE working SET x = 999; --";
    let sql = format!(
        "CREATE TABLE sample (\"x\" INT, \"{}\" INT); \
         INSERT INTO sample VALUES (1, 2), (3, 4); \
         COPY sample TO '{}' (FORMAT PARQUET);",
        evil_name.replace('"', "\"\""),
        path.display().to_string().replace('\'', "''")
    );
    conn.execute_batch(&sql).unwrap();

    let mut engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    let columns = engine.column_info().unwrap();
    let evil_idx = columns
        .iter()
        .position(|c| c.name.contains("UPDATE"))
        .expect("evil column should be present");

    engine
        .edit_cell(1, evil_idx, &serde_json::json!(7), &columns)
        .unwrap();

    let rows = engine.get_all_rows().unwrap();
    assert!(
        rows.iter().all(|r| r.values[0] != serde_json::json!(999)),
        "editing the quoted-name column must not execute injected UPDATEs on column x"
    );
    assert_eq!(rows[0].values[0], serde_json::json!(1));
    assert_eq!(rows[1].values[0], serde_json::json!(3));
}

#[test]
fn failed_mutation_leaves_engine_usable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.parquet");
    write_sample_parquet(&path);

    let mut engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    let columns = engine.column_info().unwrap();
    let before = engine.can_undo();

    let err = engine
        .edit_cell(1, 999, &serde_json::json!(5), &columns)
        .unwrap_err();
    assert!(!err.is_empty(), "out-of-range col_idx must return an error, not panic");

    assert_eq!(engine.can_undo(), before, "failed mutation must not push a savepoint");
    assert_eq!(engine.row_count(), 1022, "engine must remain usable after a failed mutation");
}

#[test]
fn empty_parquet_opens_and_reads_zero_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty.parquet");
    write_empty_parquet(&path);

    let engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    assert_eq!(engine.row_count(), 0);
let columns = engine.column_info().unwrap();
    assert_eq!(columns.len(), 2);

    let rows = engine.get_all_rows().unwrap();
    assert!(rows.is_empty(), "0-row file must return an empty row list, not an error");
}

#[test]
fn undo_redo_round_trips_for_all_mutations() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.parquet");
    write_sample_parquet(&path);

    let mut engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    let columns = engine.column_info().unwrap();
    let id_idx = columns.iter().position(|c| c.name == "id").unwrap();

    // 1. edit_cell -> undo -> redo
    engine
        .edit_cell(1, id_idx, &serde_json::json!(999), &columns)
        .unwrap();
    assert_eq!(engine.get_all_rows().unwrap()[0].values[id_idx], serde_json::json!(999));
    engine.undo().unwrap();
    assert_eq!(engine.get_all_rows().unwrap()[0].values[id_idx], serde_json::json!(0));
    engine.redo().unwrap();
    assert_eq!(engine.get_all_rows().unwrap()[0].values[id_idx], serde_json::json!(999));
    engine.undo().unwrap(); // back to clean state

    // 2. insert_row -> undo -> redo
    let inserted = engine.insert_row().unwrap();
    let new_id = inserted.rows[0].row_id;
    assert_eq!(engine.row_count(), 1023);
    engine.undo().unwrap();
    assert_eq!(engine.row_count(), 1022);
    engine.redo().unwrap();
    assert_eq!(engine.row_count(), 1023);
    assert!(engine.get_all_rows().unwrap().iter().any(|r| r.row_id == new_id));
    engine.undo().unwrap();

    // 3. delete_rows -> undo
    let doomed = vec![2u64, 4u64];
    engine.delete_rows(&doomed).unwrap();
    assert_eq!(engine.row_count(), 1020);
    engine.undo().unwrap();
    assert_eq!(engine.row_count(), 1022);
    let rows = engine.get_all_rows().unwrap();
    assert!(rows.iter().any(|r| r.row_id == 2), "deleted row 2 restored with original row_id");
    assert!(rows.iter().any(|r| r.row_id == 4), "deleted row 4 restored with original row_id");

    // 4. add_column -> undo -> redo
    engine.add_column("extra", "int64").unwrap();
    assert!(engine.column_info().unwrap().iter().any(|c| c.name == "extra"));
    engine.undo().unwrap();
    assert!(!engine.column_info().unwrap().iter().any(|c| c.name == "extra"));
    engine.redo().unwrap();
    assert!(engine.column_info().unwrap().iter().any(|c| c.name == "extra"));
    engine.undo().unwrap();

    // 5. drop_column -> undo restores values
    engine.drop_column("score").unwrap();
    assert!(!engine.column_info().unwrap().iter().any(|c| c.name == "score"));
    engine.undo().unwrap();
    let restored = engine.column_info().unwrap();
    let score_col = restored.iter().find(|c| c.name == "score").unwrap();
    assert_eq!(score_col.dtype, "DECIMAL(21,1)", "dropped-column type preserved");
    assert!(restored.iter().any(|c| c.name == "score"), "score column restored");
    let score_idx = restored.iter().position(|c| c.name == "score").unwrap();
    // score is DECIMAL(21,1) in the sample data (BIGINT * DECIMAL), which this
    // engine renders as a string — type and value must survive the round-trip
    assert_eq!(
        engine.get_all_rows().unwrap()[5].values[score_idx],
        serde_json::json!("7.5"),
        "dropped-column values restored after undo"
    );

    // 6. rename_column -> undo
    engine.rename_column("name", "label").unwrap();
    assert!(engine.column_info().unwrap().iter().any(|c| c.name == "label"));
    engine.undo().unwrap();
    assert!(engine.column_info().unwrap().iter().any(|c| c.name == "name"));

    // 7. new mutation clears the redo stack
    assert!(engine.can_redo(), "undone mutations are redoable");
    engine.edit_cell(1, id_idx, &serde_json::json!(1), &columns).unwrap();
    assert!(!engine.can_redo(), "new mutation must invalidate redo history");

    // 8. undo cap keeps engine functional (550 edits, cap 500)
    for i in 0..550u64 {
        engine
            .edit_cell(1, id_idx, &serde_json::json!(i), &columns)
            .unwrap();
    }
    assert_eq!(engine.row_count(), 1022);
    for _ in 0..10 {
        engine.undo().unwrap();
    }
    assert_eq!(
        engine.get_all_rows().unwrap()[0].values[id_idx],
        serde_json::json!(539),
        "10 undos from value 549 must land on 539"
    );
}

#[test]
fn search_rows_finds_values_across_all_columns() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sample.parquet");
    write_sample_parquet(&path);

    let engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();

    // Whole-table search: matches values in any column, including the name column
    let (rows, truncated) = engine.search_rows("value_1020", None).unwrap();
    assert!(!truncated);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].row_id, 1021);
    assert_eq!(rows[0].values[1], serde_json::json!("value_1020"));

    // Case-insensitive (matches value_5 first by row order; also value_50..59)
    let (rows, _) = engine.search_rows("VALUE_5", None).unwrap();
    assert!(rows.len() >= 1);
    assert_eq!(rows[0].row_id, 6);

    // Per-column search
    let (rows, _) = engine.search_rows("1020", Some("name")).unwrap();
    assert_eq!(rows.len(), 1);
    let (rows, _) = engine.search_rows("1020", Some("id")).unwrap();
    assert_eq!(rows.len(), 1, "id 1020 exists (0-indexed)");

    // LIKE metacharacters in the query are treated literally
    let (rows, _) = engine.search_rows("100%", None).unwrap();
    assert!(rows.is_empty(), "no value contains a literal '%'");

    // No match -> empty, not truncated
    let (rows, truncated) = engine.search_rows("zzz_nope", None).unwrap();
    assert!(rows.is_empty());
    assert!(!truncated);

    // Null values never match
    let (rows, _) = engine.search_rows("null", None).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn insert_row_continues_patterns_without_scanning() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pattern.parquet");
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let sql = r#"
        CREATE TABLE t (id INT, code VARCHAR, ts TIMESTAMP, val DOUBLE);
        INSERT INTO t VALUES
          (1, 'ID-0001', TIMESTAMP '2024-01-01 00:00:00', 1.5),
          (2, 'ID-0002', TIMESTAMP '2024-01-01 00:01:00', 2.5),
          (3, 'ID-0003', TIMESTAMP '2024-01-01 00:02:00', 3.5);
        COPY t TO '<path>' (FORMAT PARQUET);
        "#
    .replace("<path>", &path.to_str().unwrap().replace('\'', "''"));
    conn.execute_batch(&sql).unwrap();

    let mut engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    let row = engine.insert_row().unwrap().rows[0].clone();

    // int: constant +1 step confirmed -> 4
    assert_eq!(row.values[0], serde_json::json!(4));
    // varchar: "prefix + zero-padded counter" convention -> ID-0004
    assert_eq!(row.values[1], serde_json::json!("ID-0004"));
    // timestamp: constant 60s step -> +1 minute
    assert_eq!(
        row.values[2],
        serde_json::json!("2024-01-01 00:03:00.000")
    );
    // float: constant +1.0 step -> 4.5
    assert_eq!(row.values[3], serde_json::json!(4.5));

    // A second insert continues the (now extended) sequence
    let row2 = engine.insert_row().unwrap().rows[0].clone();
    assert_eq!(row2.values[0], serde_json::json!(5));
    assert_eq!(row2.values[1], serde_json::json!("ID-0005"));
    assert_eq!(row2.values[3], serde_json::json!(5.5));
}

#[test]
fn insert_row_does_not_guess_without_a_confirmed_pattern() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("noise.parquet");
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let sql = r#"
        CREATE TABLE t (noisy_int INT, noisy_float DOUBLE, seq INT, code VARCHAR);
        INSERT INTO t VALUES
          (5, 6.3, 10, 'A-10'),
          (6, 6.5, 11, 'A-11'),
          (1, 6.2, 12, 'A-12'),
          (3, 5.9, 13, 'A-13'),
          (8, 6.1, 14, 'A-14'),
          (2, 6.4, 15, 'A-15'),
          (9, 6.0, 16, 'A-16'),
          (4, 5.7, 17, 'A-17');
        COPY t TO '<path>' (FORMAT PARQUET);
        "#
    .replace("<path>", &path.to_str().unwrap().replace('\'', "''"));
    conn.execute_batch(&sql).unwrap();

    let mut engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();
    let row = engine.insert_row().unwrap().rows[0].clone();

    // No majority step in the noisy columns -> NULL (nullable), no guess
    assert_eq!(row.values[0], serde_json::json!(null));
    assert_eq!(row.values[1], serde_json::json!(null));
    // Clean +1 sequences -> still continued
    assert_eq!(row.values[2], serde_json::json!(18));
    assert_eq!(row.values[3], serde_json::json!("A-18"));
}
