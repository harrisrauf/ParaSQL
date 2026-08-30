use arrow::array::*;
use arrow::record_batch::RecordBatch;
use chrono::{Duration as ChronoDuration, NaiveDate, NaiveDateTime};
use duckdb::types::ToSql;
use duckdb::Connection;
use serde_json::Value as JsonValue;

use super::catalog;
use super::types::*;

// --- DuckDB Engine ---

pub struct DuckDbEngine {
    conn: Connection,
    table_name: String,
    page_size: usize,
    file_metadata: Option<FileMetadata>,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
    /// Workspace view names we created (for safe teardown)
    created_views: Vec<String>,
}

/// Max undo steps kept in memory. Older entries are evicted (with cleanup).
const UNDO_LIMIT: usize = 500;

/// Index on _row_id. Dropped around column ALTERs because DuckDB blocks
/// ALTER TABLE while catalog entries depend on it.
const ROW_ID_INDEX: &str = "idx_working_rowid";

/// A recorded mutation: `redo_sql` re-applies it, `undo_sql` reverses it.
/// DuckDB has no SAVEPOINT support, so undo is implemented by executing the
/// inverse statement (computed at mutation time).
struct UndoEntry {
    redo_sql: String,
    undo_sql: String,
    /// Runs when the entry is evicted without being undone (e.g. undo-stack cap).
    cleanup_sql: Option<String>,
}

impl DuckDbEngine {
    pub fn open_parquet(path: &str) -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create DuckDB connection: {}", e))?;

        // Load parquet file into a view
        let escaped = path.replace('\'', "''");
        conn.execute_batch(&format!(
            "CREATE OR REPLACE VIEW raw_parquet AS SELECT * FROM read_parquet('{}')",
            escaped
        ))
        .map_err(|e| format!("Failed to load parquet file: {}", e))?;

        // Create working table with stable _row_id
        conn.execute_batch(
            "CREATE OR REPLACE TABLE working AS \
             SELECT row_number() OVER () AS _row_id, t.* FROM raw_parquet t"
        )
        .map_err(|e| format!("Failed to create working table: {}", e))?;

        // Index _row_id: point edits/deletes/pagination become O(log n)
        conn.execute_batch("CREATE INDEX idx_working_rowid ON working(_row_id)")
            .map_err(|e| format!("Failed to index _row_id: {}", e))?;

        let row_count: i64 = conn
            .query_row("SELECT count(*) FROM working", [], |row| row.get(0))
            .map_err(|e| format!("Failed to count rows: {}", e))?;

