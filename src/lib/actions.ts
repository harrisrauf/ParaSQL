import { get } from 'svelte/store';
import { open, save } from '@tauri-apps/plugin-dialog';
import * as cmds from './commands';
import { tableStore, displayedRows, selectedRowIds, initialLoadThreshold } from './stores/table';
import { settings } from './stores/settings';
import type { RowData } from './types';

function pushRecent(path: string | null | undefined) {
  if (path) settings.pushRecentFile(path);
}

function cellText(v: string | number | boolean | null | undefined): string {
  if (v === null || v === undefined) return '';
  return String(v);
}

function tsvQuote(s: string): string {
  if (s.includes('\t') || s.includes('\n') || s.includes('"')) {
    return `"${s.replace(/"/g, '""')}"`;
  }
  return s;
}

// --- File flows ---

export async function openFileFlow(path?: string): Promise<boolean> {
  let selected = path;
  if (!selected) {
    const res = await open({
      multiple: false,
      filters: [{ name: 'Parquet', extensions: ['parquet'] }],
    });
    if (!res) return false;
    selected = res;
  }
  const meta = await cmds.openFile(selected);
  let rows: RowData[];
  if (meta.total_rows > initialLoadThreshold) {
    rows = await cmds.getPage(0, 10_000);
  } else {
    rows = await cmds.getAllRows();
  }
  tableStore.open(meta.columns, rows, meta.total_rows, meta.file_path ?? selected);
  pushRecent(meta.file_path ?? selected);
  return true;
}

export async function openFolderFlow(): Promise<boolean> {
  const selected = await open({ directory: true, multiple: false });
  if (!selected) return false;
  const meta = await cmds.openFolder(selected);
  let rows: RowData[];
  if (meta.total_rows > initialLoadThreshold) {
    rows = await cmds.getPage(0, 10_000);
  } else {
    rows = await cmds.getAllRows();
  }
  tableStore.open(meta.columns, rows, meta.total_rows, meta.file_path ?? selected);
  pushRecent(meta.file_path ?? selected);
  return true;
}

export async function saveFlow(): Promise<boolean> {
  try {
    await cmds.saveFile();
    tableStore.update(s => ({ ...s, modified: false }));
    return true;
  } catch (err) {
    console.error('Failed to save:', err);
    return false;
  }
}

export async function saveAsFlow(): Promise<boolean> {
  const selected = await save({
    filters: [{ name: 'Parquet', extensions: ['parquet'] }],
  });
  if (!selected) return false;
  try {
    await cmds.saveFileAs(selected);
    tableStore.update(s => ({ ...s, modified: false, filePath: selected }));
    pushRecent(selected);
    return true;
  } catch (err) {
    console.error('Failed to save as:', err);
    return false;
  }
}

export async function exportFlow(format: 'json' | 'csv' | 'excel'): Promise<boolean> {
  const ext = format === 'excel' ? 'xlsx' : format;
  const selected = await save({
    filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
  });
  if (!selected) return false;
  try {
    if (format === 'json') await cmds.exportJson(selected);
    else if (format === 'csv') await cmds.exportCsv(selected);
    else await cmds.exportExcel(selected);
    return true;
  } catch (err) {
    console.error(`Failed to export ${format}:`, err);
    return false;
  }
}

// --- Edit flows ---

export async function undoFlow(): Promise<void> {
  try {
    await cmds.undo();
    const [rows, columns] = await Promise.all([cmds.getAllRows(), cmds.getColumns()]);
    tableStore.update(s => ({
      ...s,
      rows,
      columns,
      sqlResult: null,
      savedTable: null,
      selection: { anchor: null, cells: new Set(), allRows: false },
      filters: {},
    }));
  } catch (err) {
    console.error('Failed to undo:', err);
  }
}

export async function redoFlow(): Promise<void> {
  try {
    await cmds.redo();
    const [rows, columns] = await Promise.all([cmds.getAllRows(), cmds.getColumns()]);
    tableStore.update(s => ({
      ...s,
      rows,
      columns,
      sqlResult: null,
      savedTable: null,
      selection: { anchor: null, cells: new Set(), allRows: false },
      filters: {},
    }));
  } catch (err) {
    console.error('Failed to redo:', err);
  }
}

export async function deleteSelectedFlow(): Promise<boolean> {
  const ids = [...get(selectedRowIds)];
  if (ids.length === 0) return false;
  if (!confirm(`Delete ${ids.length} row(s)?`)) return false;
  try {
    await cmds.deleteRows(ids);
    tableStore.removeRows(ids);
    tableStore.update(s => ({ ...s, modified: true }));
    return true;
  } catch (err) {
    console.error('Failed to delete rows:', err);
    return false;
  }
}

export async function insertRowFlow(): Promise<void> {
  try {
    const result = await cmds.insertRow();
    const row = result.rows[0];
    if (row) {
      tableStore.update(s => ({
        ...s,
        rows: [...s.rows, row],
        totalRows: s.totalRows + 1,
        modified: true,
      }));
    }
  } catch (err) {
    console.error('Failed to insert row:', err);
  }
}

// --- Clipboard ---

export async function copySelectionToClipboard(): Promise<void> {
  const state = get(tableStore);
  const cols = state.columns;
  if (cols.length === 0) return;

  if (state.selection.allRows) {
    const rows = get(displayedRows);
    const lines = rows.map(r => cols.map((_, i) => tsvQuote(cellText(r.values[i]))).join('\t'));
    await navigator.clipboard.writeText(lines.join('\n'));
    return;
  }

  const cells = state.selection.cells;
  if (cells.size === 0) return;

  // Group selected cells by row, compute the bounding column range
  const byRow = new Map<number, Set<number>>();
  let cMin = Infinity;
  let cMax = -1;
  for (const key of cells) {
    const sep = key.indexOf(':');
    const rid = Number(key.slice(0, sep));
    const cidx = Number(key.slice(sep + 1));
    cMin = Math.min(cMin, cidx);
    cMax = Math.max(cMax, cidx);
    let rowSet = byRow.get(rid);
    if (!rowSet) {
      rowSet = new Set();
      byRow.set(rid, rowSet);
    }
    rowSet.add(cidx);
  }
  const rowIndex = new Map(get(displayedRows).map((r, i) => [r.row_id, i]));
  const lines: string[] = [];
  const ordered = [...byRow.entries()].sort((a, b) => (rowIndex.get(a[0]) ?? 0) - (rowIndex.get(b[0]) ?? 0));
  for (const [rid, colSet] of ordered) {
    const row = get(tableStore).rows.find(r => r.row_id === rid);
    if (!row) continue;
    const cellsLine: string[] = [];
    for (let c = cMin; c <= cMax; c++) {
      cellsLine.push(colSet.has(c) ? tsvQuote(cellText(row.values[c])) : '');
    }
    lines.push(cellsLine.join('\t'));
  }
  await navigator.clipboard.writeText(lines.join('\n'));
}

export async function copyValue(value: string): Promise<void> {
  await navigator.clipboard.writeText(value);
}

export async function copyAsWhere(colName: string, value: string): Promise<void> {
  await navigator.clipboard.writeText(`${colName} = '${value}'`);
}

export async function copyRowAsJson(row: RowData): Promise<void> {
  const state = get(tableStore);
  const obj: Record<string, string | number | boolean | null> = {};
  state.columns.forEach((c, i) => {
    obj[c.name] = row.values[i] ?? null;
  });
  await navigator.clipboard.writeText(JSON.stringify(obj, null, 2));
}