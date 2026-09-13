use super::catalog::{SavedQuery, Workspace, WorkspaceTable};
use super::duckdb::DuckDbEngine;
use duckdb::Connection;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../Sample_data")
}

fn sales_dir() -> PathBuf {
    sample_dir().join("sales")
}

fn to_sql_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").replace('\'', "''")
}

fn ws_table(name: &str, file: &Path) -> WorkspaceTable {
    let abs = file.to_string_lossy().to_string();
    WorkspaceTable {
        name: name.to_string(),
        path: abs.clone(),
        source: "file".to_string(),
        mode: "query".to_string(),
        compression: None,
        size: None,
        mtime: None,
        abs_path: Some(abs),
        missing: false,
    }
}

/// The four Parquet files shipped in `Sample_data/sales` (regenerable with the
/// ignored `generate_sales_sample_files` test). Same schema story as a typical
/// star schema: products, customers, orders, order_items.
const SALES_TABLES: &[(&str, &str); 4] = &[
    ("products", "products.parquet"),
    ("customers", "customers.parquet"),
    ("orders", "orders.parquet"),
    ("order_items", "order_items.parquet"),
];

fn sales_workspace() -> Workspace {
    let dir = sales_dir();
    let mut ws = Workspace::new("sales");
    for (name, file) in SALES_TABLES {
        ws.tables.push(ws_table(name, &dir.join(file)));
    }
    ws
}

fn open_sales() -> Option<DuckDbEngine> {
    if !sales_dir().join("order_items.parquet").exists() {
        eprintln!("Sample_data/sales not found; skipping");
        return None;
    }
    Some(DuckDbEngine::open_workspace(&sales_workspace()).expect("sales workspace should open"))
}

fn n(v: &serde_json::Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_i64().map(|i| i as u64))
        .or_else(|| v.as_f64().map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        .unwrap_or_else(|| panic!("not numeric: {:?}", v))
}

/// Order-independent checksum of a result set: row count plus every row rendered
/// as text and sorted. Uses core DuckDB functions only — no extensions, no
/// network — so it runs anywhere, including CI.
fn checksum_sql(sql: &str) -> String {
    format!(
        "SELECT COUNT(*) AS n, string_agg(t::VARCHAR, '|' ORDER BY t::VARCHAR) AS h FROM ({}) t",
        sql
    )
}

/// Compute the same query through a plain `read_parquet` connection to prove the
/// workspace view layer is transparent (the app's abstraction must not change
/// results).
fn direct_checksum(dir: &Path, tables: &[(&str, &str)], sql: &str) -> (i64, Option<String>) {
    let conn = Connection::open_in_memory().unwrap();
    for (name, file) in tables {
        let path = to_sql_path(&dir.join(file));
        conn.execute_batch(&format!(
            "CREATE VIEW {} AS SELECT * FROM read_parquet('{}')",
            name, path
        ))
        .unwrap();
    }
    conn.query_row(&checksum_sql(sql), [], |row| {
        Ok((
            row.get::<usize, i64>(0)?,
            row.get::<usize, Option<String>>(1)?,
        ))
    })
    .unwrap()
}

fn engine_checksum(engine: &DuckDbEngine, sql: &str) -> (i64, Option<String>) {
    let q = engine.execute_sql(&checksum_sql(sql)).unwrap();
    let values = &q.rows[0].values;
    (n(&values[0]) as i64, values[1].as_str().map(str::to_string))
}

#[test]
fn sample_sales_inventory() {
    let Some(engine) = open_sales() else {
        return;
    };
    let expected: &[(&str, u64)] = &[
        ("products", 200),
        ("customers", 20000),
        ("orders", 100000),
        ("order_items", 300000),
    ];
    for (name, rows) in expected {
        let q = engine
            .execute_sql(&format!("SELECT COUNT(*) AS n FROM {}", name))
            .unwrap();
        assert_eq!(n(&q.rows[0].values[0]), *rows, "row count for {}", name);
    }
    let shape = engine
        .execute_sql("SELECT * FROM order_items LIMIT 1")
        .unwrap();
    assert_eq!(shape.columns.len(), 5, "order_items column count");
}

