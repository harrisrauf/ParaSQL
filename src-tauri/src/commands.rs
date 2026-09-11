use std::sync::{Arc, Mutex};
use tauri::State;

use crate::engine::catalog;
use crate::engine::duckdb::DuckDbEngine;
use crate::engine::types::*;

pub struct AppState {
    pub engine: Arc<Mutex<Option<DuckDbEngine>>>,
}

/// Editor-only commands (mutations on the `working` table) need a table open.
fn ensure_editor(engine: &DuckDbEngine) -> Result<(), String> {
    if !engine.has_editor() {
        return Err("No table open for editing — open a table first".to_string());
    }
    Ok(())
}

/// Like `with_engine` but requires an editor table to be open.
async fn with_editor<T: Send + 'static>(
    state: State<'_, AppState>,
    f: impl FnOnce(&DuckDbEngine) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    with_engine(state, move |engine| {
        ensure_editor(engine)?;
        f(engine)
    })
    .await
}

/// Like `with_engine_mut` but requires an editor table to be open.
async fn with_editor_mut<T: Send + 'static>(
    state: State<'_, AppState>,
    f: impl FnOnce(&mut DuckDbEngine) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    with_engine_mut(state, move |engine| {
        ensure_editor(engine)?;
        f(engine)
    })
    .await
}

/// Run a read-only engine operation on the blocking thread pool so the UI thread stays responsive.
async fn with_engine<T: Send + 'static>(
    state: State<'_, AppState>,
    f: impl FnOnce(&DuckDbEngine) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let eng = engine.lock().map_err(|e| format!("Lock error: {}", e))?;
        let inner = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
        f(inner)
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

/// Run a mutating engine operation on the blocking thread pool.
async fn with_engine_mut<T: Send + 'static>(
    state: State<'_, AppState>,
    f: impl FnOnce(&mut DuckDbEngine) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut eng = engine.lock().map_err(|e| format!("Lock error: {}", e))?;
        let inner = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
        f(inner)
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> Result<MetadataJson, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let new_engine = DuckDbEngine::open_parquet(&path)?;
        let columns = new_engine.column_info()?;
        let total_rows = new_engine.row_count();
        let file_path = new_engine
            .file_path()
            .map(|p| p.to_string_lossy().to_string());

        let mut eng = engine.lock().map_err(|e| format!("Lock error: {}", e))?;
        *eng = Some(new_engine);

        Ok(MetadataJson {
            columns,
            total_rows,
            file_path,
        })
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub async fn open_folder(path: String, state: State<'_, AppState>) -> Result<MetadataJson, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let new_engine = DuckDbEngine::open_folder(&path)?;
        let columns = new_engine.column_info()?;
        let total_rows = new_engine.row_count();
        let file_path = new_engine
            .file_path()
            .map(|p| p.to_string_lossy().to_string());

        let mut eng = engine.lock().map_err(|e| format!("Lock error: {}", e))?;
        *eng = Some(new_engine);

        Ok(MetadataJson {
            columns,
            total_rows,
            file_path,
        })
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub async fn save_file(state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| {
        let path = engine
            .file_path()
            .ok_or_else(|| "No file path set".to_string())?
            .to_string_lossy()
            .to_string();
        engine.export_parquet(&path)?;
        engine.mark_clean();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn save_file_as(path: String, state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| {
        engine.export_parquet(&path)?;
        engine.set_file_path(&path);
        engine.mark_clean();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn get_file_info(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_editor(state, move |engine| {
        let columns = engine.column_info()?;
        Ok(serde_json::json!({
            "rows": engine.row_count(),
            "cols": columns.len(),
            "modified": engine.is_dirty(),
            "path": engine.file_path().map(|p| p.to_string_lossy().to_string()),
            "columns": columns,
            "can_undo": engine.can_undo(),
            "can_redo": engine.can_redo(),
        }))
    })
    .await
}

#[tauri::command]
pub async fn get_all_rows(state: State<'_, AppState>) -> Result<Vec<RowData>, String> {
    with_editor(state, move |engine| engine.get_all_rows()).await
}

#[tauri::command]
pub async fn get_page(
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<RowData>, String> {
    with_editor(state, move |engine| engine.get_page(offset, limit)).await
}

#[tauri::command]
pub async fn search_rows(
    query: String,
    column: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    with_engine(state, move |engine| {
        let (rows, truncated) = engine.search_rows(&query, column.as_deref())?;
        Ok(serde_json::json!({ "rows": rows, "truncated": truncated }))
    })
    .await
}

#[tauri::command]
pub async fn get_columns(state: State<'_, AppState>) -> Result<Vec<ColumnInfo>, String> {
    with_editor(state, move |engine| engine.column_info()).await
}

#[tauri::command]
pub async fn edit_cell(
    row_id: u64,
    col_idx: usize,
    value: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<QueryResult, String> {
    with_editor_mut(state, move |engine| {
        let columns = engine.column_info()?;
        engine.edit_cell(row_id, col_idx, &value, &columns)
    })
    .await
}

#[tauri::command]
pub async fn delete_rows(row_ids: Vec<u64>, state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.delete_rows(&row_ids)).await
}

#[tauri::command]
pub async fn insert_row(state: State<'_, AppState>) -> Result<QueryResult, String> {
    with_editor_mut(state, move |engine| engine.insert_row()).await
}

#[tauri::command]
pub async fn add_column(name: String, dtype: String, state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.add_column(&name, &dtype)).await
}

#[tauri::command]
pub async fn drop_column(name: String, state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.drop_column(&name)).await
}

#[tauri::command]
pub async fn rename_column(
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.rename_column(&old_name, &new_name)).await
}

#[tauri::command]
pub async fn execute_sql(sql: String, state: State<'_, AppState>) -> Result<QueryResult, String> {
    with_engine(state, move |engine| engine.execute_sql(&sql)).await
}

#[tauri::command]
pub async fn sort_by(
    col_name: String,
    ascending: bool,
    state: State<'_, AppState>,
) -> Result<Vec<RowData>, String> {
    with_editor(state, move |engine| engine.sort_by(&col_name, ascending)).await
}

#[tauri::command]
pub async fn generate_schema(
    state: State<'_, AppState>,
    dialect: Option<String>,
) -> Result<String, String> {
    with_editor(state, move |engine| {
        engine.generate_schema_sql(&dialect.unwrap_or_else(|| "duckdb".to_string()))
    })
    .await
}

#[tauri::command]
pub async fn undo(state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.undo()).await
}

#[tauri::command]
pub async fn redo(state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.redo()).await
}

#[tauri::command]
pub async fn can_undo(state: State<'_, AppState>) -> Result<bool, String> {
    with_editor(state, move |engine| Ok(engine.can_undo())).await
}

#[tauri::command]
pub async fn can_redo(state: State<'_, AppState>) -> Result<bool, String> {
    with_editor(state, move |engine| Ok(engine.can_redo())).await
}

#[tauri::command]
pub async fn export_json(path: String, state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.export_json(&path)).await
}

