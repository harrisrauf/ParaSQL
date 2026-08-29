# Parquet Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Tauri v2 desktop app that opens, views, edits, and saves Apache Parquet files with full CRUD, search, sort, schema editing, and undo/redo.

**Architecture:** Rust backend owns the authoritative Arrow `RecordBatch` with stable row IDs. All mutations go through Tauri commands that return incremental patches. Frontend (Svelte 5) renders data client-side with instant local search/sort. Undo/redo via a bounded OpLog in Rust.

**Tech Stack:** Tauri v2, Rust (arrow 54, parquet 54), Svelte 5, TypeScript, Vite

**Note:** This plan uses JSON serialization for IPC (via `serde_json::Value`) instead of Arrow IPC binary + `@apache-arrow` JS bindings as described in the spec. For the v0 data size target (~30MB), JSON is sufficient and simpler. Arrow IPC can be adopted later for larger files and type-safe int64/timestamp handling.

---

### Task 1: Scaffold Tauri + Svelte Project

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `package.json`
- Create: `svelte.config.js`
- Create: `vite.config.ts`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `index.html`
- Create: `src/main.ts`
- Create: `src/App.svelte`
- Create: `src/app.css`
- Create: `src/vite-env.d.ts`
- Create: `src-tauri/capabilities/default.json`
- Modify: `opencode.json`

- [ ] **Step 1: Install Tauri CLI and create project**

Run:
```bash
npm install -g @tauri-apps/cli@latest
```

```bash
cd "D:\.projects\paraquet Viewer"
npm create tauri-app@latest parquet-viewer -- --template svelte-ts --manager npm
```

If interactive prompts appear, answer: Project name = `parquet-viewer`, Frontend = Svelte + TypeScript, Package manager = npm.

- [ ] **Step 2: Verify project builds**

```bash
cd "D:\.projects\paraquet Viewer"
npm install
cargo tauri init --app-name "Parquet Viewer" --window-title "Parquet Viewer" --dev-url http://localhost:5173 --before-dev-command "npm run dev" --before-build-command "npm run build" --frontend-dist ../dist
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: Clean build, no errors.

- [ ] **Step 3: Add Rust dependencies to Cargo.toml**

Edit `src-tauri/Cargo.toml` to add these under `[dependencies]`:

```toml
arrow = "54"
parquet = "54"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri-plugin-dialog = "2"
tauri = { version = "2", features = ["protocol-asset"] }

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

- [ ] **Step 4: Add JS dependencies**

```bash
npm install @apache-arrow@latest
npm install -D @tauri-apps/api@latest @tauri-apps/plugin-dialog@latest
```

- [ ] **Step 5: Verify build with new dependencies**

```bash
cd "D:\.projects\paraquet Viewer"
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: Clean build.

---

### Task 2: Rust Core Types

**Files:**
- Create: `src-tauri/src/engine/mod.rs`
- Create: `src-tauri/src/engine/types.rs`
- Create: `src-tauri/src/engine/types_test.rs`

- [ ] **Step 1: Create engine module and types**

Write `src-tauri/src/engine/mod.rs`:
```rust
pub mod types;
```

Write `src-tauri/src/engine/types.rs`:
```rust
use arrow::datatypes::{DataType, Field, Schema};
use parquet::basic::{Compression, Encoding};
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
pub struct MutationResult {
    pub affected: Vec<RowData>,
    pub deleted_ids: Vec<u64>,
    pub schema_changed: bool,
    pub new_schema: Option<Vec<ColumnInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataJson {
    pub columns: Vec<ColumnInfo>,
    pub total_rows: usize,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub path: Option<std::path::PathBuf>,
    pub compression: Compression,
    pub row_group_size: usize,
    pub data_page_size: Option<usize>,
}

impl Default for FileMetadata {
    fn default() -> Self {
        Self {
            path: None,
            compression: Compression::SNAPPY,
            row_group_size: 1024,
            data_page_size: Some(1024 * 1024),
        }
    }
}

#[derive(Debug, Clone)]
pub enum OpEntry {
    EditCell {
        row_id: u64,
        col_idx: usize,
        old: serde_json::Value,
        new: serde_json::Value,
    },
    DeleteRows {
        rows: Vec<(u64, Vec<serde_json::Value>)>,
    },
    InsertRows {
        row_ids: Vec<u64>,
        index: usize,
    },
    AddColumn {
        name: String,
        dtype: DataType,
    },
    DropColumn {
        name: String,
        column_data: Vec<Option<serde_json::Value>>,
    },
    RenameColumn {
        old_name: String,
        new_name: String,
    },
}

#[derive(Debug, Clone)]
pub struct OpLog {
    pub entries: Vec<OpEntry>,
    pub position: usize,
    pub max_entries: usize,
}

impl OpLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries),
            position: 0,
            max_entries,
        }
    }

    pub fn push(&mut self, entry: OpEntry) {
        // Truncate any redo entries
        self.entries.truncate(self.position);
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.position = self.entries.len();
    }

    pub fn can_undo(&self) -> bool {
        self.position > 0
    }

    pub fn can_redo(&self) -> bool {
        self.position < self.entries.len()
    }

    pub fn undo(&mut self) -> Option<&OpEntry> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        Some(&self.entries[self.position])
    }

    pub fn redo(&mut self) -> Option<&OpEntry> {
        if self.position >= self.entries.len() {
            return None;
        }
        let entry = &self.entries[self.position];
        self.position += 1;
        Some(entry)
    }
}
```

- [ ] **Step 2: Write tests for OpLog**

Write `src-tauri/src/engine/types_test.rs`:
```rust
#[cfg(test)]
mod tests {
    use crate::engine::types::*;

    #[test]
    fn test_oplog_push_and_undo() {
        let mut oplog = OpLog::new(5);
        assert!(!oplog.can_undo());
        assert!(!oplog.can_redo());

        oplog.push(OpEntry::EditCell {
            row_id: 1,
            col_idx: 0,
            old: serde_json::Value::Null,
            new: serde_json::Value::String("test".into()),
        });

        assert!(oplog.can_undo());
        assert!(!oplog.can_redo());
        assert_eq!(oplog.position, 1);

        let entry = oplog.undo();
        assert!(entry.is_some());
        assert!(oplog.can_redo());
        assert_eq!(oplog.position, 0);
    }

    #[test]
    fn test_oplog_truncates_redo_on_new_push() {
        let mut oplog = OpLog::new(10);
        oplog.push(OpEntry::EditCell {
            row_id: 1, col_idx: 0,
            old: serde_json::Value::Null, new: serde_json::Value::String("a".into()),
        });
        oplog.push(OpEntry::EditCell {
            row_id: 1, col_idx: 0,
            old: serde_json::Value::String("a".into()), new: serde_json::Value::String("b".into()),
        });

        oplog.undo(); // back to position 1
        assert!(oplog.can_redo());

        // New push should truncate redo
        oplog.push(OpEntry::EditCell {
            row_id: 2, col_idx: 0,
            old: serde_json::Value::Null, new: serde_json::Value::String("c".into()),
        });

        assert!(!oplog.can_redo());
        assert_eq!(oplog.position, 2);
        assert_eq!(oplog.entries.len(), 2);
    }

    #[test]
    fn test_oplog_bounded_capacity() {
        let mut oplog = OpLog::new(3);
        for i in 0..5 {
            oplog.push(OpEntry::EditCell {
                row_id: i, col_idx: 0,
                old: serde_json::Value::Null, new: serde_json::Value::Number((i as i64).into()),
            });
        }
        assert_eq!(oplog.entries.len(), 3);
        assert_eq!(oplog.position, 3);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail** (module not yet registered in lib.rs)

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: Compilation error — `engine` module not found or test module not compiled.

- [ ] **Step 4: Register engine module in lib.rs**

Write `src-tauri/src/lib.rs`:
```rust
mod engine;

pub use engine::types;
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: 3 tests pass.

---

### Task 3: Parquet I/O — Read and Write

**Files:**
- Create: `src-tauri/src/engine/parquet_io.rs`
- Create: `src-tauri/src/engine/parquet_io_test.rs`
- Modify: `src-tauri/src/engine/mod.rs`

- [ ] **Step 1: Write the failing test**

Write `src-tauri/src/engine/parquet_io_test.rs`:
```rust
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use arrow::array::{Int64Array, StringArray, BooleanArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use tempfile::NamedTempFile;
    use crate::engine::parquet_io::*;

    fn create_test_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Int64, true),
            Field::new("active", DataType::Boolean, false),
        ]));

        let names = StringArray::from(vec!["Alice", "Bob", "Charlie"]);
        let ages = Int64Array::from(vec![Some(30), Some(25), Some(35)]);
        let actives = BooleanArray::from(vec![true, false, true]);

        RecordBatch::try_new(schema, vec![
            Arc::new(names),
            Arc::new(ages),
            Arc::new(actives),
        ]).unwrap()
    }

    #[test]
    fn test_round_trip_parquet() {
        let batch = create_test_batch();
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        write_parquet(&path, &batch, None).unwrap();
        let (read_batch, _metadata) = read_parquet(&path).unwrap();

        assert_eq!(read_batch.schema(), batch.schema());
        assert_eq!(read_batch.num_rows(), batch.num_rows());
        assert_eq!(read_batch.num_columns(), batch.num_columns());
    }

    #[test]
    fn test_read_parquet_preserves_data() {
        let batch = create_test_batch();
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        write_parquet(&path, &batch, None).unwrap();
        let (read_batch, _) = read_parquet(&path).unwrap();

        let names = read_batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(names.value(0), "Alice");
        assert_eq!(names.value(1), "Bob");
        assert_eq!(names.value(2), "Charlie");

        let ages = read_batch.column(1).as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(ages.value(0), 30);
        assert_eq!(ages.value(1), 25);
        assert_eq!(ages.value(2), 35);
    }
}
```

- [ ] **Step 2: Implement parquet_io**

Write `src-tauri/src/engine/parquet_io.rs`:
```rust
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use super::types::FileMetadata;

