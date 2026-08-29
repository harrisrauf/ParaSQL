use arrow::array::*;
use arrow::record_batch::RecordBatch;
use duckdb::Connection;
use serde_json::Value as JsonValue;

use super::types::*;

// --- DuckDB Engine ---

pub struct DuckDbEngine {
    conn: Connection,
    table_name: String,
    page_size: usize,
    file_metadata: Option<FileMetadata>,
    undo_stack: Vec<Savepoint>,
    redo_stack: Vec<Savepoint>,
    sp_counter: u64,
}

/// A saved mutation point, plus the SQL needed to re-apply it on redo
struct Savepoint {
    name: String,
    sql: String,
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
            sp_counter: 0,
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
            sp_counter: 0,
        })
    }

    /// Create a savepoint for undo/redo, remembering the SQL to re-apply on redo
    fn create_savepoint(&mut self, sql: &str) -> Result<(), String> {
        let name = format!("sp_{}", self.sp_counter);
        self.sp_counter += 1;
        self.conn
            .execute_batch(&format!("SAVEPOINT {}", name))
            .map_err(|e| format!("Failed to create savepoint: {}", e))?;
        self.undo_stack.push(Savepoint {
            name,
            sql: sql.to_string(),
        });
        self.redo_stack.clear(); // Clear redo on new action
        Ok(())
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
        let col = &columns[col_idx];
        let sql = match value {
            JsonValue::Null => format!(
                "UPDATE {} SET \"{}\" = NULL WHERE _row_id = {}",
                self.table_name, col.name, row_id
            ),
            JsonValue::String(s) => {
                let escaped = s.replace('\'', "''");
                format!(
                    "UPDATE {} SET \"{}\" = '{}' WHERE _row_id = {}",
                    self.table_name, col.name, escaped, row_id
                )
            }
            JsonValue::Number(n) => {
                let v = if let Some(i) = n.as_i64() {
                    i.to_string()
                } else if let Some(f) = n.as_f64() {
                    f.to_string()
                } else {
                    return Err("Invalid number".to_string());
                };
                format!(
                    "UPDATE {} SET \"{}\" = {} WHERE _row_id = {}",
                    self.table_name, col.name, v, row_id
                )
            }
            JsonValue::Bool(b) => format!(
                "UPDATE {} SET \"{}\" = {} WHERE _row_id = {}",
                self.table_name, col.name, if *b { "TRUE" } else { "FALSE" }, row_id
            ),
            _ => format!(
                "UPDATE {} SET \"{}\" = '{}' WHERE _row_id = {}",
                self.table_name, col.name, value.to_string().replace('\'', "''"), row_id
            ),
        };

        self.create_savepoint(&sql)?;

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
        self.create_savepoint(&sql)?;

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

        let col_names: Vec<String> = columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let col_types: Vec<String> = columns.iter().map(|c| c.dtype.clone()).collect();

        // Build DEFAULT values based on column types
        let defaults: Vec<String> = col_types.iter().map(|dt| {
            match dt.to_lowercase().as_str() {
                "int32" | "int64" | "integer" | "bigint" => "0".to_string(),
                "float32" | "float64" | "float" | "double" => "0.0".to_string(),
                "boolean" | "bool" => "FALSE".to_string(),
                "date" => "'1970-01-01'::DATE".to_string(),
                "timestamp" | "timestamp_ms" | "timestamp_us" | "timestamp_ns" => "'1970-01-01T00:00:00'::TIMESTAMP".to_string(),
                "varchar" | "utf8" | "text" | "string" => "''".to_string(),
                _ => "NULL".to_string(),
            }
        }).collect();

        let sql = format!(
            "INSERT INTO {} (_row_id, {}) \
             VALUES ((SELECT COALESCE(MAX(_row_id), 0) + 1 FROM {}), {}) \
             RETURNING _row_id, *",
            self.table_name,
            col_names.join(", "),
            self.table_name,
            defaults.join(", ")
        );

        self.create_savepoint(&sql)?;

        let batch = self.query_to_batch(&sql)?;
        let rows = rows_from_batch(&batch, 0)?;
        Ok(QueryResult {
            columns: self.column_info()?,
            rows,
        })
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

        // DuckDB can't ALTER TABLE ADD COLUMN, so we recreate
        let escaped = name.replace('"', "\"\"");
        let default_val = match duckdb_type {
            "VARCHAR" => "''::VARCHAR".to_string(),
            "BOOLEAN" => "FALSE".to_string(),
            other => format!("CAST(NULL AS {})", other),
        };
        let sql = format!(
            "CREATE TABLE working_backup AS SELECT * FROM {}; \
             DROP TABLE {}; \
             CREATE TABLE {} AS SELECT *, {} AS \"{}\" FROM working_backup; \
             DROP TABLE working_backup;",
            self.table_name, self.table_name, self.table_name,
            default_val, escaped
        );

        self.create_savepoint(&sql)?;

        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to add column: {}", e))?;
        Ok(())
    }

    /// Drop a column (recreate table without it)
    pub fn drop_column(&mut self, name: &str) -> Result<(), String> {
        let columns = self.column_info()?;
        let remaining: Vec<String> = columns
            .iter()
            .filter(|c| c.name != name)
            .map(|c| format!("\"{}\"", c.name))
            .collect();

        if remaining.is_empty() {
            return Err("Cannot drop the last column".to_string());
        }

        let sql = format!(
            "CREATE TABLE working_backup AS SELECT _row_id, {}; \
             DROP TABLE {}; \
             CREATE TABLE {} AS SELECT * FROM working_backup; \
             DROP TABLE working_backup;",
            remaining.join(", "),
            self.table_name,
            self.table_name
        );

        self.create_savepoint(&sql)?;

        self.conn
            .execute_batch(&sql)
            .map_err(|e| format!("Failed to drop column: {}", e))?;
        Ok(())
    }

    /// Rename a column
    pub fn rename_column(&mut self, old_name: &str, new_name: &str) -> Result<(), String> {
        let escaped_old = old_name.replace('"', "\"\"");
        let escaped_new = new_name.replace('"', "\"\"");
        let sql = format!(
            "ALTER TABLE {} RENAME COLUMN \"{}\" TO \"{}\"",
            self.table_name, escaped_old, escaped_new
        );

        self.create_savepoint(&sql)?;

        self.conn
            .execute_batch(&sql)
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
                format!("  \"{}\" {}{}", c.name, map_type(&c.dtype, dialect), nullable)
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
                            worksheet.write_number(r, col as u16, f)
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

    /// Undo the last mutation
    pub fn undo(&mut self) -> Result<(), String> {
        let entry = self.undo_stack.pop()
            .ok_or_else(|| "Nothing to undo".to_string())?;

        self.conn
            .execute_batch(&format!("ROLLBACK TO SAVEPOINT {}", entry.name))
            .map_err(|e| format!("Failed to rollback: {}", e))?;

        self.conn
            .execute_batch(&format!("RELEASE SAVEPOINT {}", entry.name))
            .map_err(|e| format!("Failed to release savepoint: {}", e))?;

        self.redo_stack.push(entry);
        Ok(())
    }

    /// Redo the last undone mutation by re-applying its SQL
    pub fn redo(&mut self) -> Result<(), String> {
        let entry = self.redo_stack.pop()
            .ok_or_else(|| "Nothing to redo".to_string())?;

        self.conn
            .execute_batch(&entry.sql)
            .map_err(|e| format!("Failed to redo: {}", e))?;

        // Re-create the savepoint so the mutation can be undone again
        self.conn
            .execute_batch(&format!("SAVEPOINT {}", entry.name))
            .map_err(|e| format!("Failed to create savepoint: {}", e))?;

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

    /// Internal: execute SQL and get Arrow RecordBatch
    fn query_to_batch(&self, sql: &str) -> Result<RecordBatch, String> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let arrow_result = stmt
            .query_arrow([])
            .map_err(|e| format!("Failed to execute query: {}", e))?;

        let schema = arrow_result.get_schema();
        let mut batches = Vec::new();
        for batch in arrow_result {
            batches.push(batch);
        }

        if batches.is_empty() {
            return Err("Query returned no results".to_string());
        }
        if batches.len() == 1 {
            return Ok(batches.remove(0));
        }

        arrow::compute::concat_batches(&schema, batches.iter())
            .map_err(|e| format!("Failed to combine query results: {}", e))
    }
}

// --- Arrow to JSON conversion (supports nested types) ---

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