        Ok(Self {
            conn,
            table_name: "working".to_string(),
            page_size: 500,
            file_metadata: Some(FileMetadata {
                path: Some(std::path::PathBuf::from(path)),
                compression: CompressionPreset::SNAPPY,
                row_group_size: row_count as usize,
                data_page_size: None,
            }),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            created_views: Vec::new(),
        })
    }

    /// Open a folder containing multiple Parquet files (partitioned dataset)
    pub fn open_folder(folder_path: &str) -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create DuckDB connection: {}", e))?;

        // Use DuckDB's glob to read all parquet files in the folder
        let glob_pattern = format!("{}/**/*.parquet", folder_path.replace('\'', "''"));
        conn.execute_batch(&format!(
            "CREATE OR REPLACE VIEW raw_parquet AS SELECT * FROM read_parquet('{}')",
            glob_pattern
        ))
        .map_err(|e| format!("Failed to load parquet files from folder: {}", e))?;

        // Create working table with stable _row_id
        conn.execute_batch(
            "CREATE OR REPLACE TABLE working AS \
             SELECT row_number() OVER () AS _row_id, t.* FROM raw_parquet t"
        )
        .map_err(|e| format!("Failed to create working table: {}", e))?;

        // Index _row_id: point edits/deletes/pagination become O(log n)
        conn.execute_batch("CREATE INDEX idx_working_rowid ON working(_row_id)")
            .map_err(|e| format!("Failed to index _row_id: {}", e))?;

        let row_count: i64 = conn
            .query_row("SELECT count(*) FROM working", [], |row| row.get(0))
            .map_err(|e| format!("Failed to count rows: {}", e))?;

        Ok(Self {
            conn,
            table_name: "working".to_string(),
            page_size: 500,
            file_metadata: Some(FileMetadata {
                path: Some(std::path::PathBuf::from(folder_path)),
                compression: CompressionPreset::SNAPPY,
                row_group_size: row_count as usize,
                data_page_size: None,
            }),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            created_views: Vec::new(),
        })
    }

    /// Wrap ALTER statements so the _row_id index doesn't block them.
    /// DuckDB refuses ALTER TABLE while catalog entries depend on the table.
    fn alter_without_index(&self, stmts: &str) -> String {
        format!(
            "DROP INDEX IF EXISTS {}; {}; CREATE INDEX IF NOT EXISTS {} ON {}(_row_id)",
            ROW_ID_INDEX, stmts, ROW_ID_INDEX, self.table_name
        )
    }

    /// Record a mutation for undo/redo. `redo_sql` re-applies it, `undo_sql`
    /// reverses it. New mutations invalidate the redo stack. The stack is
    /// capped at UNDO_LIMIT; evicted entries run their cleanup SQL.
    fn push_undo(&mut self, redo_sql: String, undo_sql: String, cleanup_sql: Option<String>) {
        self.redo_stack.clear(); // New action invalidates redo history
        if self.undo_stack.len() >= UNDO_LIMIT {
            let evicted = self.undo_stack.remove(0);
            if let Some(cleanup) = evicted.cleanup_sql {
                let _ = self.conn.execute_batch(&cleanup);
            }
        }
        self.undo_stack.push(UndoEntry {
            redo_sql,
            undo_sql,
            cleanup_sql,
        });
    }

    pub fn row_count(&self) -> usize {
        let count: i64 = self.conn
            .query_row(&format!("SELECT count(*) FROM {}", self.table_name), [], |row| row.get(0))
            .unwrap_or(0);
        count as usize
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn set_page_size(&mut self, size: usize) {
        self.page_size = size;
    }

    pub fn file_metadata(&self) -> Option<&FileMetadata> {
        self.file_metadata.as_ref()
    }

    pub fn file_path(&self) -> Option<&std::path::Path> {
        self.file_metadata.as_ref().and_then(|m| m.path.as_deref())
    }

    /// Update the file path (e.g. after Save As)
    pub fn set_file_path(&mut self, path: &str) {
        if let Some(md) = &mut self.file_metadata {
            md.path = Some(std::path::PathBuf::from(path));
        } else {
            self.file_metadata = Some(FileMetadata {
                path: Some(std::path::PathBuf::from(path)),
                ..Default::default()
            });
        }
    }

    /// Get column info from DuckDB table
    pub fn column_info(&self) -> Result<Vec<ColumnInfo>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "PRAGMA table_info('{}')",
                self.table_name
            ))
            .map_err(|e| format!("Failed to prepare PRAGMA: {}", e))?;

        let mut columns = Vec::new();
        let rows = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let dtype: String = row.get(2)?;
                let not_null: bool = row.get(3)?;
                Ok((name, dtype, not_null))
            })
            .map_err(|e| format!("Failed to get table info: {}", e))?;

        for row in rows {
            let (name, dtype, not_null) = row.map_err(|e| format!("Row error: {}", e))?;
            // Skip the _row_id column from display
            if name == "_row_id" {
                continue;
            }
            columns.push(ColumnInfo {
                name,
                dtype,
                nullable: !not_null,
            });
        }

        Ok(columns)
    }

    /// Get a page of rows as JSON
    pub fn get_page(&self, offset: usize, limit: usize) -> Result<Vec<RowData>, String> {
        let sql = format!(
            "SELECT * FROM {} ORDER BY _row_id LIMIT {} OFFSET {}",
            self.table_name, limit, offset
        );
        let batch = self.query_to_batch(&sql)?;
        rows_from_batch(&batch, offset)
    }

    /// Get all rows (for small datasets)
    pub fn get_all_rows(&self) -> Result<Vec<RowData>, String> {
        let sql = format!("SELECT * FROM {} ORDER BY _row_id", self.table_name);
        let batch = self.query_to_batch(&sql)?;
        rows_from_batch(&batch, 0)
    }

    const SEARCH_LIMIT: usize = 5000;

    /// Whole-table case-insensitive search across all columns (or one column).
    /// Matches are scanned by DuckDB (not by the frontend), so unloaded rows
    /// are searchable. Returns (rows, truncated) — truncated is true when more
    /// than SEARCH_LIMIT rows matched.
    pub fn search_rows(
        &self,
        query: &str,
        column: Option<&str>,
    ) -> Result<(Vec<RowData>, bool), String> {
        let columns = self.column_info()?;
        if columns.is_empty() {
            return Ok((Vec::new(), false));
        }

        let pattern = format!("%{}%", escape_like(query));
        // CAST to VARCHAR: ILIKE only accepts string operands (e.g. BIGINT columns)
        let (cond, params): (String, Vec<&dyn ToSql>) = match column {
            Some(name) => (
                format!(
                    "CAST({} AS VARCHAR) ILIKE ? ESCAPE '\\'",
                    quote_ident(name)
                ),
                vec![&pattern],
            ),
            None => (
                columns
                    .iter()
                    .map(|c| {
                        format!(
                            "CAST({} AS VARCHAR) ILIKE ? ESCAPE '\\'",
                            quote_ident(&c.name)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" OR "),
                vec![&pattern; columns.len()],
            ),
        };

        let sql = format!(
            "SELECT * FROM {} WHERE {} ORDER BY _row_id LIMIT {}",
            self.table_name,
            cond,
            Self::SEARCH_LIMIT + 1
        );
        let batch = self.query_to_batch_params(&sql, &params)?;
        let mut rows = rows_from_batch(&batch, 0)?;
        let truncated = rows.len() > Self::SEARCH_LIMIT;
        if truncated {
            rows.truncate(Self::SEARCH_LIMIT);
        }
        Ok((rows, truncated))
    }

    /// Execute a raw SQL query and return results as rows
    pub fn execute_sql(&self, sql: &str) -> Result<QueryResult, String> {
        let batch = self.query_to_batch(sql)?;
        let schema = batch.schema();
        let fields = schema.fields();
        let skip = if !fields.is_empty() && fields[0].name() == "_row_id" { 1 } else { 0 };
        let columns = fields
            .iter()
            .enumerate()
            .filter(|(i, _)| *i >= skip)
            .map(|(_, f)| ColumnInfo {
                name: f.name().clone(),
                dtype: format!("{:?}", f.data_type()),
                nullable: f.is_nullable(),
            })
            .collect();
        let rows = rows_from_batch(&batch, 0)?;
        Ok(QueryResult { columns, rows })
    }

    /// Edit a cell value
    pub fn edit_cell(&mut self, row_id: u64, col_idx: usize, value: &JsonValue, columns: &[ColumnInfo]) -> Result<QueryResult, String> {
        let col = columns
            .get(col_idx)
            .ok_or_else(|| format!("Invalid column index: {}", col_idx))?;
        let col_ident = quote_ident(&col.name);

        // Capture the current value so undo can restore it
        let select_sql = format!(
            "SELECT * FROM {} WHERE _row_id = {}",
            self.table_name, row_id
        );
        let batch = self.query_to_batch(&select_sql)?;
        let existing = rows_from_batch(&batch, 0)?;
        let old_value = existing
            .first()
            .and_then(|r| r.values.get(col_idx).cloned())
            .unwrap_or(JsonValue::Null);

        let new_literal = json_to_literal(value)?;
        let old_literal = json_to_literal(&old_value)?;

        let sql = format!(
            "UPDATE {} SET {} = {} WHERE _row_id = {}",
            self.table_name, col_ident, new_literal, row_id
        );
        let inverse = format!(
            "UPDATE {} SET {} = {} WHERE _row_id = {}",
            self.table_name, col_ident, old_literal, row_id
        );

        self.push_undo(sql.clone(), inverse, None);

        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to update cell: {}", e))?;

        // Return the updated row
        let row_sql = format!(
            "SELECT * FROM {} WHERE _row_id = {}",
            self.table_name, row_id
        );
        let batch = self.query_to_batch(&row_sql)?;
        let rows = rows_from_batch(&batch, 0)?;
        Ok(QueryResult {
            columns: self.column_info()?,
            rows,
        })
    }

    /// Delete rows by IDs
    pub fn delete_rows(&mut self, row_ids: &[u64]) -> Result<(), String> {
        if row_ids.is_empty() {
            return Ok(());
        }

        let ids_str: Vec<String> = row_ids.iter().map(|id| id.to_string()).collect();
        let sql = format!(
            "DELETE FROM {} WHERE _row_id IN ({})",
            self.table_name,
            ids_str.join(",")
        );

        // Capture the rows before deletion so undo can restore them
        let select_sql = format!(
            "SELECT * FROM {} WHERE _row_id IN ({})",
            self.table_name,
            ids_str.join(",")
        );
        let batch = self.query_to_batch(&select_sql)?;
        let doomed = rows_from_batch(&batch, 0)?;

        let columns = self.column_info()?;
        let col_idents: Vec<String> = columns.iter().map(|c| quote_ident(&c.name)).collect();
        let mut values_sql: Vec<String> = Vec::new();
        for r in &doomed {
            let literals: Result<Vec<String>, String> = r
                .values
                .iter()
                .map(json_to_literal)
                .collect();
            let literals = literals?;
            values_sql.push(format!(
                "({}, {})",
                r.row_id,
                literals.join(", ")
            ));
        }
        let inverse = format!(
            "INSERT INTO {} (_row_id, {}) VALUES {}",
            self.table_name,
            col_idents.join(", "),
            values_sql.join(", ")
        );

        self.push_undo(sql.clone(), inverse, None);

        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to delete rows: {}", e))?;
        Ok(())
    }

    /// Insert a new row (returns the new row)
    pub fn insert_row(&mut self) -> Result<QueryResult, String> {
        // Get column names and types (excluding _row_id which is auto-generated)
        let columns = self.column_info()?;
        if columns.is_empty() {
            return Err("No columns to insert".to_string());
        }

        let col_idents: Vec<String> = columns.iter().map(|c| quote_ident(&c.name)).collect();

        // Pattern-aware defaults: peek at the last few rows of each column and
        // continue the observed sequence when a pattern is confirmed; otherwise
        // NULL (or the plain type default for NOT NULL columns). O(1) per
        // column — no full-column scans.
        let mut defaults = Vec::with_capacity(columns.len());
        for c in &columns {
            defaults.push(self.next_default(&c.name, &c.dtype, c.nullable)?);
        }

        // Materialize the row_id now so undo/redo are exact and deterministic
        let row_id_sql = format!(
            "SELECT COALESCE(MAX(_row_id), 0) + 1 FROM {}",
            self.table_name
        );
        let new_row_id: i64 = self
            .conn
            .query_row(&row_id_sql, [], |row| row.get(0))
            .map_err(|e| format!("Failed to compute new row id: {}", e))?;

        let sql = format!(
            "INSERT INTO {} (_row_id, {}) VALUES ({}, {})",
            self.table_name,
            col_idents.join(", "),
            new_row_id,
            defaults.join(", ")
        );
        let inverse = format!(
            "DELETE FROM {} WHERE _row_id = {}",
            self.table_name, new_row_id
        );

        self.push_undo(sql.clone(), inverse, None);

        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to insert row: {}", e))?;

        // Fetch the inserted row (no RETURNING dependency)
        let row_sql = format!(
            "SELECT * FROM {} WHERE _row_id = {}",
            self.table_name, new_row_id
        );
        let batch = self.query_to_batch(&row_sql)?;
        let rows = rows_from_batch(&batch, 0)?;
        Ok(QueryResult {
            columns: self.column_info()?,
            rows,
        })
    }

    /// Last `n` values of a column, newest first (used for pattern detection)
    fn last_values(&self, col: &str, n: usize) -> Result<Vec<JsonValue>, String> {
        let sql = format!(
            "SELECT {} FROM {} ORDER BY _row_id DESC LIMIT {}",
            quote_ident(col),
            self.table_name,
            n
        );
        let batch = self.query_to_batch(&sql)?;
        let rows = rows_from_batch(&batch, 0)?;
        Ok(rows
            .into_iter()
            .map(|r| r.values.into_iter().next().unwrap_or(JsonValue::Null))
            .collect())
    }

    /// Compute a pattern-aware SQL default literal for one column. Only fills
    /// when a pattern is confirmed by the window; otherwise NULL (or the plain
    /// type default when the column is NOT NULL).
    fn next_default(&self, name: &str, dtype: &str, nullable: bool) -> Result<String, String> {
        let last8 = self.last_values(name, 8)?;
        let t = dtype.to_lowercase();
        let lit = if is_int_dtype(&t) {
            next_int_default(&last8)
        } else if is_float_dtype(&t) {
            next_float_default(&last8)
        } else if t.starts_with("date") {
            next_date_default(&last8)
        } else if t.starts_with("timestamp") {
            next_timestamp_default(&last8)
        } else if t == "boolean" || t == "bool" {
            let bools: Vec<bool> = last8.iter().filter_map(|v| v.as_bool()).collect();
            if !bools.is_empty() && bools.iter().all(|&b| b == bools[0]) {
                Some(if bools[0] { "TRUE" } else { "FALSE" }.to_string())
            } else {
                None
            }
        } else if is_string_dtype(&t) {
            next_string_default(&last8)
        } else {
            None
        };
        let lit = match lit {
            Some(l) => l,
            None if nullable => "NULL".to_string(),
            None => {
                // NOT NULL column: fall back to the plain type default
                if is_int_dtype(&t) {
                    "0".to_string()
                } else if is_float_dtype(&t) {
                    "0.0".to_string()
                } else if t == "boolean" || t == "bool" {
                    "FALSE".to_string()
                } else if is_string_dtype(&t) {
                    "''".to_string()
                } else if t.starts_with("date") {
                    "'1970-01-01'::DATE".to_string()
                } else if t.starts_with("timestamp") {
                    "'1970-01-01 00:00:00'::TIMESTAMP".to_string()
                } else {
                    "NULL".to_string()
                }
            }
        };
        Ok(lit)
    }

    /// Add a new column
    pub fn add_column(&mut self, name: &str, dtype: &str) -> Result<(), String> {
        let duckdb_type = match dtype.to_lowercase().as_str() {
            "utf8" | "string" | "text" => "VARCHAR",
            "int64" | "int" | "integer" | "bigint" => "BIGINT",
            "int32" => "INTEGER",
            "float64" | "float" | "double" => "DOUBLE",
            "float32" => "FLOAT",
            "bool" | "boolean" => "BOOLEAN",
            _ => "VARCHAR",
        };

        // Backfill existing rows with the type's default value
        let default_val = match duckdb_type {
            "VARCHAR" => "''".to_string(),
            "BOOLEAN" => "FALSE".to_string(),
            _ => "NULL".to_string(),
        };
        let sql = format!(
            "ALTER TABLE {} ADD COLUMN {} {} DEFAULT {}",
            self.table_name,
            quote_ident(name),
            duckdb_type,
            default_val
        );
        let inverse = self.alter_without_index(&format!(
            "ALTER TABLE {} DROP COLUMN {}",
            self.table_name,
            quote_ident(name)
        ));

        self.push_undo(sql.clone(), inverse, None);

        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to add column: {}", e))?;
        Ok(())
    }

    /// Drop a column. Values are captured at drop time and embedded in the
    /// inverse statement as a literal VALUES list, so undo restores them
    /// without any auxiliary table (DuckDB blocks ALTER TABLE while catalog
    /// entries depend on it; the _row_id index is temporarily dropped around
    /// the ALTER to avoid that).
    pub fn drop_column(&mut self, name: &str) -> Result<(), String> {
        let columns = self.column_info()?;
        if columns.len() <= 1 {
            return Err("Cannot drop the last column".to_string());
        }
        if !columns.iter().any(|c| c.name == name) {
            return Err(format!("Column '{}' does not exist", name));
        }

        let col_ident = quote_ident(name);
        let dtype = columns
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.dtype.clone())
            .unwrap_or_else(|| "VARCHAR".to_string());

        // Capture the column values so undo can restore them exactly
        let capture_sql = format!(
            "SELECT _row_id, {} FROM {}",
            col_ident, self.table_name
        );
        let batch = self.query_to_batch(&capture_sql)?;
        let captured = rows_from_batch(&batch, 0)?;

        let mut values_sql: Vec<String> = Vec::new();
        for r in &captured {
            let lit = r
                .values
                .first()
                .map(json_to_literal)
                .unwrap_or_else(|| Ok("NULL".to_string()))?;
            values_sql.push(format!("({}, {})", r.row_id, lit));
        }

        let sql = format!(
            "DROP INDEX IF EXISTS {}; \
             ALTER TABLE {} DROP COLUMN {}; \
             CREATE INDEX IF NOT EXISTS {} ON {}(_row_id)",
            ROW_ID_INDEX, self.table_name, col_ident, ROW_ID_INDEX, self.table_name
        );

        let mut inverse = format!(
            "DROP INDEX IF EXISTS {}; \
             ALTER TABLE {} ADD COLUMN {} {};",
            ROW_ID_INDEX, self.table_name, col_ident, dtype
        );
        if !values_sql.is_empty() {
            inverse.push_str(&format!(
                " UPDATE {} SET {} = v.{} FROM (VALUES {}) AS v(_row_id, {}) WHERE {}._row_id = v._row_id;",
                self.table_name, col_ident, col_ident, values_sql.join(", "), col_ident, self.table_name
            ));
        }
        inverse.push_str(&format!(
            " CREATE INDEX IF NOT EXISTS {} ON {}(_row_id);",
            ROW_ID_INDEX, self.table_name
        ));

        self.push_undo(sql.clone(), inverse, None);

        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to drop column: {}", e))?;
        Ok(())
    }

    /// Rename a column
    pub fn rename_column(&mut self, old_name: &str, new_name: &str) -> Result<(), String> {
        let forward = self.alter_without_index(&format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            self.table_name,
            quote_ident(old_name),
            quote_ident(new_name)
        ));
        let inverse = self.alter_without_index(&format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            self.table_name,
            quote_ident(new_name),
            quote_ident(old_name)
        ));

        self.push_undo(forward.clone(), inverse, None);

        self.conn
            .execute_batch(&forward)
            .map_err(|e| format!("Failed to rename column: {}", e))?;
        Ok(())
    }

    /// Get schema as CREATE TABLE statement in the requested SQL dialect
    pub fn generate_schema_sql(&self, dialect: &str) -> Result<String, String> {
        let columns = self.column_info()?;
        let col_defs: Vec<String> = columns
            .iter()
            .map(|c| {
                let nullable = if c.nullable { "" } else { " NOT NULL" };
                format!("  {} {}{}", quote_ident(&c.name), map_type(&c.dtype, dialect), nullable)
            })
            .collect();

        Ok(format!(
            "CREATE TABLE {} (\n{}\n);",
            self.table_name,
            col_defs.join(",\n")
        ))
    }

    /// Sort rows by a column
    pub fn sort_by(&self, col_name: &str, ascending: bool) -> Result<Vec<RowData>, String> {
        let dir = if ascending { "ASC" } else { "DESC" };
        let escaped = col_name.replace('"', "\"\"");
        let sql = format!(
            "SELECT * FROM {} ORDER BY \"{}\" {} NULLS LAST",
            self.table_name, escaped, dir
        );
        let batch = self.query_to_batch(&sql)?;
        rows_from_batch(&batch, 0)
    }

    /// Build a SELECT that excludes the internal _row_id column, preserving row order
    fn export_select(&self) -> Result<String, String> {
        let columns = self.column_info()?;
        if columns.is_empty() {
            return Ok(format!("SELECT * FROM {} ORDER BY _row_id", self.table_name));
        }
        let cols: Vec<String> = columns
            .iter()
            .map(|c| format!("\"{}\"", c.name.replace('"', "\"\"")))
            .collect();
        Ok(format!(
            "SELECT {} FROM {} ORDER BY _row_id",
            cols.join(", "),
            self.table_name
        ))
    }

    /// Export to Parquet (native save format)
    pub fn export_parquet(&self, path: &str) -> Result<(), String> {
        let escaped = path.replace('\'', "''");
        let sql = format!(
            "COPY ({}) TO '{}' (FORMAT PARQUET)",
            self.export_select()?,
            escaped
        );
        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to export parquet: {}", e))?;
        Ok(())
    }

    /// Export to JSON
    pub fn export_json(&self, path: &str) -> Result<(), String> {
        let escaped = path.replace('\'', "''");
        let sql = format!(
            "COPY ({}) TO '{}' (FORMAT JSON, ARRAY true)",
            self.export_select()?,
            escaped
        );
        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to export JSON: {}", e))?;
        Ok(())
    }

    /// Export to CSV
    pub fn export_csv(&self, path: &str) -> Result<(), String> {
        let escaped = path.replace('\'', "''");
        let sql = format!(
            "COPY ({}) TO '{}' (FORMAT CSV, HEADER true)",
            self.export_select()?,
            escaped
        );
        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to export CSV: {}", e))?;
        Ok(())
    }

    /// Export to Excel (xlsx), writing cells directly from the Arrow data
    pub fn export_excel(&self, path: &str) -> Result<(), String> {
        let batch = self.query_to_batch(&self.export_select()?)?;
        let schema = batch.schema();
        let num_cols = batch.num_columns();

        let mut workbook = rust_xlsxwriter::Workbook::new();
        let worksheet = workbook.add_worksheet();

        for (col, field) in schema.fields().iter().enumerate() {
            worksheet
                .write_string(0, col as u16, field.name().to_string())
                .map_err(|e| format!("Failed to write xlsx header: {}", e))?;
        }

        for row_idx in 0..batch.num_rows() {
            let r = (row_idx + 1) as u32;
            for col in 0..num_cols {
                let value = arrow_array_to_json(batch.column(col), row_idx);
                let res = match value {
                    JsonValue::Null => continue,
                    JsonValue::Bool(b) => worksheet.write_boolean(r, col as u16, b),
                    JsonValue::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            // Preserve integer precision beyond 2^53 — Excel can't hold
                            // it as a number, so write it as text
                            let lossy = n
                                .as_i64()
                                .is_some_and(|i| (i as f64) as i64 != i);
                            if lossy {
                                worksheet.write_string(r, col as u16, n.to_string())
                            } else {
                                worksheet.write_number(r, col as u16, f)
                            }
                        } else {
                            worksheet.write_string(r, col as u16, n.to_string())
                        }
                    }
                    JsonValue::String(s) => worksheet.write_string(r, col as u16, s),
                    JsonValue::Array(_) | JsonValue::Object(_) => {
                        worksheet.write_string(r, col as u16, value.to_string())
                    }
                };
                res.map_err(|e| format!("Failed to write xlsx cell: {}", e))?;
            }
        }

        workbook.save(path)
            .map_err(|e| format!("Failed to save xlsx: {}", e))?;
        Ok(())
    }

    /// Undo the last mutation by executing its inverse statement
    pub fn undo(&mut self) -> Result<(), String> {
        let entry = self.undo_stack.pop()
            .ok_or_else(|| "Nothing to undo".to_string())?;

        self.conn
            .execute_batch(&entry.undo_sql)
            .map_err(|e| format!("Failed to undo: {}", e))?;

        self.redo_stack.push(entry);
        Ok(())
    }

    /// Redo the last undone mutation by re-applying its forward statement
    pub fn redo(&mut self) -> Result<(), String> {
        let entry = self.redo_stack.pop()
            .ok_or_else(|| "Nothing to redo".to_string())?;

        self.conn
            .execute_batch(&entry.redo_sql)
            .map_err(|e| format!("Failed to redo: {}", e))?;

        self.undo_stack.push(entry);
        Ok(())
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    // --- Workspace mode (multi-file: each file is a table) ---

    /// Open a workspace: create lazy views for every non-missing table.
    /// No `working` table exists until a table is opened for editing.
    pub fn open_workspace(ws: &catalog::Workspace) -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create DuckDB connection: {}", e))?;
        let mut engine = Self {
            conn,
            table_name: String::new(),
            page_size: 500,
            file_metadata: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            created_views: Vec::new(),
        };
        engine.sync_workspace_tables(ws)?;
        Ok(engine)
    }

    /// (Re)create views for every table in the workspace; drop stale ones we
    /// created earlier.
    pub fn sync_workspace_tables(&mut self, ws: &catalog::Workspace) -> Result<(), String> {
        let mut kept = Vec::new();
        for name in std::mem::take(&mut self.created_views) {
            if ws.tables.iter().any(|t| t.name == name) {
                kept.push(name);
            } else {
                self.conn
                    .execute_batch(&format!("DROP VIEW IF EXISTS {}", quote_ident(&name)))
                    .map_err(|e| format!("Failed to drop view '{}': {}", name, e))?;
            }
        }
        for t in &ws.tables {
            if t.missing {
                continue;
            }
            let abs = t.abs_path.clone().unwrap_or_else(|| t.path.clone());
            let escaped = abs.replace('\'', "''");
            self.conn
                .execute_batch(&format!(
                    "CREATE OR REPLACE VIEW {} AS SELECT * FROM read_parquet('{}')",
                    quote_ident(&t.name),
                    escaped
                ))
                .map_err(|e| format!("Failed to create view '{}': {}", t.name, e))?;
            if !kept.contains(&t.name) {
                kept.push(t.name.clone());
            }
        }
        self.created_views = kept;
        Ok(())
    }

    /// Whether a table is currently open for editing (the Lite flow on `working`).
    pub fn has_editor(&self) -> bool {
        !self.table_name.is_empty()
    }

    /// Materialize a workspace table into `working` for editing.
    /// The existing mutation/undo machinery operates on `working`.
    pub fn open_editor_table(&mut self, name: &str, abs_path: &str) -> Result<(), String> {
        self.conn
            .execute_batch("DROP INDEX IF EXISTS idx_working_rowid")
            .map_err(|e| format!("Failed to reset editor: {}", e))?;
        self.conn
            .execute_batch("DROP TABLE IF EXISTS working")
            .map_err(|e| format!("Failed to reset editor: {}", e))?;
        self.conn
            .execute_batch(&format!(
                "CREATE TABLE working AS SELECT row_number() OVER () AS _row_id, t.* FROM {} t",
                quote_ident(name)
            ))
            .map_err(|e| format!("Failed to open table for editing: {}", e))?;
        self.conn
            .execute_batch("CREATE INDEX idx_working_rowid ON working(_row_id)")
            .map_err(|e| format!("Failed to index _row_id: {}", e))?;
        self.table_name = "working".to_string();
        self.file_metadata = Some(FileMetadata {
            path: Some(std::path::PathBuf::from(abs_path)),
            compression: CompressionPreset::SNAPPY,
            row_group_size: 0,
            data_page_size: None,
        });
        self.undo_stack.clear();
        self.redo_stack.clear();
        Ok(())
    }

    /// Close the editor table (drop `working`), returning to query-only mode.
    pub fn close_editor(&mut self) -> Result<(), String> {
        self.conn
            .execute_batch("DROP INDEX IF EXISTS idx_working_rowid")
            .map_err(|e| format!("Failed to close editor: {}", e))?;
        self.conn
            .execute_batch("DROP TABLE IF EXISTS working")
            .map_err(|e| format!("Failed to close editor: {}", e))?;
        self.table_name.clear();
        self.file_metadata = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        Ok(())
    }

    /// Metadata for a parquet file (rows, row groups, compression, size) —
    /// read from the footer, no full scan.
    pub fn get_table_meta(&self, path: &str) -> Result<serde_json::Value, String> {
        if !std::path::Path::new(path).exists() {
            return Err(format!("File not found: {}", path));
        }
        let escaped = path.replace('\'', "''");
        let agg_sql = format!(
            "SELECT COUNT(*) AS row_groups, SUM(row_group_num_rows) AS rows \
             FROM (SELECT DISTINCT row_group_id, row_group_num_rows \
                   FROM parquet_metadata('{}'))",
            escaped
        );
        let (row_groups, rows): (i64, i64) = self
            .conn
            .query_row(&agg_sql, [], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| format!("Failed to read parquet metadata: {}", e))?;
        let comp_sql = format!(
            "SELECT DISTINCT compression FROM parquet_metadata('{}') LIMIT 1",
            escaped
        );
        let compression: String = self
            .conn
            .query_row(&comp_sql, [], |r| r.get(0))
            .unwrap_or_else(|_| "unknown".to_string());
        let size_bytes = std::fs::metadata(path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        Ok(serde_json::json!({
            "path": path,
            "rows": rows,
            "row_groups": row_groups,
            "compression": compression,
            "size_bytes": size_bytes,
        }))
    }

    /// Export a workspace table to parquet with the requested compression.
    pub fn export_table(&self, name: &str, out_path: &str, compression: &str) -> Result<(), String> {
        let comp = match compression.to_uppercase().as_str() {
            "SNAPPY" => "SNAPPY",
            "ZSTD" => "ZSTD",
            "GZIP" => "GZIP",
            "LZ4" | "LZ4_RAW" => "LZ4_RAW",
            "BROTLI" => "BROTLI",
            "NONE" | "UNCOMPRESSED" => "UNCOMPRESSED",
            other => return Err(format!("Unsupported compression: {}", other)),
        };
        let escaped = out_path.replace('\'', "''");
        let sql = format!(
            "COPY (SELECT * FROM {}) TO '{}' (FORMAT PARQUET, COMPRESSION {})",
            quote_ident(name),
            escaped,
            comp
        );
        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to export table: {}", e))?;
        Ok(())
    }

    /// Schema + SUMMARIZE stats + sample rows for a table (AI/analysis context).
    pub fn summarize_table(&self, name: &str) -> Result<serde_json::Value, String> {
        let ident = quote_ident(name);

        let desc = self.query_to_batch(&format!("DESCRIBE {}", ident))?;
        let desc_rows = rows_from_batch(&desc, 0)?;
        let columns: Vec<serde_json::Value> = desc_rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.values.first().cloned().unwrap_or(JsonValue::Null),
                    "type": r.values.get(1).cloned().unwrap_or(JsonValue::Null),
                    "null": r.values.get(2).cloned().unwrap_or(JsonValue::Null),
                })
            })
            .collect();

        let sum = self.query_to_batch(&format!("SUMMARIZE {}", ident))?;
        let names: Vec<String> = sum
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        let sum_rows = rows_from_batch(&sum, 0)?;
        let stats: Vec<serde_json::Value> = sum_rows
            .iter()
            .map(|r| {
                let mut obj = serde_json::Map::new();
                for (i, n) in names.iter().enumerate() {
                    if let Some(v) = r.values.get(i) {
                        obj.insert(n.clone(), v.clone());
                    }
                }
                serde_json::Value::Object(obj)
            })
            .collect();

        let sample_batch = self.query_to_batch(&format!("SELECT * FROM {} LIMIT 5", ident))?;
        let sample = rows_from_batch(&sample_batch, 0)?;

        Ok(serde_json::json!({
            "table": name,
            "columns": columns,
            "stats": stats,
            "sample": sample,
        }))
    }

    /// Internal: execute SQL and get Arrow RecordBatch.
    /// Returns an empty-schema batch (0 rows) when the query yields no results,
    /// so empty files, past-end pages and non-SELECT statements behave cleanly.
    fn query_to_batch(&self, sql: &str) -> Result<RecordBatch, String> {
        self.query_to_batch_params(sql, &[])
    }

    fn query_to_batch_params(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
    ) -> Result<RecordBatch, String> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let arrow_result = stmt
            .query_arrow(params)
            .map_err(|e| format!("Failed to execute query: {}", e))?;

        let schema = arrow_result.get_schema();
        let mut batches = Vec::new();
        for batch in arrow_result {
            batches.push(batch);
        }

        if batches.is_empty() {
            return Ok(RecordBatch::new_empty(schema));
        }
        if batches.len() == 1 {
            return Ok(batches.remove(0));
        }

        arrow::compute::concat_batches(&schema, batches.iter())
            .map_err(|e| format!("Failed to combine query results: {}", e))
    }
}

