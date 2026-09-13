use super::catalog::{SavedQuery, Workspace, WorkspaceTable};
use super::duckdb::DuckDbEngine;
use super::types::QueryResult;
use duckdb::Connection;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../Sample_data")
}

fn parquet_files() -> Vec<PathBuf> {
    let dir = sample_dir();
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "parquet"))
        .collect();
    files.sort();
    files
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

const SAMPLE_TABLES: &[(&str, &str)] = &[
    ("bank_failures", "bank_failures.parquet"),
    ("iris", "iris.parquet"),
    ("sample_empty", "sample-empty.parquet"),
    ("sample_gzip", "sample-gzip.parquet"),
    ("sample_large", "sample-large.parquet"),
    ("sample_nested", "sample-nested.parquet"),
    ("sample_types", "sample-types.parquet"),
    ("sample_uncompressed", "sample-uncompressed.parquet"),
    ("sample_users", "sample-users.parquet"),
    ("search_trends", "search_trends.parquet"),
    ("titanic", "titanic.parquet"),
];

fn sample_workspace() -> Workspace {
    let dir = sample_dir();
    let mut ws = Workspace::new("samples");
    for (name, file) in SAMPLE_TABLES {
        ws.tables.push(ws_table(name, &dir.join(file)));
    }
    ws
}

fn open_samples() -> Option<DuckDbEngine> {
    if parquet_files().is_empty() {
        eprintln!("Sample_data not found; skipping");
        return None;
    }
    Some(DuckDbEngine::open_workspace(&sample_workspace()).expect("sample workspace should open"))
}

fn n(v: &serde_json::Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_i64().map(|i| i as u64))
        .or_else(|| v.as_f64().map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        .unwrap_or_else(|| panic!("not numeric: {:?}", v))
}

fn engine_json_rows(q: &QueryResult) -> Vec<serde_json::Value> {
    q.rows
        .iter()
        .map(|r| {
            let mut m = serde_json::Map::new();
            for (c, v) in q.columns.iter().zip(r.values.iter()) {
                m.insert(c.name.clone(), v.clone());
            }
            serde_json::Value::Object(m)
        })
        .collect()
}

/// Compute the same SQL with plain `read_parquet` views to prove the workspace
/// view layer is transparent (the app's abstraction must not change results).
fn direct_json_rows(dir: &Path, tables: &[(&str, &str)], sql: &str) -> Vec<serde_json::Value> {
    // `to_json` lives in DuckDB's json extension, which auto-installs on first
    // use; serialize access so parallel tests never race the install.
    static DIRECT_DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = DIRECT_DB_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let conn = Connection::open_in_memory().unwrap();
    for (name, file) in tables {
        let path = dir.join(file).to_string_lossy().replace('\\', "/");
        conn.execute_batch(&format!(
            "CREATE VIEW {} AS SELECT * FROM read_parquet('{}')",
            name, path
        ))
        .unwrap();
    }
    let mut stmt = conn
        .prepare(&format!("SELECT to_json(t)::VARCHAR FROM ({}) t", sql))
        .unwrap();
    let rows = stmt
        .query_map([], |row| row.get::<usize, String>(0))
        .unwrap();
    rows.map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect()
}

#[test]
fn sample_data_inventory() {
    let files = parquet_files();
    if files.is_empty() {
        eprintln!("Sample_data not found; skipping");
        return;
    }
    for path in files {
        let p = path.to_str().unwrap();
        let engine = match DuckDbEngine::open_parquet(p) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("\n== {} ==\n  OPEN FAILED: {}", path.display(), err);
                continue;
            }
        };
        let cols = engine.column_info().unwrap();
        let count = engine.row_count();
        let meta = engine
            .get_table_meta(p)
            .map(|v| v.to_string())
            .unwrap_or_else(|e| format!("meta err: {}", e));
        eprintln!(
            "\n== {} ({} rows) ==",
            path.file_name().unwrap().to_string_lossy(),
            count
        );
        for c in &cols {
            eprintln!("   {} : {}", c.name, c.dtype);
        }
        eprintln!("   meta: {}", meta);
    }
}