/// Run once to create `Sample_data/parasql-demo.parasql` for manual app testing:
/// `cargo test --lib generate_sample_workspace -- --ignored --nocapture`
#[test]
#[ignore = "writes Sample_data/parasql-demo.parasql for manual testing"]
fn generate_sample_workspace() {
    let dir = sales_dir();
    let mut ws = Workspace::new("ParaSQL demo");
    for (name, file) in SALES_TABLES {
        let abs = dir.join(file);
        let mut t = ws_table(name, &abs);
        t.size = std::fs::metadata(&abs).ok().map(|m| m.len());
        ws.tables.push(t);
    }
    ws.saved_queries.push(SavedQuery {
        name: "Revenue by category".into(),
        sql: "SELECT p.category, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue \
              FROM order_items oi JOIN products p ON p.product_id = oi.product_id \
              GROUP BY 1 ORDER BY 2 DESC"
            .into(),
    });
    ws.saved_queries.push(SavedQuery {
        name: "Monthly revenue".into(),
        sql: "SELECT strftime(o.order_date, '%Y-%m') AS month, \
              ROUND(SUM(oi.quantity * oi.unit_price), 2) AS revenue \
              FROM orders o JOIN order_items oi ON oi.order_id = o.order_id \
              JOIN products p ON p.product_id = oi.product_id GROUP BY 1 ORDER BY 1"
            .into(),
    });
    ws.saved_queries.push(SavedQuery {
        name: "Top customers".into(),
        sql: "SELECT c.name, c.country, ROUND(SUM(oi.quantity * oi.unit_price), 2) AS revenue \
              FROM order_items oi JOIN orders o ON o.order_id = oi.order_id \
              JOIN customers c ON c.customer_id = o.customer_id \
              GROUP BY 1, 2 ORDER BY 3 DESC LIMIT 10"
            .into(),
    });
    let path = sample_dir().join("parasql-demo.parasql");
    super::catalog::write_workspace(path.to_str().unwrap(), &ws).unwrap();
    eprintln!("wrote {}", path.display());
}

#[test]
fn demo_workspace_file_loads() {
    let path = sample_dir().join("parasql-demo.parasql");
    if !path.exists() {
        eprintln!("parasql-demo.parasql missing; run the ignored generator first");
        return;
    }
    let ws = super::catalog::load_workspace(path.to_str().unwrap()).unwrap();
    assert!(ws.tables.len() >= 4, "tables: {}", ws.tables.len());
    assert!(ws.saved_queries.len() >= 3);
    let engine = DuckDbEngine::open_workspace(&ws).unwrap();
    let q = engine
        .execute_sql("SELECT COUNT(*) AS n FROM order_items")
        .unwrap();
    assert_eq!(n(&q.rows[0].values[0]), 300000);
    let p = engine
        .execute_sql("SELECT COUNT(*) AS n FROM products")
        .unwrap();
    assert_eq!(n(&p.rows[0].values[0]), 200);
}

#[test]
fn workspace_views_match_direct_parquet_reads() {
    let Some(engine) = open_sales() else {
        return;
    };
    let queries = [
        "SELECT category, COUNT(*) AS n, ROUND(SUM(unit_price), 2) AS total FROM products GROUP BY 1 ORDER BY 1",
        "SELECT status, COUNT(*) AS n FROM orders GROUP BY 1 ORDER BY 1",
        "SELECT p.category, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue \
         FROM order_items oi JOIN products p ON p.product_id = oi.product_id GROUP BY 1",
        "SELECT strftime(o.order_date, '%Y-%m') AS month, COUNT(*) AS n \
         FROM orders o GROUP BY 1 ORDER BY 1",
    ];
    for sql in queries {
        assert_eq!(
            engine_checksum(&engine, sql),
            direct_checksum(&sales_dir(), SALES_TABLES, sql),
            "query diverged: {}",
            sql
        );
    }
}