// --- Arrow to JSON conversion (supports nested types) ---

/// Quote a SQL identifier (column/table name) for safe interpolation.
/// Doubles embedded double-quotes per SQL standard.
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Render a serde_json value as a SQL literal (values are always parameter-escaped).
fn json_to_literal(v: &JsonValue) -> Result<String, String> {
    match v {
        JsonValue::Null => Ok("NULL".to_string()),
        JsonValue::String(s) => Ok(format!("'{}'", s.replace('\'', "''"))),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_string())
            } else if let Some(f) = n.as_f64() {
                Ok(f.to_string())
            } else {
                Err("Invalid number".to_string())
            }
        }
        JsonValue::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.to_string()),
        _ => Ok(format!("'{}'", v.to_string().replace('\'', "''"))),
    }
}

/// Escape a LIKE pattern so the user's query is matched literally
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// --- Pattern-aware insert defaults ---
// The +Row action continues the observed sequence in each column instead of
// blanking it. Only the last 3 rows are inspected (O(1) per column).

fn is_int_dtype(t: &str) -> bool {
    matches!(
        t,
        "int8" | "int16" | "int32" | "int64" | "integer" | "bigint" | "tinyint" | "smallint"
            | "hugeint" | "uint8" | "uint16" | "uint32" | "uint64"
    )
}