#[test]
fn query_result_edge_cases() {
    let Some(engine) = open_samples() else {
        return;
    };

    // Narrow integer columns must render as numbers, not "[unsupported type]".
    let q = engine
        .execute_sql("SELECT col_int8, col_int32, col_int64 FROM sample_types ORDER BY col_int32 LIMIT 3")
        .unwrap();
    assert!(
        q.rows.iter().all(|r| r.values[0].is_number()),
        "int8 values: {:?}",
        q.rows[0].values
    );

    // SUM over integers arrives as HUGEINT (Decimal128 scale 0) — stays numeric.
    let s = engine
        .execute_sql("SELECT SUM(col_int64) AS total FROM sample_types")
        .unwrap();
    assert!(s.rows[0].values[0].is_number(), "sum: {:?}", s.rows[0].values[0]);

    // Unicode round trip from the sample data.
    let u = engine
        .execute_sql("SELECT col_string FROM sample_types WHERE col_int32 = 1000")
        .unwrap();
    assert_eq!(u.rows[0].values[0].as_str().unwrap(), "строка-1 文字 1");

    // Duplicate aliases must be unique in the result schema for the grid.
    let dup = engine
        .execute_sql("SELECT 1 AS x, 2 AS x FROM iris LIMIT 1")
        .unwrap();
    let mut names: Vec<String> = dup.columns.iter().map(|c| c.name.clone()).collect();
    let total = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), total, "duplicate column names survived: {:?}", names);

    // Empty result keeps its column shape.
    let empty = engine
        .execute_sql("SELECT col_int32 FROM sample_types WHERE col_int32 > 999999")
        .unwrap();
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns.len(), 1);

    // Narrow types are queryable through the view layer.
    let filtered = engine
        .execute_sql("SELECT COUNT(col_int8) AS n FROM sample_types")
        .unwrap();
    assert!(n(&filtered.rows[0].values[0]) > 0);
}

/// Run once to create `Sample_data/parasql-demo.parasql` for manual app testing:
/// `cargo test --lib generate_sample_workspace -- --ignored`
#[test]
#[ignore = "writes Sample_data/parasql-demo.parasql for manual testing"]
fn generate_sample_workspace() {
    let dir = sample_dir();
    let mut ws = Workspace::new("ParaSQL demo");
    for (name, file) in SAMPLE_TABLES {
        let abs = dir.join(file);
        let mut t = ws_table(name, &abs);
        t.size = std::fs::metadata(&abs).ok().map(|m| m.len());
        ws.tables.push(t);
    }
    let sales = [
        ("products", "products.parquet"),
        ("customers", "customers.parquet"),
        ("orders", "orders.parquet"),
        ("order_items", "order_items.parquet"),
    ];
    for (name, file) in sales {
        let p = dir.join("sales").join(file);
        if p.exists() {
            ws.tables.push(ws_table(name, &p));
        }
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
              ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue \
              FROM orders o JOIN order_items oi ON oi.order_id = o.order_id \
              JOIN products p ON p.product_id = oi.product_id GROUP BY 1 ORDER BY 1"
            .into(),
    });
    ws.saved_queries.push(SavedQuery {
        name: "Titanic survival by class".into(),
        sql: "SELECT Pclass, COUNT(*) AS total, \
              SUM(CASE WHEN Survived = 1 THEN 1 ELSE 0 END) AS survived \
              FROM titanic GROUP BY Pclass ORDER BY Pclass"
            .into(),
    });
    let path = dir.join("parasql-demo.parasql");
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
    assert!(ws.tables.len() >= 15, "tables: {}", ws.tables.len());
    assert!(ws.saved_queries.len() >= 3);
    let engine = DuckDbEngine::open_workspace(&ws).unwrap();
    let q = engine
        .execute_sql("SELECT COUNT(*) AS n FROM order_items")
        .unwrap();
    assert_eq!(n(&q.rows[0].values[0]), 300000);
    let t = engine.execute_sql("SELECT COUNT(*) AS n FROM titanic").unwrap();
    assert_eq!(n(&t.rows[0].values[0]), 891);
}

