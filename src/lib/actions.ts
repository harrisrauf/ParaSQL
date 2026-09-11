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

/** Confirm discarding unsaved edits before replacing the loaded engine. */
export function confirmDiscardChanges(): boolean {
  if (!get(tableStore).modified) return true;
  return confirm('You have unsaved changes. Discard them?');
}

// --- File flows ---

export async function openFileFlow(path?: string): Promise<boolean> {
  if (!confirmDiscardChanges()) return false;
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
  if (!confirmDiscardChanges()) return false;
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
  if (!get(tableStore).editable) return;
  try {
    await cmds.undo();
    const [rows, columns, info] = await Promise.all([
      cmds.getAllRows(),
      cmds.getColumns(),
      cmds.getFileInfo(),
    ]);
    tableStore.update(s => ({
      ...s,
      rows,
      columns,
      sqlResult: null,
      savedTable: null,
      selection: { anchor: null, cells: new Set(), allRows: false },
      filters: {},
      editable: true,
      modified: info.modified,
    }));
  } catch (err) {
    console.error('Failed to undo:', err);
  }
}

export async function redoFlow(): Promise<void> {
  if (!get(tableStore).editable) return;
  try {
    await cmds.redo();
    const [rows, columns, info] = await Promise.all([
      cmds.getAllRows(),
      cmds.getColumns(),
      cmds.getFileInfo(),
    ]);
    tableStore.update(s => ({
      ...s,
      rows,
      columns,
      sqlResult: null,
      savedTable: null,
      selection: { anchor: null, cells: new Set(), allRows: false },
      filters: {},
      editable: true,
      modified: info.modified,
    }));
  } catch (err) {
    console.error('Failed to redo:', err);
  }
}

export async function deleteSelectedFlow(): Promise<boolean> {
  if (!get(tableStore).editable) return false;
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
  if (!get(tableStore).editable) return;
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
      tableStore.focusRow(row.row_id);
    }
  } catch (err) {
    console.error('Failed to insert row:', err);
  }
}