fn is_float_dtype(t: &str) -> bool {
    matches!(t, "float32" | "float64" | "float" | "double" | "real")
}

fn is_string_dtype(t: &str) -> bool {
    matches!(t, "varchar" | "utf8" | "text" | "string" | "char")
}

/// Integers: continue the dominant constant step across the window when it is
/// confirmed by a strict majority of adjacent pairs (at least 3 values);
/// otherwise None — never guess a fallback.
fn next_int_default(values: &[JsonValue]) -> Option<String> {
    let nums: Vec<i64> = values.iter().filter_map(|v| v.as_i64()).collect();
    if nums.len() < 3 {
        return None;
    }
    let s = dominant_step(nums.iter().map(|&n| n as f64), |a, b| (a - b) as i64)?;
    let mut candidate = nums[0] + s;
    let mut guard = 0;
    while nums.contains(&candidate) && guard < 1000 {
        candidate += s;
        guard += 1;
    }
    Some(candidate.to_string())
}

fn next_float_default(values: &[JsonValue]) -> Option<String> {
    let nums: Vec<f64> = values.iter().filter_map(|v| v.as_f64()).collect();
    if nums.len() < 3 {
        return None;
    }
    let s = dominant_step(
        nums.iter().copied(),
        |a, b| ((a - b) * 1_000_000.0).round() as i64,
    )
    .map(|q| q as f64 / 1_000_000.0)?;
    let mut candidate = nums[0] + s;
    let mut guard = 0;
    while nums.iter().any(|n| (n - candidate).abs() < 1e-9) && guard < 1000 {
        candidate += s;
        guard += 1;
    }
    Some(candidate.to_string())
}