/// Narrow integer columns, unicode text and duplicate aliases — the JSON-layer
/// edge cases the grid depends on.
#[test]
fn query_result_edge_cases() {
    let tmp = std::env::temp_dir().join(format!("parasql_types_it_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("types.parquet");
    let p = to_sql_path(&path);
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE t (col_int8 TINYINT, col_int32 INTEGER, col_int64 BIGINT, \
         col_float64 DOUBLE, col_bool BOOLEAN, col_string VARCHAR, col_nullable INTEGER);
         INSERT INTO t VALUES
           (1, 1000, 9007199254740993, 1.5, TRUE, 'строка-1 文字 1', NULL),
           (2, 2, 2, 2.5, FALSE, 'plain', 7);
         COPY t TO '{p}' (FORMAT PARQUET);"
    ))
    .unwrap();

    let engine = DuckDbEngine::open_parquet(path.to_str().unwrap()).unwrap();

    // Narrow integer columns must render as numbers, not "[unsupported type]".
    let q = engine
        .execute_sql(
            "SELECT col_int8, col_int32, col_int64 FROM working ORDER BY col_int32 LIMIT 3",
        )
        .unwrap();
    assert!(
        q.rows.iter().all(|r| r.values[0].is_number()),
        "int8 values: {:?}",
        q.rows[0].values
    );

    // SUM over integers arrives as HUGEINT (Decimal128 scale 0) — stays numeric.
    let s = engine
        .execute_sql("SELECT SUM(col_int64) AS total FROM working")
        .unwrap();
    assert!(s.rows[0].values[0].is_number(), "sum: {:?}", s.rows[0].values[0]);

    // Unicode round trip.
    let u = engine
        .execute_sql("SELECT col_string FROM working WHERE col_int32 = 1000")
        .unwrap();
    assert_eq!(u.rows[0].values[0].as_str().unwrap(), "строка-1 文字 1");

    // Duplicate aliases must be unique in the result schema for the grid.
    let dup = engine
        .execute_sql("SELECT 1 AS x, 2 AS x FROM working LIMIT 1")
        .unwrap();
    let mut names: Vec<String> = dup.columns.iter().map(|c| c.name.clone()).collect();
    let total = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), total, "duplicate column names survived: {:?}", names);

    // Empty result keeps its column shape.
    let empty = engine
        .execute_sql("SELECT col_int32 FROM working WHERE col_int32 > 999999")
        .unwrap();
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns.len(), 1);

    // Narrow types are queryable through the view layer.
    let filtered = engine
        .execute_sql("SELECT COUNT(col_int8) AS n FROM working")
        .unwrap();
    assert!(n(&filtered.rows[0].values[0]) > 0);

    let _ = std::fs::remove_dir_all(&tmp);
}

// --- Generated relational sample data (star schema) --------------------------

const PRODUCTS_SQL: &str = "SELECT i::BIGINT AS product_id, \
     'Product ' || lpad(i::VARCHAR, 3, '0') AS name, \
     (['Electronics','Books','Home','Toys','Sports','Beauty','Garden','Auto'])[1 + ((i - 1) % 8)] AS category, \
     round(5 + ((i * 37) % 400) / 2.0, 2) AS unit_price \
     FROM range(1, 201) t(i)";

const CUSTOMERS_SQL: &str = "SELECT i::BIGINT AS customer_id, \
     'Customer ' || lpad(i::VARCHAR, 5, '0') AS name, \
     (['Berlin','London','Sydney','Paris','Tokyo','Toronto','Austin','Madrid','Rome','Seoul','Oslo','Lima'])[1 + ((i - 1) % 12)] AS city, \
     (['DE','GB','AU','FR','JP','CA','US','ES','IT','KR','NO','PE'])[1 + ((i - 1) % 12)] AS country, \
     DATE '2022-01-01' + ((i * 7) % 1095)::INTEGER AS signup_date \
     FROM range(1, 20001) t(i)";

