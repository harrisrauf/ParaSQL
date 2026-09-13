import { invoke } from '@tauri-apps/api/core';
import type {
  ColumnInfo, RowData, QueryResult, MetadataJson, SearchResult,
  Workspace, TableMeta, TableSummary,
} from './types';

export async function openFile(path: string): Promise<MetadataJson> {
  return invoke('open_file', { path });
}

export async function openFolder(path: string): Promise<MetadataJson> {
  return invoke('open_folder', { path });
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
  can_undo: boolean;
  can_redo: boolean;
}> {
  return invoke('get_file_info');
}

export async function getAllRows(): Promise<RowData[]> {
  return invoke('get_all_rows');
}

export async function getPage(afterId: number | null, limit: number): Promise<RowData[]> {
  return invoke('get_page', { afterId, limit });
}

export async function searchRows(query: string, column: string | null = null): Promise<SearchResult> {
  return invoke('search_rows', { query, column });
}

export async function getColumns(): Promise<ColumnInfo[]> {
  return invoke('get_columns');
}

export async function editCell(
  rowId: number,
  colIdx: number,
  value: string | number | boolean | null
): Promise<QueryResult> {
  return invoke('edit_cell', { rowId, colIdx, value });
}

export async function deleteRows(rowIds: number[]): Promise<void> {
  return invoke('delete_rows', { rowIds });
}

export async function insertRow(): Promise<QueryResult> {
  return invoke('insert_row');
}

export async function addColumn(name: string, dtype: string): Promise<void> {
  return invoke('add_column', { name, dtype });
}

export async function dropColumn(name: string): Promise<void> {
  return invoke('drop_column', { name });
}

export async function renameColumn(oldName: string, newName: string): Promise<void> {
  return invoke('rename_column', { oldName, newName });
}

export async function executeSql(sql: string): Promise<QueryResult> {
  return invoke('execute_sql', { sql });
}

export async function generateSchema(dialect: string = 'duckdb'): Promise<string> {
  return invoke('generate_schema', { dialect });
}

export async function undo(): Promise<void> {
  return invoke('undo');
}

export async function redo(): Promise<void> {
  return invoke('redo');
}

export async function canUndo(): Promise<boolean> {
  return invoke('can_undo');
}

export async function canRedo(): Promise<boolean> {
  return invoke('can_redo');
}

export async function exportJson(path: string): Promise<void> {
  return invoke('export_json', { path });
}

export async function exportCsv(path: string): Promise<void> {
  return invoke('export_csv', { path });
}

export async function exportExcel(path: string): Promise<void> {
  return invoke('export_excel', { path });
}

export async function exportQuery(
  sql: string,
  outPath: string,
  format: 'csv' | 'excel' | 'parquet',
  compression = 'snappy'
): Promise<void> {
  return invoke('export_query', { sql, outPath, format, compression });
}

// --- Workspace commands ---

export function openWorkspace(path: string): Promise<Workspace> {
  return invoke('open_workspace', { path });
}

export function saveWorkspace(path: string, ws: Workspace): Promise<void> {
  return invoke('save_workspace', { path, ws });
}

export function syncWorkspaceTables(path: string, ws: Workspace): Promise<Workspace> {
  return invoke('sync_workspace_tables', { path, ws });
}

export function openTableForEdit(
  name: string,
  path: string,
  force: boolean,
  ws: Workspace
): Promise<{ opened: boolean; over_limit?: boolean; size_mb?: number }> {
  return invoke('open_table_for_edit', { name, path, force, ws });
}

export function closeEditor(): Promise<void> {
  return invoke('close_editor');
}

export function getTableMeta(path: string): Promise<TableMeta> {
  return invoke('get_table_meta', { path });
}

export function exportTable(name: string, outPath: string, compression: string): Promise<void> {
  return invoke('export_table', { name, outPath, compression });
}

export function summarizeTable(name: string): Promise<TableSummary> {
  return invoke('summarize_table', { name });
}

export function listParquetFiles(dir: string): Promise<string[]> {
  return invoke('list_parquet_files', { dir });
}
