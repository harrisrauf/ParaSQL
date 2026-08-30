mod commands;
mod engine;

use commands::AppState;
use std::sync::{Arc, Mutex};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            engine: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_file,
            commands::open_folder,
            commands::save_file,
            commands::save_file_as,
            commands::get_file_info,
            commands::get_all_rows,
            commands::get_page,
            commands::search_rows,
            commands::get_columns,
            commands::edit_cell,
            commands::delete_rows,
            commands::insert_row,
            commands::add_column,
            commands::drop_column,
            commands::rename_column,
            commands::execute_sql,
            commands::sort_by,
            commands::generate_schema,
            commands::undo,
            commands::redo,
            commands::can_undo,
            commands::can_redo,
            commands::export_json,
            commands::export_csv,
            commands::export_excel,
            commands::open_workspace,
            commands::save_workspace,
            commands::sync_workspace_tables,
            commands::open_table_for_edit,
            commands::close_editor,
            commands::get_table_meta,
            commands::export_table,
            commands::summarize_table,
            commands::list_parquet_files,
            commands::debug_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