const ORDERS_SQL: &str = "SELECT i::BIGINT AS order_id, \
     (((i * 7919) % 20000) + 1)::BIGINT AS customer_id, \
     DATE '2022-01-01' + ((i * 13) % 1095)::INTEGER AS order_date, \
     (['pending','paid','shipped','delivered','cancelled'])[1 + (i % 5)] AS status \
     FROM range(1, 100001) t(i)";

const ORDER_ITEMS_SQL: &str = "SELECT i::BIGINT AS item_id, \
     (((i - 1) // 3) + 1)::BIGINT AS order_id, \
     (((i * 31) % 200) + 1)::BIGINT AS product_id, \
     ((i % 5) + 1)::BIGINT AS quantity, \
     round(5 + (((((i * 31) % 200) + 1) * 37) % 400) / 2.0, 2) AS unit_price \
     FROM range(1, 300001) t(i)";

fn generate_sales_schema(conn: &Connection, dir: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let d = to_sql_path(dir);
    let tables: [(&str, &str); 4] = [
        ("products", PRODUCTS_SQL),
        ("customers", CUSTOMERS_SQL),
        ("orders", ORDERS_SQL),
        ("order_items", ORDER_ITEMS_SQL),
    ];
    for (name, sql) in tables {
        conn.execute_batch(&format!(
            "COPY ({}) TO '{}/{}.parquet' (FORMAT PARQUET, COMPRESSION ZSTD)",
            sql, d, name
        ))
        .map_err(|e| format!("generating {}: {}", name, e))?;
    }
    Ok(())
}

/// Run once to (re)create `Sample_data/sales/*.parquet`:
/// `cargo test --lib generate_sales_sample_files -- --ignored --nocapture`
#[test]
#[ignore = "writes Sample_data/sales/*.parquet for manual testing"]
fn generate_sales_sample_files() {
    let dir = sales_dir();
    let conn = Connection::open_in_memory().unwrap();
    generate_sales_schema(&conn, &dir).unwrap();
    for f in ["products", "customers", "orders", "order_items"] {
        let p = dir.join(format!("{}.parquet", f));
        let size = std::fs::metadata(&p).unwrap().len();
        eprintln!("{}: {:.2} MB", p.display(), size as f64 / 1_048_576.0);
    }
}