/// Find the step that occurs most often between consecutive values (the values
/// arrive newest-first, so steps are usually positive). Returns None on a tie
/// or when no pair has a non-zero step.
fn dominant_step<F, S>(values: impl Iterator<Item = f64>, step_of: F) -> Option<S>
where
    F: Fn(f64, f64) -> S,
    S: PartialEq + Copy + std::hash::Hash + Eq,
{
    let nums: Vec<f64> = values.collect();
    if nums.len() < 2 {
        return None;
    }
    let mut counts: std::collections::HashMap<S, usize> = std::collections::HashMap::new();
    for w in nums.windows(2) {
        let s = step_of(w[0], w[1]);
        if s != step_of(0.0, 0.0) {
            *counts.entry(s).or_insert(0) += 1;
        }
    }
    let pairs = nums.len() - 1;
    counts
        .into_iter()
        .filter(|(_, c)| *c > pairs / 2)
        .max_by_key(|(_, c)| *c)
        .map(|(s, _)| s)
}

fn next_date_default(values: &[JsonValue]) -> Option<String> {
    let dates: Vec<NaiveDate> = values
        .iter()
        .filter_map(|v| v.as_str())
        .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .collect();
    if dates.len() < 3 {
        return None;
    }
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let step = dominant_step(
        dates.iter().map(|d| d.signed_duration_since(epoch).num_days() as f64),
        |a, b| (a - b) as i64,
    )
    .map(ChronoDuration::days)?;
    let mut candidate = dates[0] + step;
    let mut guard = 0;
    while dates.contains(&candidate) && guard < 1000 {
        candidate = candidate + step;
        guard += 1;
    }
    Some(format!("'{}'::DATE", candidate.format("%Y-%m-%d")))
}

