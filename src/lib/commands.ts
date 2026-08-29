import { invoke } from '@tauri-apps/api/core';
import type { ColumnInfo, RowData, QueryResult, MetadataJson } from './types';

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

export async function getPage(offset: number, limit: number): Promise<RowData[]> {
  return invoke('get_page', { offset, limit });
}

export async function getColumns(): Promise<ColumnInfo[]> {
  return invoke('get_columns');
}

export async function editCell(
  rowId: number,
  colIdx: number,
  value: string | number | boolean | null
): Promise<QueryResult> {
  return invoke('edit_cell', { row_id: rowId, col_idx: colIdx, value });
}

export async function deleteRows(rowIds: number[]): Promise<void> {
  return invoke('delete_rows', { row_ids: rowIds });
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
  return invoke('rename_column', { old_name: oldName, new_name: newName });
}

export async function executeSql(sql: string): Promise<QueryResult> {
  return invoke('execute_sql', { sql });
}

export async function sortBy(colName: string, ascending: boolean): Promise<RowData[]> {
  return invoke('sort_by', { col_name: colName, ascending });
}

export async function generateSchema(): Promise<string> {
  return invoke('generate_schema');
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
