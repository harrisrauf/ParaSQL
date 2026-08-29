# Parquet Viewer — Design Spec

**Date:** 2026-05-28
**Platform:** Tauri v2 (Rust backend + Svelte frontend)
**Data size target:** ≤30MB files (full in-memory), pagination later

## 1. Architecture

```
Tauri Shell (Rust)
├── Data Engine
│   ├── Parquet Reader/Writer        (parquet + arrow crate)
│   ├── Arrow RecordBatch (in-memory) (arrow crate)
│   ├── Mutation Engine              (edit, delete, insert, schema ops)
│   └── File Metadata Store          (preserves original parquet settings)
├── OpLog (Undo/Redo)                (bounded operation history)
└── Tauri Commands (IPC Bridge)
        │
        │ Arrow IPC binary (via Tauri asset protocol)
        │ + JSON metadata
        ▼
Svelte Frontend
├── Stores (table state, selection, search/sort params)
├── Components (DataTable, Toolbar, SchemaPanel, StatusBar)
└── @apache-arrow JS bindings (binary deserialization)
```

### 1.1 Core Principle: Rust owns authoritative state

- Rust maintains the sole authoritative Arrow `RecordBatch`
- Every mutation goes through a `#[tauri::command]` that updates the RecordBatch, records an op in the OpLog, and returns incremental patch data
- Frontend is a rendering layer — all data integrity guaranteed by Rust

### 1.2 IPC Data Format

- **Metadata**: JSON (<1KB) — row count, column names, types, `_row_id` range
- **Data payload**: Arrow IPC binary via Tauri's asset protocol, parsed on the JS side with `@apache-arrow/esnext` official bindings
- This preserves int64, Decimal128, timestamps, and other types that JSON would corrupt

### 1.3 Mutation Response Format

Mutations return **incremental patches**, not full snapshots:

```rust
struct MutationResult {
    affected: Vec<RowData>,        // changed/inserted rows
    deleted_ids: Vec<u64>,         // removed row IDs
    schema_changed: bool,          // if true, frontend does full reload
    new_schema: Option<Schema>,    // present when schema_changed
}
```

## 2. Row Identity

Every row has a stable auto-incrementing `_row_id` (`u64`) assigned on file load.

- Frontend receives `_row_id` as metadata (not displayed)
- Svelte uses `_row_id` for `{#key}` blocks and selection tracking
- All mutation commands reference rows by `_row_id`, not by view index
- Sorting on the frontend is purely a display concern — mutations always reference the stable ID

```rust
struct RowEntry {
    id: u64,
    data: Vec<ScalarValue>,
}
```

## 3. Data Engine (Rust)

### 3.1 File Structure

```
src-tauri/src/engine/
├── mod.rs              # Exports, AppState (Mutex<Engine>)
├── engine.rs           # Engine struct — owns RecordBatch, OpLog, FileMetadata
├── parquet_io.rs       # read_parquet(path), write_parquet(path, engine)
├── mutation.rs         # edit_cell, delete_rows, insert_row, add_column, drop_column, rename_column
└── types.rs            # RowData, MutationResult, ColumnInfo, FileMetadata, OpLog types
```

### 3.2 Core Types

```rust
struct Engine {
    batch: RecordBatch,
    row_ids: Vec<u64>,              // stable row IDs parallel to batch
    next_row_id: u64,
    schema: SchemaRef,
    file_metadata: Option<FileMetadata>,
    modified: bool,
    oplog: OpLog,
}

struct FileMetadata {
    path: Option<PathBuf>,
    compression: Compression,
    row_group_size: usize,
    data_page_size: usize,
    writer_version: String,
}

struct OpLog {
    entries: Vec<OpEntry>,
    position: usize,        // current position for undo/redo
    max_entries: usize,      // default 500
}

enum OpEntry {
    EditCell { row_id: u64, col_idx: usize, old: ScalarValue, new: ScalarValue },
    DeleteRows { rows: Vec<(u64, Vec<ScalarValue>)> },
    InsertRows { row_ids: Vec<u64>, index: usize },
    AddColumn { name: String, dtype: DataType },
    DropColumn { name: String, data: Vec<ScalarValue> },
    RenameColumn { old_name: String, new_name: String },
    Sort { column: String, descending: bool, prev_order: Vec<u64> },
}
```