/// Prints the demo queries and their actual result rows:
/// `cargo test --lib showcase_queries -- --ignored --nocapture`
#[test]
#[ignore = "prints demo query results"]
fn showcase_queries() {
    let sales = std::env::temp_dir().join(format!("parasql_showcase_{}", std::process::id()));
    let conn = Connection::open_in_memory().unwrap();
    generate_sales_schema(&conn, &sales).unwrap();

    let dir = sample_dir();
    let mut ws = Workspace::new("showcase");
    for (name, file) in [
        ("products", "products.parquet"),
        ("customers", "customers.parquet"),
        ("orders", "orders.parquet"),
        ("order_items", "order_items.parquet"),
    ] {
        ws.tables.push(ws_table(name, &sales.join(file)));
    }
    for (name, file) in SAMPLE_TABLES {
        ws.tables.push(ws_table(name, &dir.join(file)));
    }
    let engine = DuckDbEngine::open_workspace(&ws).unwrap();

    let queries = [
        (
            "Revenue by category (4-table join)",
            "SELECT p.category, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue \
             FROM order_items oi JOIN products p ON p.product_id = oi.product_id \
             GROUP BY 1 ORDER BY 2 DESC",
        ),
        (
            "Monthly revenue (first 6 months)",
            "SELECT strftime(o.order_date, '%Y-%m') AS month, COUNT(*) AS orders, \
             ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue \
             FROM orders o JOIN order_items oi ON oi.order_id = o.order_id \
             JOIN products p ON p.product_id = oi.product_id GROUP BY 1 ORDER BY 1 LIMIT 6",
        ),
        (
            "Titanic survival by class (pivot)",
            "SELECT Pclass, COUNT(*) AS total, \
             SUM(CASE WHEN Survived = 1 THEN 1 ELSE 0 END) AS survived \
             FROM titanic GROUP BY Pclass ORDER BY Pclass",
        ),
        (
            "Iris species averages",
            "SELECT variety, COUNT(*) AS n, ROUND(AVG(\"petal.length\"), 3) AS avg_petal \
             FROM iris GROUP BY variety ORDER BY variety",
        ),
    ];
    for (title, sql) in queries {
        let q = engine.execute_sql(sql).unwrap();
        eprintln!("\n=== {} ===", title);
        eprintln!(
            "{}",
            q.columns
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
                .join(" | ")
        );
        for r in &q.rows {
            eprintln!(
                "{}",
                r.values
                    .iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
    }
    let _ = std::fs::remove_dir_all(&sales);
}

#[test]
fn sample_workspace_supports_cross_file_queries() {
    let Some(engine) = open_samples() else {
        return;
    };

    // Exact row counts through the workspace views.
    let expected_counts: &[(&str, u64)] = &[
        ("bank_failures", 545),
        ("iris", 150),
        ("sample_empty", 0),
        ("sample_gzip", 20000),
        ("sample_large", 500000),
        ("sample_nested", 4),
        ("sample_types", 8),
        ("sample_uncompressed", 20000),
        ("sample_users", 1001),
        ("search_trends", 49),
        ("titanic", 891),
    ];
    for (name, expected) in expected_counts {
        let q = engine
            .execute_sql(&format!("SELECT COUNT(*) AS n FROM \"{}\"", name))
            .unwrap();
        assert_eq!(n(&q.rows[0].values[0]), *expected, "row count for {}", name);
    }

    // Multi-file UNION ALL across two physical files.
    let union = engine
        .execute_sql(
            "SELECT COUNT(*) AS n FROM (SELECT * FROM sample_gzip UNION ALL SELECT * FROM sample_uncompressed)",
        )
        .unwrap();
    assert_eq!(n(&union.rows[0].values[0]), 40000);

    // Titanic pivot: totals and survival by class (classic known values).
    let pivot = engine
        .execute_sql(
            "SELECT Pclass, COUNT(*) AS total, SUM(CASE WHEN Survived = 1 THEN 1 ELSE 0 END) AS survived \
             FROM titanic GROUP BY Pclass ORDER BY Pclass",
        )
        .unwrap();
    assert_eq!(pivot.rows.len(), 3);
    let totals: Vec<u64> = pivot.rows.iter().map(|r| n(&r.values[1])).collect();
    let survived: Vec<u64> = pivot.rows.iter().map(|r| n(&r.values[2])).collect();
    assert_eq!(totals, vec![216, 184, 491]);
    assert_eq!(survived, vec![136, 87, 119]);

    // Window function + QUALIFY-style top-N per group.
    let top = engine
        .execute_sql(
            "SELECT Pclass, Name, Fare FROM (SELECT *, ROW_NUMBER() OVER (PARTITION BY Pclass ORDER BY Fare DESC) AS rn FROM titanic) t WHERE rn = 1 ORDER BY Pclass",
        )
        .unwrap();
    assert_eq!(top.rows.len(), 3);

    // CTE over iris; species separation sanity.
    let iris = engine
        .execute_sql(
            "WITH species AS (SELECT variety, COUNT(*) AS n, AVG(\"petal.length\") AS avg_petal FROM iris GROUP BY variety) \
             SELECT * FROM species ORDER BY variety",
        )
        .unwrap();
    assert_eq!(iris.rows.len(), 3);
    let avg_petals: Vec<f64> = iris
        .rows
        .iter()
        .map(|r| r.values[2].as_f64().unwrap())
        .collect();
    assert!(avg_petals[0] < 2.0, "setosa avg petal {:?}", avg_petals);
    assert!(avg_petals[1] > 2.5 && avg_petals[1] < 5.0);
    assert!(avg_petals[2] > 5.0);

    // Quoted identifiers with spaces, parens and dollar signs.
    let assets = engine
        .execute_sql(
            "SELECT Bank, \"Assets ($mil.)\" FROM bank_failures WHERE \"Assets ($mil.)\" > 1000 ORDER BY \"Assets ($mil.)\" DESC LIMIT 5",
        )
        .unwrap();
    assert_eq!(assets.rows.len(), 5);
    let peak = engine
        .execute_sql("SELECT MAX(\"Matthew Perry\") AS peak FROM search_trends")
        .unwrap();
    assert!(n(&peak.rows[0].values[0]) > 0);

    // Nested types via UNNEST.
    let items = engine
        .execute_sql("SELECT order_id, UNNEST(items) AS item FROM sample_nested")
        .unwrap();
    assert!(items.rows.len() >= 6);
    assert!(items
        .rows
        .iter()
        .any(|r| r.values[1].as_str() == Some("book")));

    // 500k view: aggregation is fine, unbounded SELECT is rejected by the cap.
    let agg = engine
        .execute_sql(
            "SELECT category, COUNT(*) AS n FROM sample_large GROUP BY category ORDER BY category",
        )
        .unwrap();
    let total: u64 = agg.rows.iter().map(|r| n(&r.values[1])).sum();
    assert_eq!(total, 500000);
    let err = engine.execute_sql("SELECT * FROM sample_large").unwrap_err();
    assert!(err.contains("more than"), "unexpected error: {}", err);

    // DML through the query box stays rejected.
    let err = engine.execute_sql("DELETE FROM iris").unwrap_err();
    assert!(err.contains("Only SELECT"), "unexpected error: {}", err);
}

#[test]
fn workspace_views_match_direct_parquet_reads() {
    let Some(engine) = open_samples() else {
        return;
    };
    // Sanity checks that both paths can see the two comparable files.
    let queries = [
        "SELECT COUNT(*) AS n FROM (SELECT * FROM sample_gzip UNION ALL SELECT * FROM sample_uncompressed)",
        "SELECT ROUND(AVG(value), 6) AS avg_value, COUNT(*) AS n FROM sample_large",
        "SELECT variety, COUNT(*) AS n FROM iris GROUP BY variety ORDER BY variety",
    ];
    for sql in queries {
        let q = engine.execute_sql(sql).unwrap();
        let direct = direct_json_rows(
            &sample_dir(),
            &[
                ("sample_gzip", "sample-gzip.parquet"),
                ("sample_uncompressed", "sample-uncompressed.parquet"),
                ("sample_large", "sample-large.parquet"),
                ("iris", "iris.parquet"),
            ],
            sql,
        );
        assert_eq!(engine_json_rows(&q), direct, "query diverged: {}", sql);
    }
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
    let d = dir.to_string_lossy().replace('\\', "/").replace('\'', "''");
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

/// Run once to create `Sample_data/sales/*.parquet` for manual app testing:
/// `cargo test --lib generate_sales_sample_files -- --ignored`
#[test]
#[ignore = "writes Sample_data/sales/*.parquet for manual testing"]
fn generate_sales_sample_files() {
    let dir = sample_dir().join("sales");
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
    let t0 = Instant::now();
    for sql in queries {
        let q = engine.execute_sql(sql).unwrap();
        let direct = direct_json_rows(&dir, &tables, sql);
        assert_eq!(engine_json_rows(&q), direct, "query diverged: {}", sql);
    }
    eprintln!("5 complex multi-file queries in {:?}", t0.elapsed());

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
    let path = sample_dir().join("sample-large.parquet");
    if !path.exists() {
        eprintln!("sample-large.parquet missing; skipping");
        return;
    }
    let p = path.to_str().unwrap();

    let t = Instant::now();
    let engine = DuckDbEngine::open_parquet(p).unwrap();
    eprintln!("open_parquet 500k rows: {:?}", t.elapsed());
    assert_eq!(engine.row_count(), 500000);

    let t = Instant::now();
    let page1 = engine.get_page(None, 10_000).unwrap();
    eprintln!("first keyset page (10k rows): {:?}", t.elapsed());
    assert_eq!(page1.len(), 10_000);
    let last = page1.last().unwrap().row_id;

    let t = Instant::now();
    let page2 = engine.get_page(Some(last), 10_000).unwrap();
    eprintln!("next keyset page (10k rows): {:?}", t.elapsed());
    assert_eq!(page2.len(), 10_000);
    assert!(page2.first().unwrap().row_id > last);

    let t = Instant::now();
    let (hits, truncated) = engine.search_rows("A", None).unwrap();
    eprintln!(
        "search across 500k rows: {:?}, {} hits, truncated={}",
        t.elapsed(),
        hits.len(),
        truncated
    );
    assert!(truncated, "expected more than the search cap of matches");

    // The frontend's `showEditorDataFlow` pulls all rows on editor open.
    let t = Instant::now();
    let all = engine.get_all_rows().unwrap();
    let json_len = serde_json::to_string(&all).unwrap().len();
    eprintln!(
        "get_all_rows 500k: {:?}, JSON payload {:.1} MB",
        t.elapsed(),
        json_len as f64 / 1_048_576.0
    );
    assert_eq!(all.len(), 500000);

    // Editing flow must not be required for the workspace view path.
    let mut ws = Workspace::new("large");
    ws.tables.push(ws_table("sample_large", &path));
    let mut ws_engine = DuckDbEngine::open_workspace(&ws).unwrap();
    let t = Instant::now();
    ws_engine
        .open_editor_table("sample_large", p)
        .unwrap();
    eprintln!("workspace open_editor_table 500k: {:?}", t.elapsed());
    assert_eq!(ws_engine.row_count(), 500000);
}

// --- Editing round trip on a copy --------------------------------------------

#[test]
fn edit_round_trip_persists_and_undo_restores() {
    let src = sample_dir().join("sample-users.parquet");
    if !src.exists() {
        eprintln!("sample-users.parquet missing; skipping");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("parasql_edit_it_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let copy = tmp.join("users_copy.parquet");
    std::fs::copy(&src, &copy).unwrap();

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
    let src = sample_dir().join("iris.parquet");
    if !src.exists() {
        eprintln!("iris.parquet missing; skipping");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("parasql_relink_it_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
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
    assert_eq!(n(&q.rows[0].values[0]), 150);

    let _ = std::fs::remove_dir_all(&tmp);
}