fn parse_timestamp(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.3f").ok())
}

fn next_timestamp_default(values: &[JsonValue]) -> Option<String> {
    let ts: Vec<NaiveDateTime> = values
        .iter()
        .filter_map(|v| v.as_str())
        .filter_map(parse_timestamp)
        .collect();
    if ts.len() < 3 {
        return None;
    }
    let step = dominant_step(
        ts.iter().map(|t| t.and_utc().timestamp() as f64),
        |a, b| (a - b) as i64,
    )
    .map(ChronoDuration::seconds)?;
    let mut candidate = ts[0] + step;
    let mut guard = 0;
    while ts.contains(&candidate) && guard < 1000 {
        candidate = candidate + step;
        guard += 1;
    }
    Some(format!(
        "'{}'::TIMESTAMP",
        candidate.format("%Y-%m-%d %H:%M:%S")
    ))
}

/// Split "ID-0003" into ("ID-", 3, 4). Returns None when there's no trailing number.
fn split_trailing_number(s: &str) -> Option<(&str, i64, usize)> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == bytes.len() {
        return None;
    }
    let num = s[i..].parse::<i64>().ok()?;
    Some((&s[..i], num, s.len() - i))
}

/// Strings: continue a "prefix + zero-padded counter" convention when most
/// adjacent values follow it (e.g. "ID-0001", "ID-0002" -> "ID-0003"),
/// skipping counters that already exist in the window; else None.
fn next_string_default(values: &[JsonValue]) -> Option<String> {
    let strs: Vec<&str> = values.iter().filter_map(|v| v.as_str()).collect();
    let mut counts: std::collections::HashMap<(&str, usize), usize> = std::collections::HashMap::new();
    for w in strs.windows(2) {
        if let (Some((pre0, num0, w0)), Some((pre1, num1, w1))) = (
            split_trailing_number(w[0]),
            split_trailing_number(w[1]),
        ) {
            if pre0 == pre1 && w0 == w1 && num1 == num0 - 1 {
                *counts.entry((pre0, w0)).or_insert(0) += 1;
            }
        }
    }
    let pairs = strs.len().saturating_sub(1);
    let best = counts
        .into_iter()
        .filter(|(_, c)| pairs > 0 && *c > pairs / 2)
        .max_by_key(|(_, c)| *c)
        .map(|((pre, w), _)| (pre, w));
    if let Some((pre, w)) = best {
        let used: Vec<i64> = strs
            .iter()
            .filter_map(|s| split_trailing_number(s))
            .filter(|(p, _, ww)| *p == pre && *ww == w)
            .map(|(_, n, _)| n)
            .collect();
        let newest = used.first().copied().unwrap_or(0);
        let mut candidate = newest + 1;
        let mut guard = 0;
        while used.contains(&candidate) && guard < 1000 {
            candidate += 1;
            guard += 1;
        }
        let next = format!("{}{:0width$}", pre, candidate, width = w);
        return Some(format!("'{}'", next.replace('\'', "''")));
    }
    None
}