### 3.3 File I/O

- **Open**: Read Parquet, extract file metadata (compression, row group size etc.), build `RecordBatch` + `row_ids`
- **Save**: Re-encode using original `FileMetadata` settings; update `modified = false`
- **Save As**: Use default settings (Snappy, 1MB row groups) unless user configures; update `path`
- **New file**: Empty `RecordBatch` with no columns, default settings

### 3.4 Mutation Behavior

| Operation | Rust behavior | Frontend effect |
|---|---|---|
| `edit_cell(row_id, col, value)` | Update ScalarValue in batch, record OpEntry | Patch single row in store |
| `delete_rows([row_id, ...])` | Remove from batch + row_ids, record OpEntry | Remove rows from store |
| `insert_row(index)` | Create empty row, assign new `_row_id` | Insert row in store |
| `add_column(name, dtype)` | Append column with nulls | Schema reload |
| `drop_column(name)` | Remove column, record data for undo | Schema reload |
| `rename_column(old, new)` | Rename in schema | Schema reload |
| `undo()` | Reverse current OpEntry, decrement position | Patch or schema reload |
| `redo()` | Re-apply next OpEntry, increment position | Patch or schema reload |

When `schema_changed = true`, frontend requests a fresh Arrow IPC binary for full reload.

## 4. Frontend (Svelte)

### 4.1 File Structure

```
src/
├── App.svelte
├── main.ts
├── lib/
│   ├── stores/
│   │   └── table.ts              # Writable store for current data view
│   ├── commands.ts               # Typed Tauri invoke() wrappers
│   ├── components/
│   │   ├── DataTable.svelte      # Main table, header + rows, keyboard nav
│   │   ├── CellEditor.svelte     # Inline edit, type-aware input
│   │   ├── Toolbar.svelte        # Open, Save, Save As, Undo, Redo
│   │   ├── SearchBar.svelte      # Full-text + column-specific search
│   │   ├── SchemaPanel.svelte    # Column list, visibility toggles, add/drop/rename
│   │   ├── StatusBar.svelte      # Row count, modified flag, current sort info
│   │   └── NestedValueModal.svelte # Modal for list/map/struct display
│   └── types.ts
```

### 4.2 Layout

```
┌────────────────────────────────────────────────────────────┐
│ [Open] [Save] [↩ Undo] [↪ Redo]    [🔍 Search...]    [⋮]  │
├──────────┬─────────────────────────────────────────────────┤
│ Schema   │  col_a ▲ │ col_b │ col_c ▼ │ col_d             │
│ Panel    │──────────┼────────┼─────────┼───────────────────   │
│          │  val     │ val    │ val     │ val               │
│ col_a ☑  │  val     │ val    │ val     │ val               │
│ col_b ☑  │                                                   │
│ col_c ☐  │                                                   │
│ [+ add]  │                                                   │
├──────────┴─────────────────────────────────────────────────┤
│ 42 rows · Modified · Sorted by: col_a (asc)                 │
└─────────────────────────────────────────────────────────────┘
```

### 4.3 Key UX Details

- **Sort**: Click column header to cycle: none → asc → desc → none. Active sort shown with `▲`/`▼`. Sort is local on the frontend snapshot.
- **Search**: Debounced input. Filters frontend snapshot locally (instant). Highlights matching cells. Option to restrict search to a specific column.
- **Edit**: Double-click cell → `CellEditor` opens inline. Type-aware: text input for strings, number input for numerics, date picker for dates, checkbox for bools, dropdown for enums.
- **Delete**: Select rows (click row, shift+click range, Ctrl+click multi), then Delete key or right-click → Delete Rows.
- **Schema Panel**: Left sidebar, collapsible. Lists all columns with checkboxes (visibility toggle). Click name to rename. "x" button to drop with confirmation. "+ add" at bottom to add new column (name + type picker).
- **Keyboard navigation**: Arrow keys to move between cells, Enter to edit, Tab to move to next cell, Escape to cancel edit.
- **Unsaved changes**: Tauri's `on_window_event` + `CloseRequested` handler shows native confirm dialog.
- **Nested types**: Struct columns show as expandable sub-headers (`address.street`). List/map columns show JSON preview in cell, click to open `NestedValueModal` with syntax-highlighted view.