#[tauri::command]
pub async fn export_csv(path: String, state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.export_csv(&path)).await
}

#[tauri::command]
pub async fn export_excel(path: String, state: State<'_, AppState>) -> Result<(), String> {
    with_editor_mut(state, move |engine| engine.export_excel(&path)).await
}

// --- Workspace (multi-file "database") commands ---

fn workspace_dir(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn open_workspace(path: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let ws = catalog::load_workspace(&path)?;
        let new_engine = DuckDbEngine::open_workspace(&ws)?;
        let mut eng = engine.lock().map_err(|e| format!("Lock error: {}", e))?;
        *eng = Some(new_engine);
        serde_json::to_value(&ws).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub async fn save_workspace(path: String, ws: catalog::Workspace, _state: State<'_, AppState>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut doc = ws;
        doc.dir = Some(workspace_dir(&path));
        doc.relativize_paths();
        catalog::write_workspace(&path, &doc)
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub async fn sync_workspace_tables(
    path: String,
    ws: catalog::Workspace,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut doc = ws;
        doc.dir = Some(workspace_dir(&path));
        doc.resolve_paths();
        let mut eng = engine.lock().map_err(|e| format!("Lock error: {}", e))?;
        let inner = eng.as_mut().ok_or_else(|| "No workspace open".to_string())?;
        inner.sync_workspace_tables(&doc)?;
        serde_json::to_value(&doc).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub async fn open_table_for_edit(
    name: String,
    path: String,
    force: bool,
    ws: catalog::Workspace,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let doc = ws;
        let size_mb = std::fs::metadata(&path)
            .map(|m| m.len() as f64 / (1024.0 * 1024.0))
            .unwrap_or(0.0);
        let limit = doc.edit_size_limit_mb.unwrap_or(u64::MAX) as f64;
        if !force && limit != f64::MAX && size_mb > limit {
            return Ok(serde_json::json!({
                "opened": false,
                "over_limit": true,
                "size_mb": size_mb.round(),
            }));
        }
        let mut eng = engine.lock().map_err(|e| format!("Lock error: {}", e))?;
        let inner = eng.as_mut().ok_or_else(|| "No workspace open".to_string())?;
        inner.open_editor_table(&name, &path)?;
        Ok(serde_json::json!({
            "opened": true,
            "size_mb": size_mb.round(),
        }))
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub async fn close_editor(state: State<'_, AppState>) -> Result<(), String> {
    with_engine_mut(state, move |engine| engine.close_editor()).await
}

#[tauri::command]
pub async fn get_table_meta(path: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_engine(state, move |engine| engine.get_table_meta(&path)).await
}

#[tauri::command]
pub async fn export_table(
    name: String,
    out_path: String,
    compression: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_engine(state, move |engine| engine.export_table(&name, &out_path, &compression)).await
}

#[tauri::command]
pub async fn summarize_table(name: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    with_engine(state, move |engine| engine.summarize_table(&name)).await
}

/// Recursively list .parquet files under a directory (for folder import).
#[tauri::command]
pub async fn list_parquet_files(dir: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(&dir)];
        while let Some(p) = stack.pop() {
            match std::fs::read_dir(&p) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            stack.push(path);
                        } else if path
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("parquet"))
                        {
                            out.push(path.to_string_lossy().to_string());
                        }
                    }
                }
                Err(_) => continue,
            }
        }
        out.sort();
        Ok(out)
    })
    .await
    .map_err(|e| format!("Background task error: {}", e))?
}

#[tauri::command]
pub fn debug_log(msg: String) {
    println!("[webview] {}", msg);
}