#[test]
fn generated_star_schema_multi_file_analytics() {
    let dir = std::env::temp_dir().join(format!("parasql_sales_it_{}", std::process::id()));
    let conn = Connection::open_in_memory().unwrap();
    generate_sales_schema(&conn, &dir).unwrap();

    let tables: [(&str, &str); 4] = [
        ("products", "products.parquet"),
        ("customers", "customers.parquet"),
        ("orders", "orders.parquet"),
        ("order_items", "order_items.parquet"),
    ];
    let mut ws = Workspace::new("sales");
    for (t, file) in tables {
        ws.tables.push(ws_table(t, &dir.join(file)));
    }
    let engine = DuckDbEngine::open_workspace(&ws).unwrap();

    for (name, expected) in [
        ("products", 200),
        ("customers", 20000),
        ("orders", 100000),
        ("order_items", 300000),
    ] {
        let q = engine
            .execute_sql(&format!("SELECT COUNT(*) AS n FROM {}", name))
            .unwrap();
        assert_eq!(n(&q.rows[0].values[0]), expected, "count for {}", name);
    }

    // Every order has exactly three items by construction.
    let bad = engine
        .execute_sql(
            "SELECT COUNT(*) AS n FROM (SELECT order_id FROM order_items GROUP BY order_id HAVING COUNT(*) <> 3) t",
        )
        .unwrap();
    assert_eq!(n(&bad.rows[0].values[0]), 0);

    let queries = [
        "SELECT p.category, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue FROM order_items oi JOIN products p ON p.product_id = oi.product_id GROUP BY 1 ORDER BY 1",
        "SELECT COUNT(*) AS customers_without_orders FROM customers c WHERE NOT EXISTS (SELECT 1 FROM orders o WHERE o.customer_id = c.customer_id)",
        "SELECT strftime(o.order_date, '%Y-%m') AS month, COUNT(*) AS orders, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue FROM orders o JOIN order_items oi ON oi.order_id = o.order_id JOIN products p ON p.product_id = oi.product_id GROUP BY 1 ORDER BY 1",
        "SELECT c.country, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue, COUNT(DISTINCT o.order_id) AS orders FROM orders o JOIN customers c ON c.customer_id = o.customer_id JOIN order_items oi ON oi.order_id = o.order_id JOIN products p ON p.product_id = oi.product_id GROUP BY 1 ORDER BY 2 DESC",
        "SELECT month, revenue, ROUND(SUM(revenue) OVER (ORDER BY month), 2) AS running_revenue FROM (SELECT strftime(o.order_date, '%Y-%m') AS month, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue FROM orders o JOIN order_items oi ON oi.order_id = o.order_id JOIN products p ON p.product_id = oi.product_id GROUP BY 1) t ORDER BY month",
    ];
    for sql in queries {
        assert_eq!(
            engine_checksum(&engine, sql),
            direct_checksum(&dir, &tables, sql),
            "query diverged: {}",
            sql
        );
    }

    let category_rows = engine
        .execute_sql("SELECT category, COUNT(*) AS n FROM products GROUP BY category ORDER BY category")
        .unwrap();
    assert_eq!(category_rows.rows.len(), 8);
    assert!(category_rows.rows.iter().all(|r| n(&r.values[1]) == 25));

    let _ = std::fs::remove_dir_all(&dir);
}

// --- Large-file behavior ------------------------------------------------------

#[test]
fn large_file_pagination_search_and_materialization() {
    let dir = std::env::temp_dir().join(format!("parasql_large_it_{}", std::process::id()));
    let conn = Connection::open_in_memory().unwrap();
    generate_sales_schema(&conn, &dir).unwrap();
    let path = dir.join("orders.parquet");
    let p = path.to_str().unwrap();

    let engine = DuckDbEngine::open_parquet(p).unwrap();
    assert_eq!(engine.row_count(), 100000);

    let page1 = engine.get_page(None, 10_000).unwrap();
    assert_eq!(page1.len(), 10_000);
    let last = page1.last().unwrap().row_id;

    let page2 = engine.get_page(Some(last), 10_000).unwrap();
    assert_eq!(page2.len(), 10_000);
    assert!(page2.first().unwrap().row_id > last);

    // "paid" matches ~20k rows — well past the 5k search cap.
    let (hits, truncated) = engine.search_rows("paid", None).unwrap();
    assert!(!hits.is_empty());
    assert!(truncated, "expected more than the search cap of matches");

    let all = engine.get_all_rows().unwrap();
    assert_eq!(all.len(), 100000);

    // The workspace editing path must mount the same table correctly.
    let mut ws = Workspace::new("large");
    ws.tables.push(ws_table("orders", &path));
    let mut ws_engine = DuckDbEngine::open_workspace(&ws).unwrap();
    ws_engine.open_editor_table("orders", p).unwrap();
    assert_eq!(ws_engine.row_count(), 100000);

    let _ = std::fs::remove_dir_all(&dir);
}

// --- Editing round trip -------------------------------------------------------

fn write_users_parquet(path: &Path) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE u (user_id BIGINT, username VARCHAR);
         INSERT INTO u SELECT i::BIGINT, 'user_' || lpad(i::VARCHAR, 4, '0') FROM range(1, 1002) t(i);
         COPY u TO '{}' (FORMAT PARQUET);",
        to_sql_path(path)
    ))
    .unwrap();
}

