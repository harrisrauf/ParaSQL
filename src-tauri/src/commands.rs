use std::sync::Mutex;
use tauri::State;

use crate::engine::duckdb::DuckDbEngine;
use crate::engine::types::*;

pub struct AppState {
    pub engine: Mutex<Option<DuckDbEngine>>,
}

#[tauri::command]
pub fn open_file(path: String, state: State<AppState>) -> Result<MetadataJson, String> {
    let engine = DuckDbEngine::open_parquet(&path)?;
    let cols = engine.column_info()?;
    let total_rows = engine.row_count();
    let meta = engine.file_metadata().cloned();

    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    *eng = Some(engine);

    Ok(MetadataJson {
        columns: cols,
        total_rows,
        file_path: meta.and_then(|m| m.path.map(|p| p.to_string_lossy().to_string())),
    })
}

#[tauri::command]
pub fn open_folder(path: String, state: State<AppState>) -> Result<MetadataJson, String> {
    let engine = DuckDbEngine::open_folder(&path)?;
    let cols = engine.column_info()?;
    let total_rows = engine.row_count();
    let meta = engine.file_metadata().cloned();

    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    *eng = Some(engine);

    Ok(MetadataJson {
        columns: cols,
        total_rows,
        file_path: meta.and_then(|m| m.path.map(|p| p.to_string_lossy().to_string())),
    })
}

#[tauri::command]
pub fn save_file(state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    let path = engine.file_path().ok_or_else(|| "No file path set".to_string())?;
    let path_str = path.to_string_lossy().to_string();
    engine.export_parquet(&path_str)
}

#[tauri::command]
pub fn save_file_as(path: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.export_parquet(&path)?;
    engine.set_file_path(&path);
    Ok(())
}

#[tauri::command]
pub fn get_file_info(state: State<AppState>) -> Result<serde_json::Value, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    let columns = engine.column_info()?;
    Ok(serde_json::json!({
        "rows": engine.row_count(),
        "cols": columns.len(),
        "modified": false,
        "path": engine.file_path().map(|p| p.to_string_lossy().to_string()),
        "columns": columns,
        "can_undo": engine.can_undo(),
        "can_redo": engine.can_redo(),
    }))
}

#[tauri::command]
pub fn get_all_rows(state: State<AppState>) -> Result<Vec<RowData>, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    engine.get_all_rows()
}

#[tauri::command]
pub fn get_page(offset: usize, limit: usize, state: State<AppState>) -> Result<Vec<RowData>, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    engine.get_page(offset, limit)
}

#[tauri::command]
pub fn get_columns(state: State<AppState>) -> Result<Vec<ColumnInfo>, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    engine.column_info()
}

#[tauri::command]
pub fn edit_cell(row_id: u64, col_idx: usize, value: serde_json::Value, state: State<AppState>) -> Result<QueryResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    let columns = engine.column_info()?;
    engine.edit_cell(row_id, col_idx, &value, &columns)
}

#[tauri::command]
pub fn delete_rows(row_ids: Vec<u64>, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.delete_rows(&row_ids)
}

#[tauri::command]
pub fn insert_row(state: State<AppState>) -> Result<QueryResult, String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.insert_row()
}

#[tauri::command]
pub fn add_column(name: String, dtype: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.add_column(&name, &dtype)
}

#[tauri::command]
pub fn drop_column(name: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.drop_column(&name)
}

#[tauri::command]
pub fn rename_column(old_name: String, new_name: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.rename_column(&old_name, &new_name)
}

#[tauri::command]
pub fn execute_sql(sql: String, state: State<AppState>) -> Result<QueryResult, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    engine.execute_sql(&sql)
}

#[tauri::command]
pub fn sort_by(col_name: String, ascending: bool, state: State<AppState>) -> Result<Vec<RowData>, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    engine.sort_by(&col_name, ascending)
}

#[tauri::command]
pub fn generate_schema(state: State<AppState>) -> Result<String, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    engine.generate_schema_sql()
}

#[tauri::command]
pub fn undo(state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.undo()
}

#[tauri::command]
pub fn redo(state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.redo()
}

#[tauri::command]
pub fn can_undo(state: State<AppState>) -> Result<bool, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    Ok(engine.can_undo())
}

#[tauri::command]
pub fn can_redo(state: State<AppState>) -> Result<bool, String> {
    let eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_ref().ok_or_else(|| "No file open".to_string())?;
    Ok(engine.can_redo())
}

#[tauri::command]
pub fn export_json(path: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.export_json(&path)
}

#[tauri::command]
pub fn export_csv(path: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.export_csv(&path)
}

#[tauri::command]
pub fn export_excel(path: String, state: State<AppState>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| format!("Lock error: {}", e))?;
    let engine = eng.as_mut().ok_or_else(|| "No file open".to_string())?;
    engine.export_excel(&path)
}