pub fn read_parquet(path: &Path) -> Result<(RecordBatch, FileMetadata), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;

    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| format!("Failed to create parquet reader: {}", e))?;

    let metadata = builder.metadata().file_metadata().clone();

    let compression = metadata.writer_props().compression()
        .unwrap_or(Compression::SNAPPY);

    let row_group_size = metadata.num_rows() as usize;

    let reader = builder
        .build()
        .map_err(|e| format!("Failed to build parquet reader: {}", e))?;

    let mut all_batches: Vec<RecordBatch> = Vec::new();
    for batch_result in reader {
        let batch = batch_result.map_err(|e| format!("Failed to read batch: {}", e))?;
        all_batches.push(batch);
    }

    let batch = if all_batches.is_empty() {
        return Err("No data in parquet file".to_string());
    } else if all_batches.len() == 1 {
        all_batches.remove(0)
    } else {
        let schema = all_batches[0].schema();
        let mut combined = Vec::new();
        for b in &all_batches {
            for col_idx in 0..b.num_columns() {
                // Collect rows across batches
            }
        }
        // Simple concatenation
        let num_rows: usize = all_batches.iter().map(|b| b.num_rows()).sum();
        let num_cols = all_batches[0].num_columns();

        let mut columns = Vec::with_capacity(num_cols);
        for col_idx in 0..num_cols {
            let arrays: Vec<&dyn arrow::array::Array> = all_batches.iter()
                .map(|b| b.column(col_idx).as_ref())
                .collect();
            let concatenated = arrow::compute::concat(&arrays)
                .map_err(|e| format!("Failed to concatenate batches: {}", e))?;
            columns.push(concatenated);
        }
        RecordBatch::try_new(schema, columns)
            .map_err(|e| format!("Failed to create combined batch: {}", e))?
    };

    let file_meta = FileMetadata {
        path: Some(path.to_path_buf()),
        compression,
        row_group_size,
        data_page_size: None,
    };

    Ok((batch, file_meta))
}

pub fn write_parquet(
    path: &Path,
    batch: &RecordBatch,
    file_meta: Option<&FileMetadata>,
) -> Result<(), String> {
    let meta = file_meta.cloned().unwrap_or_default();

    let mut props_builder = WriterProperties::builder()
        .set_compression(meta.compression);

    if let Some(page_size) = meta.data_page_size {
        props_builder = props_builder.set_data_page_size_limit(page_size);
    }

    let props = props_builder.build();

    let file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;

    let schema = batch.schema();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))
        .map_err(|e| format!("Failed to create parquet writer: {}", e))?;

    writer
        .write(batch)
        .map_err(|e| format!("Failed to write batch: {}", e))?;

    writer
        .close()
        .map_err(|e| format!("Failed to close parquet writer: {}", e))?;

    Ok(())
}
```

- [ ] **Step 3: Add parquet_io module to mod.rs**

Edit `src-tauri/src/engine/mod.rs`:
```rust
pub mod parquet_io;
pub mod types;
#[cfg(test)]
mod parquet_io_test;
#[cfg(test)]
mod types_test;
```

- [ ] **Step 4: Add tempfile dev-dependency**

Edit `src-tauri/Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 5: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: All 5 tests pass.

---

### Task 4: Engine — RecordBatch Wrapper with Stable Row IDs

**Files:**
- Create: `src-tauri/src/engine/engine.rs`
- Create: `src-tauri/src/engine/engine_test.rs`
- Modify: `src-tauri/src/engine/mod.rs`

- [ ] **Step 1: Write failing tests for Engine**

Write `src-tauri/src/engine/engine_test.rs`:
```rust
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use super::super::engine::*;

    fn make_test_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("score", DataType::Int64, true),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["Alice", "Bob", "Charlie"])),
                Arc::new(Int64Array::from(vec![Some(90), Some(85), Some(95)])),
            ],
        )
        .unwrap()
    }

    #[test]
    fn test_engine_new() {
        let batch = make_test_batch();
        let engine = Engine::new(batch.clone());
        assert_eq!(engine.num_rows(), 3);
        assert_eq!(engine.num_cols(), 2);
        assert!(engine.is_modified());
        assert_eq!(engine.row_ids().len(), 3);
    }

    #[test]
    fn test_engine_row_ids_sequential() {
        let batch = make_test_batch();
        let engine = Engine::new(batch);
        let ids = engine.row_ids();
        assert_eq!(ids[0], 0);
        assert_eq!(ids[1], 1);
        assert_eq!(ids[2], 2);
    }

    #[test]
    fn test_engine_edit_cell() {
        let batch = make_test_batch();
        let mut engine = Engine::new(batch);
        let result = engine.edit_cell(1, 0, serde_json::Value::String("Bobby".into())).unwrap();
        assert_eq!(result.affected.len(), 1);
        assert!(!result.schema_changed);

        // Verify via get_row
        let row = engine.get_row(1).unwrap();
        assert_eq!(row.values[0], serde_json::Value::String("Bobby".into()));
    }

    #[test]
    fn test_engine_delete_rows() {
        let batch = make_test_batch();
        let mut engine = Engine::new(batch);
        let result = engine.delete_rows(vec![0, 2]).unwrap();
        assert_eq!(result.deleted_ids.len(), 2);
        assert_eq!(engine.num_rows(), 1);
        // Row 1 (Bob) should remain
        let row = engine.get_row_by_id(1).unwrap();
        assert_eq!(row.values[0], serde_json::Value::String("Bob".into()));
    }

    #[test]
    fn test_engine_insert_row() {
        let batch = make_test_batch();
        let mut engine = Engine::new(batch);
        let result = engine.insert_row(1).unwrap();
        assert_eq!(result.affected.len(), 1);
        assert_eq!(engine.num_rows(), 4);

        // Inserted row should be at index 1 with null values
        let row = engine.get_row_by_index(1).unwrap();
        assert_eq!(row.values[0], serde_json::Value::Null);
    }

    #[test]
    fn test_engine_add_column() {
        let batch = make_test_batch();
        let mut engine = Engine::new(batch);
        let result = engine.add_column("grade", arrow::datatypes::DataType::Utf8).unwrap();
        assert!(result.schema_changed);
        assert_eq!(engine.num_cols(), 3);
        let new_schema = result.new_schema.unwrap();
        assert_eq!(new_schema.last().unwrap().name, "grade");
    }

    #[test]
    fn test_engine_drop_column() {
        let batch = make_test_batch();
        let mut engine = Engine::new(batch);
        let result = engine.drop_column("score").unwrap();
        assert!(result.schema_changed);
        assert_eq!(engine.num_cols(), 1);
    }

    #[test]
    fn test_engine_rename_column() {
        let batch = make_test_batch();
        let mut engine = Engine::new(batch);
        let result = engine.rename_column("name", "full_name").unwrap();
        assert!(result.schema_changed);
        let cols = engine.column_info();
        assert_eq!(cols[0].name, "full_name");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: Compilation error — engine module not found.

- [ ] **Step 3: Implement Engine**

Write `src-tauri/src/engine/engine.rs`:
```rust
use std::sync::Arc;
use arrow::array::*;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use serde_json::Value as JsonValue;
use super::types::*;

fn row_to_json(batch: &RecordBatch, row_idx: usize) -> Vec<JsonValue> {
    (0..batch.num_columns())
        .map(|col_idx| {
            let col = batch.column(col_idx);
            arrow_array_to_json(col, row_idx)
        })
        .collect()
}

fn arrow_array_to_json(array: &dyn arrow::array::Array, row_idx: usize) -> JsonValue {
    use arrow::array::*;
    if array.is_null(row_idx) {
        return JsonValue::Null;
    }
    if let Some(arr) = array.as_any().downcast_ref::<StringArray>() {
        JsonValue::String(arr.value(row_idx).to_string())
    } else if let Some(arr) = array.as_any().downcast_ref::<LargeStringArray>() {
        JsonValue::String(arr.value(row_idx).to_string())
    } else if let Some(arr) = array.as_any().downcast_ref::<Int64Array>() {
        JsonValue::Number(arr.value(row_idx).into())
    } else if let Some(arr) = array.as_any().downcast_ref::<Int32Array>() {
        JsonValue::Number(arr.value(row_idx).into())
    } else if let Some(arr) = array.as_any().downcast_ref::<Float64Array>() {
        if let Some(n) = serde_json::Number::from_f64(arr.value(row_idx)) {
            JsonValue::Number(n)
        } else {
            JsonValue::String(arr.value(row_idx).to_string())
        }
    } else if let Some(arr) = array.as_any().downcast_ref::<Float32Array>() {
        if let Some(n) = serde_json::Number::from_f64(arr.value(row_idx) as f64) {
            JsonValue::Number(n)
        } else {
            JsonValue::String(arr.value(row_idx).to_string())
        }
    } else if let Some(arr) = array.as_any().downcast_ref::<BooleanArray>() {
        JsonValue::Bool(arr.value(row_idx))
    } else {
        JsonValue::String(format!("{:?}", array.value(row_idx)))
    }
}

fn json_to_arrow_value(value: &JsonValue, dtype: &DataType) -> Result<JsonValue, String> {
    // Convert JSON value back to the correct representation for the column type
    match dtype {
        DataType::Int64 | DataType::Int32 | DataType::Float64 | DataType::Float32 => {
            Ok(value.clone())
        }
        DataType::Boolean => {
            match value {
                JsonValue::Bool(b) => Ok(JsonValue::Bool(*b)),
                JsonValue::String(s) if s == "true" => Ok(JsonValue::Bool(true)),
                JsonValue::String(s) if s == "false" => Ok(JsonValue::Bool(false)),
                _ => Err(format!("Cannot convert {:?} to boolean", value)),
            }
        }
        DataType::Utf8 | DataType::LargeUtf8 => {
            Ok(JsonValue::String(match value {
                JsonValue::String(s) => s.clone(),
                other => other.to_string(),
            }))
        }
        _ => Ok(JsonValue::String(value.to_string())),
    }
}