### 4.4 Parquet Type → Frontend Mapping

| Parquet Type | Frontend Display | Edit Control |
|---|---|---|
| BOOLEAN | ✓/✗ | Checkbox |
| INT32 | number | Number input |
| INT64 | number | Number input (safe range), string if >2^53 |
| FLOAT/DOUBLE | number | Number input |
| BYTE_ARRAY / UTF8 | string | Text input |
| DATE | 2026-05-28 | Date input |
| TIMESTAMP (ms/ns) | 2026-05-28 14:30:00 | Datetime input |
| DECIMAL | number | Number input (precision-aware) |
| STRUCT | Expandable sub-columns | Per-field editing |
| LIST | [val, val, ...] | JSON textarea in modal |
| MAP | {k: v, ...} | JSON textarea in modal |

## 5. Tauri Commands

| Command | Args | Returns |
|---|---|---|
| `open_file_dialog` | — | `{ path: String, metadata: MetadataJson }` |
| `open_file_data` | path: String | (Arrow IPC binary via asset protocol, served from path returned by dialog) |
| `edit_cell` | row_id: u64, col_idx: usize, value: String | `MutationResult` |
| `save_file` | — | `void` |
| `save_file_as_dialog` | — | `void` |
| `edit_cell` | row_id, col_idx, value | `MutationResult` |
| `delete_rows` | row_ids: Vec<u64> | `MutationResult` |
| `insert_row` | index: usize | `MutationResult` |
| `add_column` | name, dtype | `MutationResult` |
| `drop_column` | name | `MutationResult` |
| `rename_column` | old, new | `MutationResult` |
| `undo` | — | `MutationResult` |
| `redo` | — | `MutationResult` |
| `get_data_slice` | start, end | (Arrow IPC binary) |
| `get_file_info` | — | `{ path, size, compression, rows, cols }` |

## 6. Undo/Redo Details

- Oplog stores complete reverse information for every operation
- `undo()`: reads `entries[position]`, performs inverse, decrements `position`
- `redo()`: reads `entries[position + 1]`, performs forward, increments `position`
- Any new mutation after undoing truncates the redo stack (`entries.truncate(position)`)
- Max 500 entries (configurable); old entries dropped from front when full
- `EditCell` undo simply stores the previous `ScalarValue`
- `DeleteRows` undo stores the full row data including `_row_id`
- `DropColumn` undo stores the entire column data array

## 7. File Format Preservation

- On open: read writer metadata from Parquet footer → store in `FileMetadata`
- On save: use stored `FileMetadata` values (compression codec, row group size, data page size, encoding)
- On "Save As": use Parquet defaults (Snappy, 1024 row group size, 1MB data page)
- If metadata cannot be read (corrupt/legacy file): fall back to defaults, status bar shows "Default encoding"
- New unsaved file: defaults, no file path

## 8. Future Scalability (Not in v0)

- **Pagination**: When files exceed threshold, load page-by-page using `get_data_slice` with row offsets
- **Virtual scrolling**: Replace naive table rendering with windowed rendering for thousands of rows
- **SQL query mode**: Pass-through SQL queries via DataFusion on the Rust side
- **Export**: CSV, JSON, SQLite export commands
- **Bulk edit**: Multi-cell paste, formula-like column transformations
- **Comparison/diff**: Side-by-side diff of two Parquet files
- **Column statistics**: Min/max/null count/distinct per column

## 9. Error Handling

- All Tauri commands return `Result<T, String>` — frontend shows toast on error
- Parquet parse errors → descriptive message with file offset if available
- Schema mismatch on edit (wrong type for column) → reject with type explanation
- Disk full on save → catch IO error, show "disk full" message, mark modified=true
- Concurrent file modification → stat file on save, warn if mtime changed

## 10. Testing Strategy

- **Rust unit tests**: `engine::mutation` — all CRUD operations, undo/redo sequences, edge cases
- **Rust integration tests**: Round-trip: create RecordBatch → write Parquet → read back → verify identical
- **Frontend tests**: Component rendering tests with mock Tauri invoke
- **Manual test plan**: Open 5 different Parquet files (various schemas), exercise all mutations, save, re-open, verify
