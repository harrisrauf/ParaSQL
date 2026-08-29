<script lang="ts">
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { openFile, openFolder, saveFile, saveFileAs, getAllRows, getColumns, exportJson, exportCsv, exportExcel, undo, redo } from '../commands';
  import { tableStore } from '../stores/table';
  import { settings } from '../stores/settings';
  import SearchBar from './SearchBar.svelte';

  async function handleOpen() {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Parquet', extensions: ['parquet'] }],
    });
    if (!selected) return;

    try {
      const meta = await openFile(selected);
      const rows = await getAllRows();
      const columns = meta.columns;

      tableStore.set({
        columns,
        rows,
        selectedRowIds: new Set(),
        sort: { column: '', direction: null },
        search: { query: '', column: null },
        modified: false,
        filePath: meta.file_path,
        totalRows: meta.total_rows,
        pageSize: 500,
        currentPage: 0,
        sqlResult: null,
        savedTable: null,
        activeTab: 'schema',
      });
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }

  async function handleOpenFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
    });
    if (!selected) return;

    try {
      const meta = await openFolder(selected);
      const rows = await getAllRows();
      const columns = meta.columns;

      tableStore.set({
        columns,
        rows,
        selectedRowIds: new Set(),
        sort: { column: '', direction: null },
        search: { query: '', column: null },
        modified: false,
        filePath: meta.file_path,
        totalRows: meta.total_rows,
        pageSize: 500,
        currentPage: 0,
        sqlResult: null,
        savedTable: null,
        activeTab: 'schema',
      });
    } catch (err) {
      console.error('Failed to open folder:', err);
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

  async function handleExportJson() {
    const selected = await save({
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!selected) return;
    try {
      await exportJson(selected);
    } catch (err) {
      console.error('Failed to export JSON:', err);
    }
  }

  async function handleExportCsv() {
    const selected = await save({
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    });
    if (!selected) return;
    try {
      await exportCsv(selected);
    } catch (err) {
      console.error('Failed to export CSV:', err);
    }
  }

  async function handleExportExcel() {
    const selected = await save({
      filters: [{ name: 'Excel', extensions: ['xlsx'] }],
    });
    if (!selected) return;
    try {
      await exportExcel(selected);
    } catch (err) {
      console.error('Failed to export Excel:', err);
    }
  }

  async function handleInsertRow() {
    try {
      const { insertRow } = await import('../commands');
      await insertRow();
      const rows = await getAllRows();
      tableStore.setRows(rows);
      tableStore.update(s => ({ ...s, modified: true }));
    } catch (err) {
      console.error('Failed to insert row:', err);
    }
  }

  async function handleUndo() {
    try {
      await undo();
      const [rows, columns] = await Promise.all([getAllRows(), getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns, sqlResult: null, savedTable: null }));
    } catch (err) {
      console.error('Failed to undo:', err);
    }
  }

  async function handleRedo() {
    try {
      await redo();
      const [rows, columns] = await Promise.all([getAllRows(), getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns, sqlResult: null, savedTable: null }));
    } catch (err) {
      console.error('Failed to redo:', err);
    }
  }

  let modified = $derived($tableStore.modified);
  let darkMode = $derived($settings.darkMode);
</script>

<div class="toolbar">
  <div class="toolbar-group">
    <button onclick={handleOpen} class="toolbar-btn primary">Open</button>
    <button onclick={handleOpenFolder} class="toolbar-btn" title="Open folder with multiple Parquet files">Open Folder</button>
    <button onclick={handleSave} class="toolbar-btn" disabled={!modified}>Save</button>
    <button onclick={handleSaveAs} class="toolbar-btn">Save As</button>
  </div>

  <div class="toolbar-divider"></div>

  <div class="toolbar-group">
    <button onclick={handleUndo} class="toolbar-btn" title="Undo (Ctrl+Z)">↩ Undo</button>
    <button onclick={handleRedo} class="toolbar-btn" title="Redo (Ctrl+Y)">↪ Redo</button>
  </div>

  <div class="toolbar-divider"></div>

  <div class="toolbar-group">
    <button onclick={handleInsertRow} class="toolbar-btn" title="Insert new row">+ Row</button>
  </div>

  <div class="toolbar-divider"></div>

  <div class="toolbar-group">
    <button onclick={handleExportJson} class="toolbar-btn" title="Export to JSON">JSON</button>
    <button onclick={handleExportCsv} class="toolbar-btn" title="Export to CSV">CSV</button>
    <button onclick={handleExportExcel} class="toolbar-btn" title="Export to Excel">Excel</button>
  </div>

  <div class="toolbar-spacer"></div>

  <div class="toolbar-group">
    <button
      class="toolbar-btn theme-toggle"
      onclick={() => settings.toggleDarkMode()}
      title={darkMode ? 'Switch to light mode' : 'Switch to dark mode'}
    >
      {darkMode ? '☀️' : '🌙'}
    </button>
  </div>

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
    border-bottom: 1px solid var(--border-color, #e0e0e0);
    background: var(--toolbar-bg, #fafafa);
  }

  .toolbar-group {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .toolbar-divider {
    width: 1px;
    height: 24px;
    background: var(--border-color, #e0e0e0);
  }

  .toolbar-spacer {
    flex: 1;
  }

  .toolbar-btn {
    padding: 5px 12px;
    border: 1px solid var(--border-color, #ddd);
    border-radius: 4px;
    background: var(--panel-bg, white);
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary, #333);
    white-space: nowrap;
  }

  .toolbar-btn:hover:not(:disabled) {
    background: var(--hover-bg, #f0f0f0);
  }

  .toolbar-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .toolbar-btn.primary {
    background: var(--accent-color, #1a73e8);
    color: white;
    border-color: var(--accent-color, #1a73e8);
  }

  .toolbar-btn.primary:hover {
    background: var(--accent-hover, #1557b0);
  }

  .toolbar-btn.theme-toggle {
    padding: 5px 8px;
    font-size: 14px;
  }
</style>