/** Whole-table search via the engine (matches unloaded rows too) */
export async function searchFlow(query: string, column: string | null = null): Promise<void> {
  const q = query.trim();
  if (!q) {
    tableStore.setSearch('', null);
    return;
  }
  try {
    const result = await cmds.searchRows(q, column);
    tableStore.setSearch(q, column);
    tableStore.update(s => ({
      ...s,
      searchRows: result.rows,
      searchTruncated: result.truncated,
    }));
  } catch (err) {
    console.error('Search failed:', err);
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

/// JSON array of objects for the current selection (row-major, column-named),
/// mirroring copySelectionToClipboard's selection semantics.
export async function copySelectionAsJson(): Promise<void> {
  const state = get(tableStore);
  const cols = state.columns;
  if (cols.length === 0) return;

  let rows: RowData[];
  if (state.selection.allRows) {
    rows = get(displayedRows);
  } else {
    const cells = state.selection.cells;
    if (cells.size === 0) return;
    const rowIds = new Set<number>();
    for (const key of cells) {
      rowIds.add(Number(key.slice(0, key.indexOf(':'))));
    }
    const rowIndex = new Map(get(displayedRows).map((r, i) => [r.row_id, i]));
    rows = [...rowIds]
      .sort((a, b) => (rowIndex.get(a) ?? 0) - (rowIndex.get(b) ?? 0))
      .map(rid => state.rows.find(r => r.row_id === rid))
      .filter((r): r is RowData => !!r);
  }

  const objects = rows.map(r => {
    const obj: Record<string, string | number | boolean | null> = {};
    cols.forEach((c, i) => {
      obj[c.name] = r.values[i] ?? null;
    });
    return obj;
  });
  await navigator.clipboard.writeText(JSON.stringify(objects, null, 2));
}

export async function copyAsWhere(colName: string, value: string): Promise<void> {
  const escaped = value.replace(/'/g, "''");
  await navigator.clipboard.writeText(`${colName} = '${escaped}'`);
}

export async function copyRowAsJson(row: RowData): Promise<void> {
  const state = get(tableStore);
  const obj: Record<string, string | number | boolean | null> = {};
  state.columns.forEach((c, i) => {
    obj[c.name] = row.values[i] ?? null;
  });
  await navigator.clipboard.writeText(JSON.stringify(obj, null, 2));
}

// --- Workspace flows ---

import { workspaceStore } from './stores/workspace';
import type { Workspace, WorkspaceTable, TableMeta, TableSummary } from './types';

const PARASQL_FILTER = [{ name: 'ParaSQL Workspace', extensions: ['parasql'] }];

function emptyWorkspace(name: string): Workspace {
  return {
    version: 1,
    name,
    tables: [],
    saved_queries: [],
    charts: [],
    dashboards: [],
    notebooks: [],
    edit_size_limit_mb: 2048,
    dir: null,
  };
}

function baseNameOf(p: string): string {
  const seg = p.replace(/\\/g, '/').split('/').pop() ?? 'table';
  return seg.replace(/\.parquet$/i, '');
}

function uniqueTableName(doc: Workspace, base: string): string {
  const names = new Set(doc.tables.map(t => t.name));
  if (!names.has(base)) return base;
  let i = 2;
  while (names.has(`${base}_${i}`)) i++;
  return `${base}_${i}`;
}

async function persist(doc: Workspace, path: string): Promise<Workspace> {
  return cmds.syncWorkspaceTables(path, doc);
}

export async function newWorkspaceFlow(): Promise<boolean> {
  if (!confirmDiscardChanges()) return false;
  const selected = await save({ filters: PARASQL_FILTER });
  if (!selected) return false;
  const doc = emptyWorkspace(baseNameOf(selected));
  await cmds.saveWorkspace(selected, doc);
  const loaded = await cmds.openWorkspace(selected);
  workspaceStore.set({
    path: selected, doc: loaded, activeView: 'query', editorOpen: false,
    metas: {}, loading: false, error: null,
  });
  return true;
}

export async function openWorkspaceFlow(path?: string): Promise<boolean> {
  if (!confirmDiscardChanges()) return false;
  let selected = path;
  if (!selected) {
    const res = await open({ multiple: false, filters: PARASQL_FILTER });
    if (!res) return false;
    selected = res;
  }
  try {
    const doc = await cmds.openWorkspace(selected);
    workspaceStore.set({
      path: selected, doc, activeView: 'query', editorOpen: false,
      metas: {}, loading: false, error: null,
    });
    tableStore.open([], [], 0, null);
    return true;
  } catch (e) {
    console.error('Failed to open workspace:', e);
    return false;
  }
}

export async function saveWorkspaceFlow(): Promise<boolean> {
  const s = get(workspaceStore);
  if (!s.doc) return false;
  if (!s.path) return saveWorkspaceAsFlow();
  try {
    await cmds.saveWorkspace(s.path, s.doc);
    return true;
  } catch (e) {
    console.error('Failed to save workspace:', e);
    return false;
  }
}

export async function saveWorkspaceAsFlow(): Promise<boolean> {
  const s = get(workspaceStore);
  if (!s.doc) return false;
  const selected = await save({ defaultPath: `${s.doc.name}.parasql`, filters: PARASQL_FILTER });
  if (!selected) return false;
  try {
    await cmds.saveWorkspace(selected, s.doc);
    workspaceStore.update(v => ({ ...v, path: selected }));
    return true;
  } catch (e) {
    console.error('Failed to save workspace:', e);
    return false;
  }
}

export async function addTableFlow(): Promise<boolean> {
  const s = get(workspaceStore);
  if (!s.doc || !s.path) return false;
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Parquet', extensions: ['parquet'] }],
  });
  if (!selected) return false;
  try {
    const meta = await cmds.getTableMeta(selected);
    const table: WorkspaceTable = {
      name: uniqueTableName(s.doc, baseNameOf(selected)),
      path: selected,
      source: 'file',
      mode: 'query',
      compression: meta.compression,
      size: meta.size_bytes,
      mtime: null,
      abs_path: selected,
      missing: false,
    };
    const doc = { ...s.doc, tables: [...s.doc.tables, table] };
    const synced = await persist(doc, s.path);
    workspaceStore.update(v => ({
      ...v, doc: synced, metas: { ...v.metas, [selected]: meta },
    }));
    return true;
  } catch (e) {
    console.error('Failed to add table:', e);
    return false;
  }
}

export async function addFolderFlow(): Promise<boolean> {
  const s = get(workspaceStore);
  if (!s.doc || !s.path) return false;
  const dir = await open({ directory: true, multiple: false });
  if (!dir) return false;
  try {
    const files = await cmds.listParquetFiles(dir);
    if (files.length === 0) return false;
    const tables: WorkspaceTable[] = [...s.doc.tables];
    const metas: Record<string, TableMeta> = { ...s.metas };
    for (const f of files) {
      const meta = await cmds.getTableMeta(f);
      tables.push({
        name: uniqueTableName(s.doc, baseNameOf(f)),
        path: f,
        source: 'file',
        mode: 'query',
        compression: meta.compression,
        size: meta.size_bytes,
        mtime: null,
        abs_path: f,
        missing: false,
      });
      metas[f] = meta;
    }
    const doc = { ...s.doc, tables };
    const synced = await persist(doc, s.path);
    workspaceStore.update(v => ({ ...v, doc: synced, metas }));
    return true;
  } catch (e) {
    console.error('Failed to add folder:', e);
    return false;
  }
}

export async function runTableQueryFlow(name: string): Promise<void> {
  const escaped = name.replace(/"/g, '""');
  const result = await cmds.executeSql(`SELECT * FROM "${escaped}" LIMIT 500`);
  tableStore.applyQueryResult(result);
  workspaceStore.update(v => ({ ...v, activeView: 'query' }));
}

export async function removeTableFlow(name: string): Promise<boolean> {  const s = get(workspaceStore);
  if (!s.doc || !s.path) return false;
  const doc = { ...s.doc, tables: s.doc.tables.filter(t => t.name !== name) };
  try {
    const synced = await persist(doc, s.path);
    workspaceStore.update(v => ({ ...v, doc: synced }));
    return true;
  } catch (e) {
    console.error('Failed to remove table:', e);
    return false;
  }
}

export async function renameTableFlow(oldName: string, newName: string): Promise<boolean> {
  const s = get(workspaceStore);
  if (!s.doc || !s.path) return false;
  const trimmed = newName.trim();
  if (!trimmed || trimmed === oldName) return false;
  if (s.doc.tables.some(t => t.name === trimmed)) return false;
  const doc = {
    ...s.doc,
    tables: s.doc.tables.map(t => (t.name === oldName ? { ...t, name: trimmed } : t)),
  };
  try {
    const synced = await persist(doc, s.path);
    workspaceStore.update(v => ({ ...v, doc: synced }));
    return true;
  } catch (e) {
    console.error('Failed to rename table:', e);
    return false;
  }
}

export async function setTableModeFlow(name: string, mode: 'query' | 'editable'): Promise<boolean> {
  const s = get(workspaceStore);
  if (!s.doc || !s.path) return false;
  const doc = {
    ...s.doc,
    tables: s.doc.tables.map(t => (t.name === name ? { ...t, mode } : t)),
  };
  try {
    const synced = await persist(doc, s.path);
    workspaceStore.update(v => ({ ...v, doc: synced }));
    return true;
  } catch (e) {
    console.error('Failed to set table mode:', e);
    return false;
  }
}

export async function exportTableFlow(name: string, compression = 'zstd'): Promise<boolean> {
  const s = get(workspaceStore);
  const table = s.doc?.tables.find(t => t.name === name);
  if (!table) return false;
  const selected = await save({
    defaultPath: `${name}.parquet`,
    filters: [{ name: 'Parquet', extensions: ['parquet'] }],
  });
  if (!selected) return false;
  try {
    await cmds.exportTable(name, selected, compression);
    return true;
  } catch (e) {
    console.error('Failed to export table:', e);
    return false;
  }
}

export async function openEditorFlow(name: string): Promise<boolean> {
  const s = get(workspaceStore);
  const doc = s.doc;
  const table = doc?.tables.find(t => t.name === name);
  if (!table || !s.path || !doc) return false;
  const path = table.abs_path ?? table.path;
  try {
    const res = await cmds.openTableForEdit(name, path, false, doc);
    if (!res.opened && res.over_limit) {
      const ok = confirm(
        `"${name}" is ${res.size_mb} MB. Editing loads the full table into memory ` +
        'and saving rewrites the whole file. Open anyway?'
      );
      if (!ok) return false;
      const res2 = await cmds.openTableForEdit(name, path, true, doc);
      if (!res2.opened) return false;
    }
    const [columns, rows, fileInfo] = await Promise.all([
      cmds.getColumns(),
      cmds.getAllRows(),
      cmds.getFileInfo(),
    ]);
    tableStore.open(columns, rows, fileInfo.rows ?? rows.length, path);
    workspaceStore.update(v => ({ ...v, editorOpen: true, activeView: 'data' }));
    return true;
  } catch (e) {
    console.error('Failed to open table for editing:', e);
    return false;
  }
}

export async function closeEditorFlow(): Promise<void> {
  try {
    await cmds.closeEditor();
  } catch {
    // no editor open is fine
  }
  workspaceStore.update(v => ({ ...v, editorOpen: false }));
}

export async function showEditorDataFlow(): Promise<void> {
  const s = get(workspaceStore);
  workspaceStore.update(v => ({ ...v, activeView: 'data' }));
  if (s.editorOpen && s.doc && s.path && get(tableStore).sqlResult) {
    const columns = await cmds.getColumns();
    const rows = await cmds.getAllRows();
    tableStore.open(columns, rows, rows.length, s.path);
  }
}

export async function summarizeTableFlow(name: string): Promise<TableSummary | null> {
  try {
    return await cmds.summarizeTable(name);
  } catch (e) {
    console.error('Failed to summarize table:', e);
    return null;
  }
}