/// Map a DuckDB column type to the requested SQL dialect
fn map_type(dtype: &str, dialect: &str) -> String {
    let t = dtype.to_uppercase();
    let base = match t.as_str() {
        "BIGINT" | "INT64" => "BIGINT",
        "INTEGER" | "INT32" | "INT" => "INTEGER",
        "DOUBLE" | "FLOAT64" => "DOUBLE",
        "FLOAT" | "FLOAT32" | "REAL" => "FLOAT",
        "BOOLEAN" | "BOOL" => "BOOLEAN",
        "DATE" => "DATE",
        "TIMESTAMP" | "DATETIME" => "TIMESTAMP",
        _ => "VARCHAR",
    };
    match dialect {
        "postgres" => match base {
            "FLOAT" => "REAL",
            "DOUBLE" => "DOUBLE PRECISION",
            "TIMESTAMP" => "TIMESTAMP",
            _ => base,
        },
        "mysql" => match base {
            "VARCHAR" => "VARCHAR(255)",
            "INTEGER" => "INT",
            "DOUBLE" => "DOUBLE",
            "FLOAT" => "FLOAT",
            "TIMESTAMP" => "DATETIME",
            _ => base,
        },
        "sqlite" => match base {
            "BIGINT" | "INTEGER" | "FLOAT" | "DOUBLE" => match base {
                "BIGINT" | "INTEGER" => "INTEGER",
                _ => "REAL",
            },
            "BOOLEAN" => "INTEGER",
            "TIMESTAMP" => "DATETIME",
            _ => "TEXT",
        },
        _ => base,
    }
    .to_string()
}

pub fn rows_from_batch(batch: &RecordBatch, offset: usize) -> Result<Vec<RowData>, String> {
    let schema = batch.schema();
    let num_rows = batch.num_rows();

    if batch.num_columns() == 0 {
        return Ok((0..num_rows)
            .map(|i| RowData {
                row_id: (offset + i) as u64,
                values: vec![],
            })
            .collect());
    }

    // Skip the internal _row_id column when present so values align 1:1 with display columns
    let skip = if schema.field(0).name() == "_row_id" { 1 } else { 0 };

    let mut rows = Vec::with_capacity(num_rows);
    for row_idx in 0..num_rows {
        let row_id = {
            // Try to get _row_id from first column
            if skip == 1 {
                let col = batch.column(0);
                if let Some(arr) = col.as_any().downcast_ref::<UInt64Array>() {
                    arr.value(row_idx)
                } else if let Some(arr) = col.as_any().downcast_ref::<Int64Array>() {
                    arr.value(row_idx) as u64
                } else {
                    (offset + row_idx) as u64
                }
            } else {
                (offset + row_idx) as u64
            }
        };

        let values: Vec<JsonValue> = (skip..batch.num_columns())
            .map(|col_idx| arrow_array_to_json(batch.column(col_idx), row_idx))
            .collect();

        rows.push(RowData { row_id, values });
    }

    Ok(rows)
}