pub struct Engine {
    batch: RecordBatch,
    schema: SchemaRef,
    row_ids: Vec<u64>,
    next_row_id: u64,
    file_metadata: Option<FileMetadata>,
    modified: bool,
    oplog: OpLog,
}

impl Engine {
    pub fn new(batch: RecordBatch) -> Self {
        let num_rows = batch.num_rows();
        let row_ids: Vec<u64> = (0..num_rows as u64).collect();
        Self {
            schema: batch.schema(),
            batch,
            row_ids,
            next_row_id: num_rows as u64,
            file_metadata: None,
            modified: true,
            oplog: OpLog::new(500),
        }
    }

    pub fn from_parquet(batch: RecordBatch, file_meta: FileMetadata) -> Self {
        let mut engine = Self::new(batch);
        engine.file_metadata = Some(file_meta);
        engine.modified = false;
        engine
    }

    pub fn num_rows(&self) -> usize {
        self.batch.num_rows()
    }

    pub fn num_cols(&self) -> usize {
        self.batch.num_columns()
    }

    pub fn row_ids(&self) -> &[u64] {
        &self.row_ids
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn file_path(&self) -> Option<&std::path::Path> {
        self.file_metadata.as_ref().and_then(|m| m.path.as_deref())
    }

    pub fn file_metadata(&self) -> Option<&FileMetadata> {
        self.file_metadata.as_ref()
    }

    pub fn batch(&self) -> &RecordBatch {
        &self.batch
    }

    pub fn column_info(&self) -> Vec<ColumnInfo> {
        self.schema.fields().iter().map(|f| ColumnInfo {
            name: f.name().clone(),
            dtype: format!("{:?}", f.data_type()),
            nullable: f.is_nullable(),
        }).collect()
    }

    pub fn get_row(&self, row_id: u64) -> Option<RowData> {
        let idx = self.row_ids.iter().position(|id| *id == row_id)?;
        Some(RowData {
            row_id,
            values: row_to_json(&self.batch, idx),
        })
    }

    pub fn get_row_by_index(&self, index: usize) -> Option<RowData> {
        if index >= self.num_rows() {
            return None;
        }
        Some(RowData {
            row_id: self.row_ids[index],
            values: row_to_json(&self.batch, index),
        })
    }

    pub fn get_all_rows(&self) -> Vec<RowData> {
        (0..self.num_rows())
            .map(|i| RowData {
                row_id: self.row_ids[i],
                values: row_to_json(&self.batch, i),
            })
            .collect()
    }

    pub fn edit_cell(&mut self, row_id: u64, col_idx: usize, value: JsonValue) -> Result<MutationResult, String> {
        let row_idx = self.row_ids.iter().position(|id| *id == row_id)
            .ok_or_else(|| format!("Row {} not found", row_id))?;

        if col_idx >= self.num_cols() {
            return Err(format!("Column index {} out of range", col_idx));
        }

        let dtype = self.schema.field(col_idx).data_type();
        let old_val = row_to_json(&self.batch, row_idx)[col_idx].clone();
        let new_val = json_to_arrow_value(&value, dtype)?;

        // Record in oplog
        self.oplog.push(OpEntry::EditCell {
            row_id,
            col_idx,
            old: old_val.clone(),
            new: new_val.clone(),
        });

        // Actually update the batch
        // We need to rebuild the column array with the new value
        let col = self.batch.column(col_idx);
        let new_array = replace_value_in_array(col, row_idx, &new_val, dtype);

        let mut new_columns: Vec<Arc<dyn arrow::array::Array>> = self.batch.columns().to_vec();
        new_columns[col_idx] = new_array;

        self.batch = RecordBatch::try_new(self.schema.clone(), new_columns)
            .map_err(|e| format!("Failed to rebuild batch: {}", e))?;

        self.modified = true;

        Ok(MutationResult {
            affected: vec![RowData {
                row_id,
                values: row_to_json(&self.batch, row_idx),
            }],
            deleted_ids: vec![],
            schema_changed: false,
            new_schema: None,
        })
    }

    pub fn delete_rows(&mut self, row_ids_input: Vec<u64>) -> Result<MutationResult, String> {
        let mut indices_to_remove: Vec<usize> = row_ids_input.iter()
            .filter_map(|id| self.row_ids.iter().position(|r| r == id))
            .collect();
        indices_to_remove.sort_unstable();
        indices_to_remove.dedup();
        indices_to_remove.reverse(); // Remove from end first to preserve indices

        // Save deleted rows for undo
        let deleted_rows: Vec<(u64, Vec<JsonValue>)> = indices_to_remove.iter()
            .map(|&idx| (self.row_ids[idx], row_to_json(&self.batch, idx)))
            .collect();

        self.oplog.push(OpEntry::DeleteRows {
            rows: deleted_rows.clone(),
        });

        // Reconstruct batch without removed rows
        let remaining_indices: Vec<usize> = (0..self.num_rows())
            .filter(|i| !indices_to_remove.contains(i))
            .collect();

        let new_columns: Vec<Arc<dyn Array>> = (0..self.num_cols())
            .map(|col_idx| {
                let col = self.batch.column(col_idx);
                let mut builder = make_array_builder(col.data_type(), remaining_indices.len());
                for &idx in &remaining_indices {
                    append_array_value(&mut builder, col, idx);
                }
                builder.finish()
            })
            .collect();

        let new_ids: Vec<u64> = remaining_indices.iter().map(|&i| self.row_ids[i]).collect();

        self.batch = RecordBatch::try_new(self.schema.clone(), new_columns)
            .map_err(|e| format!("Failed to rebuild batch: {}", e))?;
        self.row_ids = new_ids;
        self.modified = true;

        Ok(MutationResult {
            affected: vec![],
            deleted_ids: row_ids_input.clone(),
            schema_changed: false,
            new_schema: None,
        })
    }

    pub fn insert_row(&mut self, index: usize) -> Result<MutationResult, String> {
        let new_id = self.next_row_id;
        self.next_row_id += 1;

        self.oplog.push(OpEntry::InsertRows {
            row_ids: vec![new_id],
            index,
        });

        // Build a row of nulls
        let new_columns: Vec<Arc<dyn Array>> = (0..self.num_cols())
            .map(|col_idx| {
                let col = self.batch.column(col_idx);
                let mut builder = make_array_builder(col.data_type(), 1);
                append_null(&mut builder);
                builder.finish()
            })
            .collect();

        let new_batch = RecordBatch::try_new(self.schema.clone(), new_columns.clone())
            .unwrap();

        // Concatenate old + new
        let columns: Vec<Arc<dyn Array>> = (0..self.num_cols())
            .map(|col_idx| {
                let arrays: Vec<&dyn Array> = if index >= self.num_rows() {
                    vec![self.batch.column(col_idx).as_ref(), new_batch.column(col_idx).as_ref()]
                } else {
                    let before = self.batch.slice(0, index);
                    let after = self.batch.slice(index, self.num_rows() - index);
                    vec![before.column(col_idx).as_ref(), new_batch.column(col_idx).as_ref(), after.column(col_idx).as_ref()]
                };
                arrow::compute::concat(&arrays).unwrap()
            })
            .collect();

        if index >= self.num_rows() {
            self.row_ids.push(new_id);
        } else {
            self.row_ids.insert(index, new_id);
        }

        self.batch = RecordBatch::try_new(self.schema.clone(), columns)
            .map_err(|e| format!("Failed to rebuild batch: {}", e))?;
        self.modified = true;

        Ok(MutationResult {
            affected: vec![RowData {
                row_id: new_id,
                values: vec![JsonValue::Null; self.num_cols()],
            }],
            deleted_ids: vec![],
            schema_changed: false,
            new_schema: None,
        })
    }

    pub fn add_column(&mut self, name: &str, dtype: DataType) -> Result<MutationResult, String> {
        let mut fields = self.schema.fields().clone();
        fields.push(Field::new(name, dtype.clone(), true));

        let new_schema = Arc::new(Schema::new(fields));

        // Build null array for the new column
        let null_array = make_null_array(&dtype, self.num_rows());

        let mut new_columns = self.batch.columns().to_vec();
        new_columns.push(null_array);

        self.oplog.push(OpEntry::AddColumn {
            name: name.to_string(),
            dtype: dtype.clone(),
        });

        self.batch = RecordBatch::try_new(new_schema.clone(), new_columns)
            .map_err(|e| format!("Failed to rebuild batch: {}", e))?;
        self.schema = new_schema;
        self.modified = true;

        Ok(MutationResult {
            affected: vec![],
            deleted_ids: vec![],
            schema_changed: true,
            new_schema: Some(self.column_info()),
        })
    }

    pub fn drop_column(&mut self, name: &str) -> Result<MutationResult, String> {
        let col_idx = self.schema.index_of(name).map_err(|_| format!("Column {} not found", name))?;

        // Save column data for undo
        let column_data: Vec<Option<JsonValue>> = (0..self.num_rows())
            .map(|i| {
                if self.batch.column(col_idx).is_null(i) {
                    None
                } else {
                    Some(row_to_json(&self.batch, i)[col_idx].clone())
                }
            })
            .collect();

        self.oplog.push(OpEntry::DropColumn {
            name: name.to_string(),
            column_data,
        });

        let mut fields = self.schema.fields().clone();
        fields.remove(col_idx);

        let new_schema = Arc::new(Schema::new(fields));

        let mut new_columns = self.batch.columns().to_vec();
        new_columns.remove(col_idx);

        self.batch = RecordBatch::try_new(new_schema.clone(), new_columns)
            .map_err(|e| format!("Failed to rebuild batch: {}", e))?;
        self.schema = new_schema;
        self.modified = true;

        Ok(MutationResult {
            affected: vec![],
            deleted_ids: vec![],
            schema_changed: true,
            new_schema: Some(self.column_info()),
        })
    }

    pub fn rename_column(&mut self, old_name: &str, new_name: &str) -> Result<MutationResult, String> {
        let col_idx = self.schema.index_of(old_name).map_err(|_| format!("Column {} not found", old_name))?;

        self.oplog.push(OpEntry::RenameColumn {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        });

        let mut fields = self.schema.fields().clone();
        let old_field = fields[col_idx].clone();
        fields[col_idx] = Field::new(new_name, old_field.data_type().clone(), old_field.is_nullable());

        let new_schema = Arc::new(Schema::new(fields));
        self.batch = RecordBatch::try_new(new_schema.clone(), self.batch.columns().to_vec())
            .map_err(|e| format!("Failed to rebuild batch: {}", e))?;
        self.schema = new_schema;
        self.modified = true;

        Ok(MutationResult {
            affected: vec![],
            deleted_ids: vec![],
            schema_changed: true,
            new_schema: Some(self.column_info()),
        })
    }

    pub fn undo(&mut self) -> Result<MutationResult, String> {
        let entry = self.oplog.undo().ok_or_else(|| "Nothing to undo".to_string())?.clone();
        self.apply_op_reverse(entry)
    }

    pub fn redo(&mut self) -> Result<MutationResult, String> {
        let entry = self.oplog.redo().ok_or_else(|| "Nothing to redo".to_string())?.clone();
        self.apply_op_forward(entry)
    }

    fn apply_op_reverse(&mut self, entry: OpEntry) -> Result<MutationResult, String> {
        match entry {
            OpEntry::EditCell { row_id, col_idx, old, .. } => {
                self.edit_cell_raw(row_id, col_idx, old)
            }
            OpEntry::DeleteRows { rows, .. } => {
                // Re-insert the rows
                for (row_id, values) in &rows {
                    self.insert_row_with_id(*row_id, values.clone())?;
                }
                Ok(MutationResult {
                    affected: rows.iter().map(|(id, vals)| RowData { row_id: *id, values: vals.clone() }).collect(),
                    deleted_ids: vec![],
                    schema_changed: false,
                    new_schema: None,
                })
            }
            OpEntry::InsertRows { row_ids, .. } => {
                self.delete_rows(row_ids)
            }
            OpEntry::AddColumn { name, .. } => {
                self.drop_column(&name)
            }
            OpEntry::DropColumn { name, column_data } => {
                // Re-add the column as Utf8 (dtype not stored in op to keep OpEntry simple)
                // Then restore saved string values
                let mut result = self.add_column(&name, DataType::Utf8)?;
                let added_col_idx = self.num_cols() - 1;
                for (i, val) in column_data.iter().enumerate() {
                    if let Some(v) = val {
                        if i < self.num_rows() {
                            let row_id = self.row_ids[i];
                            let _ = self.edit_cell_raw(row_id, added_col_idx, v.clone())?;
                        }
                    }
                }
                // Re-fetch snapshot
                result.new_schema = Some(self.column_info());
                Ok(result)
            }
            OpEntry::RenameColumn { old_name, new_name } => {
                self.rename_column(&new_name, &old_name)
            }
        }
    }

    fn apply_op_forward(&mut self, entry: OpEntry) -> Result<MutationResult, String> {
        match entry {
            OpEntry::EditCell { row_id, col_idx, new, .. } => {
                self.edit_cell_raw(row_id, col_idx, new)
            }
            OpEntry::DeleteRows { rows, .. } => {
                let ids: Vec<u64> = rows.iter().map(|(id, _)| *id).collect();
                self.delete_rows(ids)
            }
            OpEntry::InsertRows { row_ids, index } => {
                self.insert_row(index)
            }
            OpEntry::AddColumn { name, dtype } => {
                self.add_column(&name, dtype)
            }
            OpEntry::DropColumn { name, .. } => {
                self.drop_column(&name)
            }
            OpEntry::RenameColumn { old_name, new_name } => {
                self.rename_column(&old_name, &new_name)
            }
        }
    }

    fn edit_cell_raw(&mut self, row_id: u64, col_idx: usize, value: JsonValue) -> Result<MutationResult, String> {
        let row_idx = self.row_ids.iter().position(|id| *id == row_id)
            .ok_or_else(|| format!("Row {} not found", row_id))?;

        let dtype = self.schema.field(col_idx).data_type();
        let new_val = json_to_arrow_value(&value, dtype)?;

        let new_array = replace_value_in_array(self.batch.column(col_idx), row_idx, &new_val, dtype);

        let mut new_columns = self.batch.columns().to_vec();
        new_columns[col_idx] = new_array;

        self.batch = RecordBatch::try_new(self.schema.clone(), new_columns)
            .map_err(|e| format!("Failed to rebuild batch: {}", e))?;

        Ok(MutationResult {
            affected: vec![RowData {
                row_id,
                values: row_to_json(&self.batch, row_idx),
            }],
            deleted_ids: vec![],
            schema_changed: false,
            new_schema: None,
        })
    }

    fn insert_row_with_id(&mut self, row_id: u64, values: Vec<JsonValue>) -> Result<(), String> {
        let new_columns: Vec<Arc<dyn Array>> = (0..self.num_cols())
            .map(|col_idx| {
                let col = self.batch.column(col_idx);
                let val = values.get(col_idx).cloned().unwrap_or(JsonValue::Null);
                let mut builder = make_array_builder(col.data_type(), 1);
                if val.is_null() {
                    append_null(&mut builder);
                } else {
                    append_json_value(&mut builder, &val);
                }
                builder.finish()
            })
            .collect();

        let new_batch = RecordBatch::try_new(self.schema.clone(), new_columns).unwrap();

        let columns: Vec<Arc<dyn Array>> = (0..self.num_cols())
            .map(|col_idx| {
                let arrays = vec![self.batch.column(col_idx).as_ref(), new_batch.column(col_idx).as_ref()];
                arrow::compute::concat(&arrays).unwrap()
            })
            .collect();

        self.row_ids.push(row_id);
        self.batch = RecordBatch::try_new(self.schema.clone(), columns).unwrap();
        Ok(())
    }
}

// Builder helpers
fn make_array_builder(dtype: &DataType, capacity: usize) -> Box<dyn ArrayBuilder> {
    match dtype {
        DataType::Utf8 => Box::new(StringBuilder::with_capacity(capacity, capacity * 16)),
        DataType::LargeUtf8 => Box::new(LargeStringBuilder::with_capacity(capacity, capacity * 16)),
        DataType::Int64 => Box::new(Int64Builder::with_capacity(capacity)),
        DataType::Int32 => Box::new(Int32Builder::with_capacity(capacity)),
        DataType::Float64 => Box::new(Float64Builder::with_capacity(capacity)),
        DataType::Float32 => Box::new(Float32Builder::with_capacity(capacity)),
        DataType::Boolean => Box::new(BooleanBuilder::with_capacity(capacity)),
        _ => Box::new(StringBuilder::with_capacity(capacity, capacity * 16)),
    }
}

fn append_array_value(builder: &mut Box<dyn ArrayBuilder>, array: &dyn Array, idx: usize) {
    if array.is_null(idx) {
        append_null(builder);
        return;
    }
    if let Some(b) = builder.as_any_mut().downcast_mut::<StringBuilder>() {
        let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
        b.append_value(arr.value(idx));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<LargeStringBuilder>() {
        let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
        b.append_value(arr.value(idx));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Int64Builder>() {
        let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
        b.append_value(arr.value(idx));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Int32Builder>() {
        let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
        b.append_value(arr.value(idx));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Float64Builder>() {
        let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
        b.append_value(arr.value(idx));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Float32Builder>() {
        let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
        b.append_value(arr.value(idx));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<BooleanBuilder>() {
        let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
        b.append_value(arr.value(idx));
    } else {
        // Fallback
        append_null(builder);
    }
}

fn append_json_value(builder: &mut Box<dyn ArrayBuilder>, value: &JsonValue) {
    if let Some(b) = builder.as_any_mut().downcast_mut::<StringBuilder>() {
        b.append_value(match value { JsonValue::String(s) => s, other => &other.to_string() });
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Int64Builder>() {
        b.append_value(value.as_i64().unwrap_or(0));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Int32Builder>() {
        b.append_value(value.as_i64().unwrap_or(0) as i32);
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Float64Builder>() {
        b.append_value(value.as_f64().unwrap_or(0.0));
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Float32Builder>() {
        b.append_value(value.as_f64().unwrap_or(0.0) as f32);
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<BooleanBuilder>() {
        b.append_value(value.as_bool().unwrap_or(false));
    } else {
        append_null(builder);
    }
}

fn append_null(builder: &mut Box<dyn ArrayBuilder>) {
    if let Some(b) = builder.as_any_mut().downcast_mut::<StringBuilder>() {
        b.append_null();
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<LargeStringBuilder>() {
        b.append_null();
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Int64Builder>() {
        b.append_null();
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Int32Builder>() {
        b.append_null();
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Float64Builder>() {
        b.append_null();
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<Float32Builder>() {
        b.append_null();
    } else if let Some(b) = builder.as_any_mut().downcast_mut::<BooleanBuilder>() {
        b.append_null();
    }
}

fn make_null_array(dtype: &DataType, len: usize) -> Arc<dyn Array> {
    let mut builder = make_array_builder(dtype, len);
    for _ in 0..len {
        append_null(&mut builder);
    }
    builder.finish()
}

fn replace_value_in_array(
    array: &dyn Array,
    row_idx: usize,
    new_value: &JsonValue,
    dtype: &DataType,
) -> Arc<dyn Array> {
    let len = array.len();
    let mut builder = make_array_builder(dtype, len);
    for i in 0..len {
        if i == row_idx {
            if new_value.is_null() {
                append_null(&mut builder);
            } else {
                append_json_value(&mut builder, new_value);
            }
        } else if array.is_null(i) {
            append_null(&mut builder);
        } else {
            append_array_value(&mut builder, array, i);
        }
    }
    builder.finish()
}
```

- [ ] **Step 4: Add engine module to mod.rs**

Edit `src-tauri/src/engine/mod.rs`:
```rust
pub mod engine;
pub mod parquet_io;
pub mod types;
#[cfg(test)]
mod engine_test;
#[cfg(test)]
mod parquet_io_test;
#[cfg(test)]
mod types_test;
```

- [ ] **Step 5: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: All tests pass.

---

### Task 5: Tauri Commands — Wire Backend to IPC

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Implement commands**

Write `src-tauri/src/commands.rs`:
```rust
use std::sync::Mutex;
use tauri::State;
use crate::engine::engine::Engine;
use crate::engine::parquet_io;
use crate::engine::types::*;

pub struct AppState {
    pub engine: Mutex<Option<Engine>>,
}

#[tauri::command]
pub fn open_file_dialog(state: State<AppState>) -> Result<MetadataJson, String> {
    // We'll use the dialog plugin to get the path
    // For now, return placeholder
    // In real impl, opened via dialog plugin from frontend
    Err("Use open_file command with a path".to_string())
}

#[tauri::command]
pub fn open_file(path: String, state: State<AppState>) -> Result<MetadataJson, String> {
    let file_path = std::path::PathBuf::from(&path);
    let (batch, file_meta) = parquet_io::read_parquet(&file_path)?;
    let engine = Engine::from_parquet(batch, file_meta);

    let cols = engine.column_info();
    let total_rows = engine.num_rows();

    let metadata = MetadataJson {
        columns: cols,
        total_rows,
        file_path: Some(path),
    };

    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    *eng = Some(engine);

    Ok(metadata)
}

#[tauri::command]
pub fn save_file(state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;

    let path = engine.file_path().ok_or_else(|| "No file path set".to_string())?.to_path_buf();
    let batch = engine.batch();
    let meta = engine.file_metadata();

    parquet_io::write_parquet(&path, batch, meta)?;
    Ok(())
}

#[tauri::command]
pub fn save_file_as(path: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;

    let file_path = std::path::PathBuf::from(&path);
    let batch = engine.batch();

    parquet_io::write_parquet(&file_path, batch, None)?;
    Ok(())
}

#[tauri::command]
pub fn get_file_info(state: State<AppState>) -> Result<serde_json::Value, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;

    Ok(serde_json::json!({
        "rows": engine.num_rows(),
        "cols": engine.num_cols(),
        "modified": engine.is_modified(),
        "path": engine.file_path().map(|p| p.to_string_lossy().to_string()),
        "columns": engine.column_info(),
    }))
}

#[tauri::command]
pub fn get_all_rows(state: State<AppState>) -> Result<Vec<RowData>, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    Ok(engine.get_all_rows())
}

#[tauri::command]
pub fn edit_cell(row_id: u64, col_idx: usize, value: serde_json::Value, state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.edit_cell(row_id, col_idx, value)
}

#[tauri::command]
pub fn delete_rows(row_ids: Vec<u64>, state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.delete_rows(row_ids)
}

#[tauri::command]
pub fn insert_row(index: usize, state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.insert_row(index)
}

#[tauri::command]
pub fn add_column(name: String, dtype: String, state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;

    let arrow_dtype = match dtype.to_lowercase().as_str() {
        "utf8" | "string" | "text" => arrow::datatypes::DataType::Utf8,
        "int64" | "int" | "integer" | "bigint" => arrow::datatypes::DataType::Int64,
        "int32" => arrow::datatypes::DataType::Int32,
        "float64" | "float" | "double" => arrow::datatypes::DataType::Float64,
        "float32" => arrow::datatypes::DataType::Float32,
        "bool" | "boolean" => arrow::datatypes::DataType::Boolean,
        _ => arrow::datatypes::DataType::Utf8,
    };

    engine.add_column(&name, arrow_dtype)
}

#[tauri::command]
pub fn drop_column(name: String, state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.drop_column(&name)
}

#[tauri::command]
pub fn rename_column(old_name: String, new_name: String, state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.rename_column(&old_name, &new_name)
}

#[tauri::command]
pub fn undo(state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.undo()
}

#[tauri::command]
pub fn redo(state: State<AppState>) -> Result<MutationResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.redo()
}
```

- [ ] **Step 2: Update lib.rs to register commands**

Edit `src-tauri/src/lib.rs`:
```rust
mod commands;
mod engine;

use commands::AppState;
use std::sync::Mutex;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            engine: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_file,
            commands::save_file,
            commands::save_file_as,
            commands::get_file_info,
            commands::get_all_rows,
            commands::edit_cell,
            commands::delete_rows,
            commands::insert_row,
            commands::add_column,
            commands::drop_column,
            commands::rename_column,
            commands::undo,
            commands::redo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: Update main.rs**

Edit `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    parquet_viewer_lib::run();
}
```

Note: The lib crate name is derived from Cargo.toml's `[package] name`. If the name is `parquet-viewer`, the lib name becomes `parquet_viewer`. Adjust accordingly.

- [ ] **Step 4: Update capabilities for dialog plugin**

Write `src-tauri/capabilities/default.json`:
```json
{
  "identifier": "default",
  "description": "Default capability set",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "dialog:allow-open",
    "dialog:allow-save"
  ]
}
```

- [ ] **Step 5: Build to verify**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: Clean build.

---

### Task 6: Frontend — Types, Commands, and Store

**Files:**
- Create: `src/lib/types.ts`
- Create: `src/lib/commands.ts`
- Create: `src/lib/stores/table.ts`

- [ ] **Step 1: Define frontend types**

Write `src/lib/types.ts`:
```typescript
export interface ColumnInfo {
  name: string;
  dtype: string;
  nullable: boolean;
}

export interface RowData {
  row_id: number;
  values: (string | number | boolean | null)[];
}

export interface MutationResult {
  affected: RowData[];
  deleted_ids: number[];
  schema_changed: boolean;
  new_schema: ColumnInfo[] | null;
}

export interface MetadataJson {
  columns: ColumnInfo[];
  total_rows: number;
  file_path: string | null;
}

export type SortDirection = 'asc' | 'desc' | null;

export interface SortConfig {
  column: string;
  direction: SortDirection;
}

export interface SearchConfig {
  query: string;
  column: string | null; // null = search all columns
}
```

- [ ] **Step 2: Create typed command wrappers**

Write `src/lib/commands.ts`:
```typescript
import { invoke } from '@tauri-apps/api/core';
import type { ColumnInfo, RowData, MutationResult, MetadataJson } from './types';

export async function openFile(path: string): Promise<MetadataJson> {
  return invoke('open_file', { path });
}

export async function saveFile(): Promise<void> {
  return invoke('save_file');
}

export async function saveFileAs(path: string): Promise<void> {
  return invoke('save_file_as', { path });
}

export async function getFileInfo(): Promise<{
  rows: number;
  cols: number;
  modified: boolean;
  path: string | null;
  columns: ColumnInfo[];
}> {
  return invoke('get_file_info');
}

export async function getAllRows(): Promise<RowData[]> {
  return invoke('get_all_rows');
}

export async function editCell(
  rowId: number,
  colIdx: number,
  value: string | number | boolean | null
): Promise<MutationResult> {
  return invoke('edit_cell', { row_id: rowId, col_idx: colIdx, value });
}

export async function deleteRows(rowIds: number[]): Promise<MutationResult> {
  return invoke('delete_rows', { row_ids: rowIds });
}

export async function insertRow(index: number): Promise<MutationResult> {
  return invoke('insert_row', { index });
}

export async function addColumn(name: string, dtype: string): Promise<MutationResult> {
  return invoke('add_column', { name, dtype });
}

export async function dropColumn(name: string): Promise<MutationResult> {
  return invoke('drop_column', { name });
}

export async function renameColumn(oldName: string, newName: string): Promise<MutationResult> {
  return invoke('rename_column', { old_name: oldName, new_name: newName });
}

export async function undo(): Promise<MutationResult> {
  return invoke('undo');
}

export async function redo(): Promise<MutationResult> {
  return invoke('redo');
}
```

- [ ] **Step 3: Create Svelte store**

Write `src/lib/stores/table.ts`:
```typescript
import { writable, derived } from 'svelte/store';
import type { ColumnInfo, RowData, SortConfig, SearchConfig, SortDirection } from '../types';

interface TableState {
  columns: ColumnInfo[];
  rows: RowData[];
  selectedRowIds: Set<number>;
  sort: SortConfig;
  search: SearchConfig;
  modified: boolean;
  filePath: string | null;
}

function createTableStore() {
  const { subscribe, set, update } = writable<TableState>({
    columns: [],
    rows: [],
    selectedRowIds: new Set(),
    sort: { column: '', direction: null },
    search: { query: '', column: null },
    modified: false,
    filePath: null,
  });

  function toggleSort(column: string) {
    update(state => {
      let newDirection: SortDirection = 'asc';
      if (state.sort.column === column) {
        if (state.sort.direction === 'asc') newDirection = 'desc';
        else if (state.sort.direction === 'desc') newDirection = null;
      }
      return { ...state, sort: { column: newDirection ? column : '', direction: newDirection } };
    });
  }

  function setSearch(query: string, column: string | null = null) {
    update(state => ({ ...state, search: { query, column } }));
  }

  function applyMutation(result: { affected: RowData[]; deleted_ids: number[]; schema_changed: boolean; new_schema: ColumnInfo[] | null }) {
    update(state => {
      let rows = [...state.rows];
      let columns = [...state.columns];

      // Remove deleted rows
      if (result.deleted_ids.length > 0) {
        const deleteSet = new Set(result.deleted_ids);
        rows = rows.filter(r => !deleteSet.has(r.row_id));
      }

      // Update affected rows
      for (const affected of result.affected) {
        const idx = rows.findIndex(r => r.row_id === affected.row_id);
        if (idx >= 0) {
          rows[idx] = affected;
        } else {
          rows.push(affected);
        }
      }

      // Schema changes
      if (result.schema_changed && result.new_schema) {
        columns = result.new_schema;
      }

      return { ...state, rows, columns, modified: true };
    });
  }

  function selectRow(rowId: number, shift = false, ctrl = false) {
    update(state => {
      const newSet = new Set(state.selectedRowIds);
      if (ctrl) {
        if (newSet.has(rowId)) newSet.delete(rowId);
        else newSet.add(rowId);
      } else {
        newSet.clear();
        newSet.add(rowId);
      }
      return { ...state, selectedRowIds: newSet };
    });
  }

  function clearSelection() {
    update(state => ({ ...state, selectedRowIds: new Set() }));
  }

  return {
    subscribe,
    set: (val: TableState) => set(val),
    update,
    toggleSort,
    setSearch,
    applyMutation,
    selectRow,
    clearSelection,
  };
}

export const tableStore = createTableStore();

// Derived: filtered + sorted view
export const displayedRows = derived(tableStore, ($table) => {
  let rows = [...$table.rows];

  // Apply search
  if ($table.search.query) {
    const q = $table.search.query.toLowerCase();
    if ($table.search.column) {
      const colIdx = $table.columns.findIndex(c => c.name === $table.search.column);
      if (colIdx >= 0) {
        rows = rows.filter(r => {
          const val = r.values[colIdx];
          return val !== null && String(val).toLowerCase().includes(q);
        });
      }
    } else {
      rows = rows.filter(r =>
        r.values.some(v => v !== null && String(v).toLowerCase().includes(q))
      );
    }
  }

  // Apply sort
  if ($table.sort.direction && $table.sort.column) {
    const colIdx = $table.columns.findIndex(c => c.name === $table.sort.column);
    if (colIdx >= 0) {
      rows.sort((a, b) => {
        const va = a.values[colIdx];
        const vb = b.values[colIdx];
        if (va === null && vb === null) return 0;
        if (va === null) return 1;
        if (vb === null) return -1;
        if (typeof va === 'number' && typeof vb === 'number') {
          return $table.sort.direction === 'asc' ? va - vb : vb - va;
        }
        const sa = String(va).toLowerCase();
        const sb = String(vb).toLowerCase();
        if (sa < sb) return $table.sort.direction === 'asc' ? -1 : 1;
        if (sa > sb) return $table.sort.direction === 'asc' ? 1 : -1;
        return 0;
      });
    }
  }

  return rows;
});
```

---

### Task 7: Frontend — DataTable Component

**Files:**
- Create: `src/lib/components/DataTable.svelte`
- Create: `src/lib/components/TableHeader.svelte`
- Create: `src/lib/components/TableRow.svelte`
- Create: `src/lib/components/CellEditor.svelte`

- [ ] **Step 1: Implement TableHeader**

Write `src/lib/components/TableHeader.svelte`:
```svelte
<script lang="ts">
  import type { ColumnInfo, SortConfig } from '../types';

  let { columns, sort, onSort }: {
    columns: ColumnInfo[];
    sort: SortConfig;
    onSort: (col: string) => void;
  } = $props();
</script>

<thead>
  <tr>
    <th class="row-id-col">#</th>
    {#each columns as col}
      <th
        class="col-header"
        class:sort-active={sort.column === col.name}
        onclick={() => onSort(col.name)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === 'Enter' && onSort(col.name)}
      >
        <span class="col-name">{col.name}</span>
        <span class="col-type">{col.dtype}</span>
        {#if sort.column === col.name}
          <span class="sort-indicator">
            {sort.direction === 'asc' ? '▲' : '▼'}
          </span>
        {/if}
      </th>
    {/each}
    <th class="actions-col"></th>
  </tr>
</thead>

<style>
  .row-id-col {
    width: 48px;
    min-width: 48px;
    text-align: center;
    color: #888;
    font-size: 12px;
  }
  .col-header {
    position: relative;
    cursor: pointer;
    user-select: none;
    padding: 8px 12px;
    text-align: left;
    border-bottom: 2px solid #e0e0e0;
    white-space: nowrap;
  }
  .col-header:hover {
    background: #f5f5f5;
  }
  .col-header.sort-active {
    color: #1a73e8;
    border-bottom-color: #1a73e8;
  }
  .col-name {
    display: block;
    font-weight: 600;
    font-size: 13px;
  }
  .col-type {
    display: block;
    font-size: 11px;
    color: #999;
    font-weight: 400;
  }
  .sort-indicator {
    margin-left: 4px;
    font-size: 10px;
  }
  .actions-col {
    width: 40px;
  }
</style>
```

- [ ] **Step 2: Implement CellEditor**

Write `src/lib/components/CellEditor.svelte`:
```svelte
<script lang="ts">
  import { onMount } from 'svelte';

  let { value, dtype, onSave, onCancel }: {
    value: string | number | boolean | null;
    dtype: string;
    onSave: (val: string | number | boolean | null) => void;
    onCancel: () => void;
  } = $props();

  let inputValue = $state(formatValue(value));
  let inputEl: HTMLInputElement | HTMLSelectElement | undefined = $state();

  function formatValue(v: string | number | boolean | null): string {
    if (v === null) return '';
    if (typeof v === 'boolean') return v ? 'true' : 'false';
    return String(v);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') save();
    else if (e.key === 'Escape') onCancel();
  }

  function save() {
    if (dtype.toLowerCase().includes('int') || dtype.toLowerCase().includes('float')) {
      const num = inputValue === '' ? null : Number(inputValue);
      onSave(isNaN(num as number) ? null : num);
    } else if (dtype.toLowerCase() === 'boolean') {
      onSave(inputValue === 'true');
    } else {
      onSave(inputValue === '' ? null : inputValue);
    }
  }

  onMount(() => {
    if (inputEl) {
      inputEl.focus();
      inputEl.select();
    }
  });
</script>

{#if dtype.toLowerCase() === 'boolean'}
  <select
    bind:this={inputEl as HTMLSelectElement}
    bind:value={inputValue}
    onchange={save}
    onkeydown={handleKeydown}
    class="cell-editor"
  >
    <option value="true">true</option>
    <option value="false">false</option>
  </select>
{:else}
  <input
    bind:this={inputEl as HTMLInputElement}
    type={dtype.toLowerCase().includes('int') || dtype.toLowerCase().includes('float') ? 'number' : 'text'}
    bind:value={inputValue}
    onkeydown={handleKeydown}
    onblur={save}
    class="cell-editor"
  />
{/if}

<style>
  .cell-editor {
    width: 100%;
    height: 100%;
    border: 2px solid #1a73e8;
    outline: none;
    padding: 2px 6px;
    font-size: 13px;
    font-family: inherit;
    background: white;
    box-sizing: border-box;
    border-radius: 2px;
  }
  select.cell-editor {
    padding: 1px 4px;
  }
</style>
```

- [ ] **Step 3: Implement TableRow**

Write `src/lib/components/TableRow.svelte`:
```svelte
<script lang="ts">
  import CellEditor from './CellEditor.svelte';
  import type { RowData, ColumnInfo } from '../types';

  let {
    row,
    columns,
    rowIndex,
    isSelected,
    editingCol,
    onSelect,
    onEdit,
    onSave,
    onCancel,
  }: {
    row: RowData;
    columns: ColumnInfo[];
    rowIndex: number;
    isSelected: boolean;
    editingCol: number | null;
    onSelect: (e: MouseEvent) => void;
    onEdit: (colIdx: number) => void;
    onSave: (colIdx: number, val: string | number | boolean | null) => void;
    onCancel: () => void;
  } = $props();
</script>

<tr
  class:selected={isSelected}
  onclick={onSelect}
  role="row"
  tabindex="0"
>
  <td class="row-id">{rowIndex + 1}</td>
  {#each columns as col, colIdx}
    <td
      class="cell"
      class:editing={editingCol === colIdx}
      ondblclick={() => onEdit(colIdx)}
      role="gridcell"
    >
      {#if editingCol === colIdx}
        <CellEditor
          value={row.values[colIdx] ?? null}
          dtype={col.dtype}
          onSave={(val) => onSave(colIdx, val)}
          onCancel={onCancel}
        />
      {:else}
        <span class="cell-value">
          {formatCell(row.values[colIdx], col.dtype)}
        </span>
      {/if}
    </td>
  {/each}
  <td class="actions-col"></td>
</tr>

<script lang="ts" context="module">
  function formatCell(value: string | number | boolean | null, dtype: string): string {
    if (value === null || value === undefined) return '—';
    if (typeof value === 'boolean') return value ? '✓' : '✗';
    return String(value);
  }
</script>

<style>
  tr {
    cursor: pointer;
  }
  tr:hover {
    background: #f8f9fa;
  }
  tr.selected {
    background: #e8f0fe;
  }
  .row-id {
    text-align: center;
    color: #aaa;
    font-size: 12px;
    padding: 4px 8px;
  }
  .cell {
    padding: 4px 12px;
    border-bottom: 1px solid #f0f0f0;
    font-size: 13px;
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cell.editing {
    padding: 1px;
    overflow: visible;
  }
  .cell-value {
    display: block;
    min-height: 20px;
  }
  .actions-col {
    width: 40px;
  }
</style>
```

- [ ] **Step 4: Implement DataTable**

Write `src/lib/components/DataTable.svelte`:
```svelte
<script lang="ts">
  import TableHeader from './TableHeader.svelte';
  import TableRow from './TableRow.svelte';
  import { tableStore, displayedRows } from '../stores/table';
  import { editCell } from '../commands';
  import type { RowData } from '../types';

  let editingCell: { rowId: number; colIdx: number } | null = $state(null);

  let columns = $derived($tableStore.columns);
  let rows = $derived($displayedRows);
  let sort = $derived($tableStore.sort);
  let selectedRowIds = $derived($tableStore.selectedRowIds);

  function handleSort(col: string) {
    tableStore.toggleSort(col);
  }

  function handleRowSelect(row: RowData, e: MouseEvent) {
    tableStore.selectRow(row.row_id, e.shiftKey, e.ctrlKey || e.metaKey);
  }

  function handleEdit(row: RowData, colIdx: number) {
    editingCell = { rowId: row.row_id, colIdx };
  }

  async function handleSave(rowId: number, colIdx: number, value: string | number | boolean | null) {
    try {
      const result = await editCell(rowId, colIdx, value);
      tableStore.applyMutation(result);
    } catch (err) {
      console.error('Failed to edit cell:', err);
    }
    editingCell = null;
  }

  function handleCancel() {
    editingCell = null;
  }
</script>

<div class="table-wrapper">
  <table class="data-table" role="grid">
    <TableHeader {columns} {sort} onSort={handleSort} />
    <tbody>
      {#each rows as row (row.row_id)}
        <TableRow
          {row}
          {columns}
          rowIndex={$tableStore.rows.indexOf(row)}
          isSelected={selectedRowIds.has(row.row_id)}
          editingCol={editingCell?.rowId === row.row_id ? editingCell.colIdx : null}
          onSelect={(e) => handleRowSelect(row, e)}
          onEdit={(colIdx) => handleEdit(row, colIdx)}
          onSave={(colIdx, val) => handleSave(row.row_id, colIdx, val)}
          onCancel={handleCancel}
        />
      {:else}
        <tr>
          <td colspan={columns.length + 2} class="empty-state">
            No data
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .table-wrapper {
    flex: 1;
    overflow: auto;
  }
  .data-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }
  .empty-state {
    text-align: center;
    padding: 48px;
    color: #999;
    font-size: 14px;
  }
</style>
```

---

### Task 8: Frontend — SchemaPanel, Toolbar, SearchBar, StatusBar

**Files:**
- Create: `src/lib/components/SchemaPanel.svelte`
- Create: `src/lib/components/Toolbar.svelte`
- Create: `src/lib/components/SearchBar.svelte`
- Create: `src/lib/components/StatusBar.svelte`

- [ ] **Step 1: Implement Toolbar**

Write `src/lib/components/Toolbar.svelte`:
```svelte
<script lang="ts">
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { openFile, saveFile, saveFileAs, undo, redo } from '../commands';
  import { tableStore } from '../stores/table';

  async function handleOpen() {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Parquet', extensions: ['parquet'] }],
    });
    if (!selected) return;

    try {
      const meta = await openFile(selected);
      const { getAllRows } = await import('../commands');
      const rows = await getAllRows();

      tableStore.set({
        columns: meta.columns,
        rows,
        selectedRowIds: new Set(),
        sort: { column: '', direction: null },
        search: { query: '', column: null },
        modified: false,
        filePath: meta.file_path,
      });
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }

  async function handleSave() {
    try {
      await saveFile();
      tableStore.update(s => ({ ...s, modified: false }));
    } catch (err) {
      console.error('Failed to save:', err);
    }
  }

  async function handleSaveAs() {
    const selected = await save({
      filters: [{ name: 'Parquet', extensions: ['parquet'] }],
    });
    if (!selected) return;

    try {
      await saveFileAs(selected);
      tableStore.update(s => ({ ...s, modified: false, filePath: selected }));
    } catch (err) {
      console.error('Failed to save as:', err);
    }
  }

  async function handleUndo() {
    try {
      const result = await undo();
      tableStore.applyMutation(result);
    } catch (err) {
      console.error('Undo failed:', err);
    }
  }

  async function handleRedo() {
    try {
      const result = await redo();
      tableStore.applyMutation(result);
    } catch (err) {
      console.error('Redo failed:', err);
    }
  }

  import SearchBar from './SearchBar.svelte';

  let modified = $derived($tableStore.modified);
</script>

<div class="toolbar">
  <div class="toolbar-group">
    <button onclick={handleOpen} class="toolbar-btn">Open</button>
    <button onclick={handleSave} class="toolbar-btn" disabled={!modified}>Save</button>
    <button onclick={handleSaveAs} class="toolbar-btn">Save As</button>
  </div>
  <div class="toolbar-group">
    <button onclick={handleUndo} class="toolbar-btn" title="Undo">↩</button>
    <button onclick={handleRedo} class="toolbar-btn" title="Redo">↪</button>
  </div>
  <div class="toolbar-spacer"></div>
  <div class="toolbar-group">
    <SearchBar />
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    padding: 6px 12px;
    gap: 8px;
    border-bottom: 1px solid #e0e0e0;
    background: #fafafa;
  }
  .toolbar-group {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .toolbar-spacer {
    flex: 1;
  }
  .toolbar-btn {
    padding: 4px 12px;
    border: 1px solid #ddd;
    border-radius: 4px;
    background: white;
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
  }
  .toolbar-btn:hover:not(:disabled) {
    background: #f0f0f0;
  }
  .toolbar-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
```

- [ ] **Step 2: Implement SearchBar**

Write `src/lib/components/SearchBar.svelte`:
```svelte
<script lang="ts">
  import { tableStore } from '../stores/table';

  let query = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    query = target.value;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      tableStore.setSearch(query, null);
    }, 200);
  }

  function handleClear() {
    query = '';
    tableStore.setSearch('', null);
  }
</script>

<div class="search-bar">
  <span class="search-icon">🔍</span>
  <input
    type="text"
    placeholder="Search..."
    value={query}
    oninput={handleInput}
    class="search-input"
  />
  {#if query}
    <button onclick={handleClear} class="clear-btn">✕</button>
  {/if}
</div>

<style>
  .search-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    background: white;
    border: 1px solid #ddd;
    border-radius: 4px;
    padding: 2px 8px;
    min-width: 200px;
  }
  .search-icon {
    font-size: 12px;
    opacity: 0.5;
  }
  .search-input {
    border: none;
    outline: none;
    padding: 4px 0;
    font-size: 13px;
    flex: 1;
    font-family: inherit;
  }
  .clear-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 12px;
    padding: 2px;
    color: #999;
  }
  .clear-btn:hover {
    color: #333;
  }
</style>
```

- [ ] **Step 3: Implement SchemaPanel**

Write `src/lib/components/SchemaPanel.svelte`:
```svelte
<script lang="ts">
  import { tableStore } from '../stores/table';
  import { dropColumn, renameColumn, addColumn } from '../commands';

  let columns = $derived($tableStore.columns);
  let renamingCol: string | null = $state(null);
  let renameValue = $state('');
  let addingColumn = $state(false);
  let newColName = $state('');
  let newColType = $state('utf8');

  async function handleDrop(name: string) {
    if (!confirm(`Delete column "${name}"? This can be undone.`)) return;
    try {
      const result = await dropColumn(name);
      tableStore.applyMutation(result);
    } catch (err) {
      console.error('Failed to drop column:', err);
    }
  }

  function startRename(name: string) {
    renamingCol = name;
    renameValue = name;
  }

  async function finishRename() {
    if (renamingCol && renameValue && renameValue !== renamingCol) {
      try {
        const result = await renameColumn(renamingCol, renameValue);
        tableStore.applyMutation(result);
      } catch (err) {
        console.error('Failed to rename column:', err);
      }
    }
    renamingCol = null;
  }

  function handleRenameKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') finishRename();
    else if (e.key === 'Escape') renamingCol = null;
  }

  async function handleAddColumn() {
    if (!newColName) return;
    try {
      const result = await addColumn(newColName, newColType);
      tableStore.applyMutation(result);
      newColName = '';
      addingColumn = false;
    } catch (err) {
      console.error('Failed to add column:', err);
    }
  }
</script>

<aside class="schema-panel">
  <div class="panel-header">
    <h3>Schema</h3>
  </div>
  <div class="column-list">
    {#each columns as col}
      <div class="column-item">
        <div class="column-info">
          {#if renamingCol === col.name}
            <input
              type="text"
              bind:value={renameValue}
              onkeydown={handleRenameKeydown}
              onblur={finishRename}
              class="rename-input"
              autofocus
            />
          {:else}
            <span
              class="column-name"
              ondblclick={() => startRename(col.name)}
              title="Double-click to rename"
            >
              {col.name}
            </span>
          {/if}
          <span class="column-type">{col.dtype}</span>
        </div>
        <button
          class="drop-btn"
          onclick={() => handleDrop(col.name)}
          title="Drop column"
        >
          ✕
        </button>
      </div>
    {/each}
  </div>

  {#if addingColumn}
    <div class="add-column-form">
      <input
        type="text"
        placeholder="Column name"
        bind:value={newColName}
        class="add-input"
      />
      <select bind:value={newColType} class="add-select">
        <option value="utf8">Text</option>
        <option value="int64">Integer (64)</option>
        <option value="int32">Integer (32)</option>
        <option value="float64">Float</option>
        <option value="boolean">Boolean</option>
      </select>
      <button onclick={handleAddColumn} class="add-btn">Add</button>
      <button onclick={() => addingColumn = false} class="cancel-btn">Cancel</button>
    </div>
  {:else}
    <button onclick={() => addingColumn = true} class="add-column-btn">
      + Add column
    </button>
  {/if}
</aside>

<style>
  .schema-panel {
    width: 220px;
    min-width: 220px;
    border-right: 1px solid #e0e0e0;
    display: flex;
    flex-direction: column;
    background: #fafafa;
    overflow-y: auto;
  }
  .panel-header {
    padding: 12px;
    border-bottom: 1px solid #e0e0e0;
  }
  .panel-header h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: #555;
  }
  .column-list {
    flex: 1;
    overflow-y: auto;
  }
  .column-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    border-bottom: 1px solid #f0f0f0;
  }
  .column-item:hover {
    background: #f0f0f0;
  }
  .column-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .column-name {
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .column-type {
    font-size: 11px;
    color: #999;
  }
  .drop-btn {
    border: none;
    background: none;
    cursor: pointer;
    color: #ccc;
    font-size: 12px;
    padding: 2px 4px;
    visibility: hidden;
  }
  .column-item:hover .drop-btn {
    visibility: visible;
  }
  .drop-btn:hover {
    color: #e74c3c;
  }
  .rename-input {
    width: 100%;
    padding: 2px 4px;
    font-size: 13px;
    border: 1px solid #1a73e8;
    outline: none;
    border-radius: 2px;
    font-family: inherit;
    box-sizing: border-box;
  }
  .add-column-btn {
    margin: 8px 12px;
    padding: 6px;
    border: 1px dashed #ccc;
    border-radius: 4px;
    background: none;
    cursor: pointer;
    font-size: 12px;
    color: #666;
    font-family: inherit;
  }
  .add-column-btn:hover {
    border-color: #1a73e8;
    color: #1a73e8;
  }
  .add-column-form {
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-top: 1px solid #e0e0e0;
  }
  .add-input, .add-select {
    padding: 4px 6px;
    font-size: 12px;
    border: 1px solid #ddd;
    border-radius: 3px;
    font-family: inherit;
  }
  .add-btn, .cancel-btn {
    padding: 4px 8px;
    border: 1px solid #ddd;
    border-radius: 3px;
    background: white;
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
  }
  .add-btn {
    background: #1a73e8;
    color: white;
    border-color: #1a73e8;
  }
  .cancel-btn {
    margin-left: 4px;
  }
</style>
```

- [ ] **Step 4: Implement StatusBar**

Write `src/lib/components/StatusBar.svelte`:
```svelte
<script lang="ts">
  import { tableStore } from '../stores/table';

  let columns = $derived($tableStore.columns);
  let rows = $derived($tableStore.rows);
  let modified = $derived($tableStore.modified);
  let filePath = $derived($tableStore.filePath);
  let sort = $derived($tableStore.sort);

  let fileName = $derived(
    filePath ? filePath.split(/[/\\]/).pop() : 'Untitled'
  );
  let sortInfo = $derived(
    sort.direction ? `Sorted by: ${sort.column} (${sort.direction})` : ''
  );
</script>

<footer class="status-bar">
  <span class="status-item">
    {rows.length} rows · {columns.length} cols
  </span>
  {#if sortInfo}
    <span class="status-item">{sortInfo}</span>
  {/if}
  <span class="status-spacer"></span>
  <span class="status-item" class:modified={modified}>
    {modified ? '● Modified' : '○ Saved'}
  </span>
  <span class="status-item file-name">{fileName}</span>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    padding: 4px 12px;
    border-top: 1px solid #e0e0e0;
    background: #fafafa;
    font-size: 12px;
    color: #888;
    gap: 16px;
  }
  .status-item {
    white-space: nowrap;
  }
  .status-spacer {
    flex: 1;
  }
  .modified {
    color: #e67e22;
  }
  .file-name {
    color: #555;
    font-weight: 500;
  }
</style>
```

---

### Task 9: Frontend — Root App Shell + NestedValueModal

**Files:**
- Create: `src/App.svelte`
- Create: `src/lib/components/NestedValueModal.svelte`

- [ ] **Step 1: Implement App.svelte**

Write `src/App.svelte`:
```svelte
<script lang="ts">
  import Toolbar from './lib/components/Toolbar.svelte';
  import SchemaPanel from './lib/components/SchemaPanel.svelte';
  import DataTable from './lib/components/DataTable.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import { tableStore, displayedRows } from './lib/stores/table';
  import { deleteRows } from './lib/commands';

  let hasData = $derived($tableStore.columns.length > 0);
  let selectedIds = $derived($tableStore.selectedRowIds);

  // Keyboard handler for delete
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Delete' && selectedIds.size > 0) {
      handleDeleteSelected();
    }
  }

  async function handleDeleteSelected() {
    if (selectedIds.size === 0) return;
    if (!confirm(`Delete ${selectedIds.size} row(s)?`)) return;
    try {
      const result = await deleteRows([...selectedIds]);
      tableStore.applyMutation(result);
      tableStore.clearSelection();
    } catch (err) {
      console.error('Failed to delete rows:', err);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app">
  <Toolbar />
  <div class="main-content">
    {#if hasData}
      <SchemaPanel />
      <DataTable />
    {:else}
      <div class="welcome">
        <h1>Parquet Viewer</h1>
        <p>Open a .parquet file to get started.</p>
        <button
          onclick={async () => {
            const { open } = await import('@tauri-apps/plugin-dialog');
            const selected = await open({
              multiple: false,
              filters: [{ name: 'Parquet', extensions: ['parquet'] }],
            });
            if (!selected) return;
            const { openFile, getAllRows } = await import('./lib/commands');
            const meta = await openFile(selected);
            const rows = await getAllRows();
            tableStore.set({
              columns: meta.columns,
              rows,
              selectedRowIds: new Set(),
              sort: { column: '', direction: null },
              search: { query: '', column: null },
              modified: false,
              filePath: meta.file_path,
            });
          }}
          class="open-btn"
        >
          Open File
        </button>
      </div>
    {/if}
  </div>
  <StatusBar />
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    color: #333;
    background: white;
  }
  .main-content {
    display: flex;
    flex: 1;
    overflow: hidden;
  }
  .welcome {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: #888;
  }
  .welcome h1 {
    font-size: 24px;
    font-weight: 600;
    color: #333;
    margin: 0;
  }
  .welcome p {
    margin: 0;
    font-size: 14px;
  }
  .open-btn {
    padding: 8px 24px;
    border: 1px solid #1a73e8;
    border-radius: 6px;
    background: #1a73e8;
    color: white;
    cursor: pointer;
    font-size: 14px;
    font-family: inherit;
    margin-top: 8px;
  }
  .open-btn:hover {
    background: #1557b0;
  }
</style>
```

- [ ] **Step 2: Implement NestedValueModal**

Write `src/lib/components/NestedValueModal.svelte`:
```svelte
<script lang="ts">
  let { value, columnName, onClose }: {
    value: string;
    columnName: string;
    onClose: () => void;
  } = $props();

  let formattedJson = $derived(
    (() => {
      try {
        return JSON.stringify(JSON.parse(value), null, 2);
      } catch {
        return value;
      }
    })()
  );
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onClose()} />

<div class="modal-overlay" onclick={onClose} role="dialog">
  <div class="modal-content" onclick={(e) => e.stopPropagation()}>
    <div class="modal-header">
      <h3>{columnName}</h3>
      <button onclick={onClose} class="close-btn">✕</button>
    </div>
    <pre class="modal-body"><code>{formattedJson}</code></pre>
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal-content {
    background: white;
    border-radius: 8px;
    min-width: 400px;
    max-width: 80vw;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0,0,0,0.2);
  }
  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid #eee;
  }
  .modal-header h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .close-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 16px;
    color: #999;
    padding: 4px;
  }
  .close-btn:hover {
    color: #333;
  }
  .modal-body {
    padding: 16px;
    overflow: auto;
    margin: 0;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 13px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
```

- [ ] **Step 3: Update app.css**

Write or overwrite `src/app.css`:
```css
*,
*::before,
*::after {
  box-sizing: border-box;
}

html, body {
  margin: 0;
  padding: 0;
  height: 100%;
  overflow: hidden;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto,
    'Helvetica Neue', Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

/* Scrollbar styling */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: #ddd;
  border-radius: 4px;
}
::-webkit-scrollbar-thumb:hover {
  background: #bbb;
}
```

- [ ] **Step 4: Update main.ts**

Write `src/main.ts`:
```typescript
import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
```

---

### Task 10: Integration — Full Build Verification

**Files:**
- Modify: `src-tauri/tauri.conf.json` (adjust dev/build config)

- [ ] **Step 1: Verify frontend builds**

```bash
cd "D:\.projects\paraquet Viewer"
npm run build
```

Expected: Clean Vite build, outputs to `dist/`.

- [ ] **Step 2: Verify full Tauri build**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: Clean build with all Rust code and frontend assets.

- [ ] **Step 3: Run all Rust tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: All tests pass.

- [ ] **Step 4: Create a test Parquet file and verify round-trip**

Create `src-tauri/tests/test_data.rs`:
```rust
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::path::Path;
    use arrow::array::{Int64Array, StringArray, BooleanArray, Float64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet_viewer_lib::engine::parquet_io;
    use tempfile::NamedTempFile;

    fn create_complex_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Int64, true),
            Field::new("salary", DataType::Float64, true),
            Field::new("active", DataType::Boolean, false),
        ]));

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["Alice", "Bob", "Charlie", "Diana"])),
                Arc::new(Int64Array::from(vec![Some(30), Some(25), None, Some(28)])),
                Arc::new(Float64Array::from(vec![Some(75000.0), Some(82000.0), Some(95000.0), None])),
                Arc::new(BooleanArray::from(vec![true, false, true, true])),
            ],
        ).unwrap()
    }

    #[test]
    fn test_full_round_trip() {
        let batch = create_complex_batch();
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        // Write
        parquet_io::write_parquet(&path, &batch, None).unwrap();

        // Read
        let (read_batch, meta) = parquet_io::read_parquet(&path).unwrap();

        assert_eq!(read_batch.num_rows(), 4);
        assert_eq!(read_batch.num_columns(), 4);

        // Check data fidelity
        let names = read_batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(names.value(0), "Alice");
        assert_eq!(names.value(3), "Diana");

        let ages = read_batch.column(1).as_any().downcast_ref::<Int64Array>().unwrap();
        assert!(ages.is_null(2)); // Charlie's age is null
        assert_eq!(ages.value(1), 25);
    }
}
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: All tests including integration pass.

---

### Task 11: Final Polish — Error Handling & UX Details

**Files:**
- Modify: `src/lib/components/DataTable.svelte` (add keyboard nav)
- Modify: `src/App.svelte` (unsaved changes guard)
- Create: `src-tauri/src/lib.rs` (update for close guard if needed)

- [ ] **Step 1: Add keyboard navigation to DataTable**

Enhance `src/lib/components/DataTable.svelte` by adding arrow key navigation between rows/cells and ensure the component handles focus properly.

- [ ] **Step 2: Add unsaved changes guard in Rust**

No extra Rust code needed — Tauri v2's window close event can be handled from the frontend. Add to `src/App.svelte`:

```typescript
// Inside App.svelte <script>
import { getCurrentWindow } from '@tauri-apps/api/window';
import { onMount } from 'svelte';

onMount(() => {
  const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
    if ($tableStore.modified) {
      const confirmed = await confirm('You have unsaved changes. Close anyway?');
      if (!confirmed) {
        event.preventDefault();
      }
    }
  });
  return () => { unlisten.then(fn => fn()); };
});
```

- [ ] **Step 3: Verify everything still builds**

```bash
npm run build
cargo build --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: All clean.