#[test]
fn edit_round_trip_persists_and_undo_restores() {
    let tmp = std::env::temp_dir().join(format!("parasql_edit_it_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let copy = tmp.join("users_copy.parquet");
    write_users_parquet(&copy);

    let mut ws = Workspace::new("edit");
    ws.tables.push(ws_table("users_copy", &copy));
    let mut engine = DuckDbEngine::open_workspace(&ws).unwrap();
    engine
        .open_editor_table("users_copy", copy.to_str().unwrap())
        .unwrap();
    assert!(engine.has_editor());
    assert!(!engine.is_dirty());

    let cols = engine.column_info().unwrap();
    let row1 = engine.get_page(None, 1).unwrap()[0].clone();
    assert_eq!(row1.values[1].as_str().unwrap(), "user_0001");

    engine
        .edit_cell(row1.row_id, 1, &serde_json::json!("renamed_user"), &cols)
        .unwrap();
    assert!(engine.is_dirty());
    assert_eq!(
        engine.get_page(None, 1).unwrap()[0].values[1].as_str().unwrap(),
        "renamed_user"
    );

    engine.undo().unwrap();
    assert_eq!(
        engine.get_page(None, 1).unwrap()[0].values[1].as_str().unwrap(),
        "user_0001"
    );
    engine.redo().unwrap();
    assert_eq!(
        engine.get_page(None, 1).unwrap()[0].values[1].as_str().unwrap(),
        "renamed_user"
    );

    // Save (atomic replace) and confirm the edit reached the file.
    engine.export_parquet(copy.to_str().unwrap()).unwrap();
    engine.mark_clean();
    assert!(!engine.is_dirty());

    let mut reopened = DuckDbEngine::open_parquet(copy.to_str().unwrap()).unwrap();
    assert_eq!(
        reopened.get_page(None, 1).unwrap()[0].values[1].as_str().unwrap(),
        "renamed_user"
    );

    // Insert / delete / undo / redo on a fresh editor.
    let inserted = reopened.insert_row().unwrap();
    let iid = inserted.rows[0].row_id;
    assert_eq!(reopened.row_count(), 1002);
    reopened.delete_rows(&[iid]).unwrap();
    assert_eq!(reopened.row_count(), 1001);
    reopened.undo().unwrap();
    assert_eq!(reopened.row_count(), 1002);
    reopened.redo().unwrap();
    assert_eq!(reopened.row_count(), 1001);
    assert!(reopened.is_dirty());

    // Workspace export with explicit compression + metadata verification.
    let zstd = tmp.join("users_zstd.parquet");
    engine
        .export_table("users_copy", zstd.to_str().unwrap(), "ZSTD")
        .unwrap();
    let meta = engine.get_table_meta(zstd.to_str().unwrap()).unwrap();
    assert_eq!(meta["compression"], "ZSTD");
    assert_eq!(meta["rows"], 1001);
    let exported = DuckDbEngine::open_parquet(zstd.to_str().unwrap()).unwrap();
    assert_eq!(exported.row_count(), 1001);
    assert!(engine
        .export_table("users_copy", zstd.to_str().unwrap(), "NOT_A_CODEC")
        .is_err());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn missing_workspace_table_can_be_relinked() {
    let tmp = std::env::temp_dir().join(format!("parasql_relink_it_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let src = tmp.join("users_src.parquet");
    write_users_parquet(&src);
    let ghost = tmp.join("ghost.parquet");

    let mut ws = Workspace::new("relink");
    let mut t = ws_table("ghost", &ghost);
    t.missing = true;
    ws.tables.push(t);

    let mut engine = DuckDbEngine::open_workspace(&ws).unwrap();
    assert!(engine.execute_sql("SELECT * FROM ghost").is_err());

    std::fs::copy(&src, &ghost).unwrap();
    ws.tables[0].missing = false;
    ws.tables[0].abs_path = Some(ghost.to_string_lossy().to_string());
    engine.sync_workspace_tables(&ws).unwrap();
    let q = engine.execute_sql("SELECT COUNT(*) AS n FROM ghost").unwrap();
    assert_eq!(n(&q.rows[0].values[0]), 1001);

    let _ = std::fs::remove_dir_all(&tmp);
}