pub fn arrow_array_to_json(array: &dyn Array, row_idx: usize) -> JsonValue {
    if array.is_null(row_idx) {
        return JsonValue::Null;
    }

    // Primitive types
    if let Some(arr) = array.as_any().downcast_ref::<StringArray>() {
        return JsonValue::String(arr.value(row_idx).to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<LargeStringArray>() {
        return JsonValue::String(arr.value(row_idx).to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<Int64Array>() {
        return JsonValue::Number(arr.value(row_idx).into());
    }
    if let Some(arr) = array.as_any().downcast_ref::<Int32Array>() {
        return JsonValue::Number((arr.value(row_idx) as i64).into());
    }
    if let Some(arr) = array.as_any().downcast_ref::<UInt64Array>() {
        return JsonValue::Number(arr.value(row_idx).into());
    }
    if let Some(arr) = array.as_any().downcast_ref::<UInt32Array>() {
        return JsonValue::Number((arr.value(row_idx) as i64).into());
    }
    if let Some(arr) = array.as_any().downcast_ref::<Float64Array>() {
        if let Some(n) = serde_json::Number::from_f64(arr.value(row_idx)) {
            return JsonValue::Number(n);
        }
        return JsonValue::String(arr.value(row_idx).to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<Float32Array>() {
        if let Some(n) = serde_json::Number::from_f64(arr.value(row_idx) as f64) {
            return JsonValue::Number(n);
        }
        return JsonValue::String(arr.value(row_idx).to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<BooleanArray>() {
        return JsonValue::Bool(arr.value(row_idx));
    }

    // Date types
    if let Some(arr) = array.as_any().downcast_ref::<Date32Array>() {
        let days = arr.value(row_idx);
        let date = chrono::NaiveDate::from_num_days_from_ce_opt(719163 + days)
            .unwrap_or_default();
        return JsonValue::String(date.format("%Y-%m-%d").to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<Date64Array>() {
        let ms = arr.value(row_idx);
        let date = chrono::DateTime::from_timestamp_millis(ms)
            .map(|dt| dt.naive_utc())
            .unwrap_or_default();
        return JsonValue::String(date.format("%Y-%m-%d").to_string());
    }

    // Timestamp types
    if let Some(arr) = array.as_any().downcast_ref::<TimestampMillisecondArray>() {
        let ms = arr.value(row_idx);
        if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
            return JsonValue::String(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
        }
    }
    if let Some(arr) = array.as_any().downcast_ref::<TimestampMicrosecondArray>() {
        let us = arr.value(row_idx);
        let ms = us / 1000;
        if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
            return JsonValue::String(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
        }
    }
    if let Some(arr) = array.as_any().downcast_ref::<TimestampNanosecondArray>() {
        let ns = arr.value(row_idx);
        let ms = ns / 1_000_000;
        if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
            return JsonValue::String(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
        }
    }
    if let Some(arr) = array.as_any().downcast_ref::<TimestampSecondArray>() {
        let s = arr.value(row_idx);
        if let Some(dt) = chrono::DateTime::from_timestamp(s, 0) {
            return JsonValue::String(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
        }
    }

    // Binary (BLOB) - show as hex preview
    if let Some(arr) = array.as_any().downcast_ref::<BinaryArray>() {
        let bytes = arr.value(row_idx);
        let preview: String = bytes.iter().take(32).map(|b| format!("{:02x}", b)).collect();
        let suffix = if bytes.len() > 32 { "..." } else { "" };
        return JsonValue::String(format!("[{} bytes: {}{}]", bytes.len(), preview, suffix));
    }
    if let Some(arr) = array.as_any().downcast_ref::<LargeBinaryArray>() {
        let bytes = arr.value(row_idx);
        let preview: String = bytes.iter().take(32).map(|b| format!("{:02x}", b)).collect();
        let suffix = if bytes.len() > 32 { "..." } else { "" };
        return JsonValue::String(format!("[{} bytes: {}{}]", bytes.len(), preview, suffix));
    }

    // Fixed size binary
    if let Some(arr) = array.as_any().downcast_ref::<FixedSizeBinaryArray>() {
        let bytes = arr.value(row_idx);
        let preview: String = bytes.iter().take(32).map(|b| format!("{:02x}", b)).collect();
        let suffix = if bytes.len() > 32 { "..." } else { "" };
        return JsonValue::String(format!("[{} bytes: {}{}]", bytes.len(), preview, suffix));
    }

    // Decimal types
    if let Some(arr) = array.as_any().downcast_ref::<Decimal128Array>() {
        return JsonValue::String(arr.value_as_string(row_idx).to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<Decimal256Array>() {
        return JsonValue::String(arr.value_as_string(row_idx).to_string());
    }

    // List / Array types
    if let Some(arr) = array.as_any().downcast_ref::<ListArray>() {
        let values = arr.value(row_idx);
        let json_values: Vec<JsonValue> = (0..values.len())
            .map(|i| arrow_array_to_json(values.as_ref(), i))
            .collect();
        return JsonValue::Array(json_values);
    }
    if let Some(arr) = array.as_any().downcast_ref::<LargeListArray>() {
        let values = arr.value(row_idx);
        let json_values: Vec<JsonValue> = (0..values.len())
            .map(|i| arrow_array_to_json(values.as_ref(), i))
            .collect();
        return JsonValue::Array(json_values);
    }

    // Struct type
    if let Some(arr) = array.as_any().downcast_ref::<StructArray>() {
        let mut obj = serde_json::Map::new();
        for (i, field) in arr.fields().iter().enumerate() {
            let child = arr.column(i);
            let val = arrow_array_to_json(child, row_idx);
            obj.insert(field.name().clone(), val);
        }
        return JsonValue::Object(obj);
    }

    // Map type
    if let Some(arr) = array.as_any().downcast_ref::<MapArray>() {
        let entries = arr.value(row_idx);
        let mut obj = serde_json::Map::new();
        if let Some(struct_arr) = entries.as_any().downcast_ref::<StructArray>() {
            let keys = struct_arr.column_by_name("key");
            let values_col = struct_arr.column_by_name("value");

            if let (Some(keys), Some(values)) = (keys, values_col) {
                for i in 0..entries.len() {
                    let k = arrow_array_to_json(keys.as_ref(), i);
                    let v = arrow_array_to_json(values.as_ref(), i);
                    if let JsonValue::String(s) = k {
                        obj.insert(s, v);
                    }
                }
            }
        }
        return JsonValue::Object(obj);
    }

    // Duration type
    if let Some(arr) = array.as_any().downcast_ref::<DurationSecondArray>() {
        return JsonValue::String(format!("{}s", arr.value(row_idx)));
    }
    if let Some(arr) = array.as_any().downcast_ref::<DurationMillisecondArray>() {
        return JsonValue::String(format!("{}ms", arr.value(row_idx)));
    }

    // Interval type
    if let Some(arr) = array.as_any().downcast_ref::<IntervalYearMonthArray>() {
        let val = arr.value(row_idx);
        let years = val / 12;
        let months = val % 12;
        return JsonValue::String(format!("{}y {}m", years, months));
    }

    // Null type
    if let Some(_) = array.as_any().downcast_ref::<NullArray>() {
        return JsonValue::Null;
    }

    // Fallback: try to display as string representation
    JsonValue::String(format!("[unsupported type: {}]", array.data_type()))
}
