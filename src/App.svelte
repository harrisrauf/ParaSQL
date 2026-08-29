<script lang="ts">
  import { onMount } from 'svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import DataTable from './lib/components/DataTable.svelte';
  import RightSidebar from './lib/components/RightSidebar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import { tableStore } from './lib/stores/table';
  import { deleteRows, undo, redo } from './lib/commands';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  let hasData = $derived($tableStore.columns.length > 0);
  let selectedIds = $derived($tableStore.selectedRowIds);

  // Context menu state
  let contextMenu = $state<{ show: boolean; x: number; y: number; value: string; colName: string }>({
    show: false, x: 0, y: 0, value: '', colName: ''
  });

  function handleKeydown(e: KeyboardEvent) {
    // Don't hijack shortcuts while typing in inputs, editors, or selects
    const target = e.target as HTMLElement | null;
    const tag = target?.tagName?.toLowerCase();
    if (
      tag === 'input' || tag === 'textarea' || tag === 'select' ||
      target?.isContentEditable || !!target?.closest('.cm-editor')
    ) {
      return;
    }
    // Undo: Ctrl+Z
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
      e.preventDefault();
      handleUndo();
      return;
    }
    // Redo: Ctrl+Y or Ctrl+Shift+Z
    if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) {
      e.preventDefault();
      handleRedo();
      return;
    }
    // Delete selected rows
    if (e.key === 'Delete' && selectedIds.size > 0) {
      handleDeleteSelected();
    }
  }

  async function handleDeleteSelected() {
    if (selectedIds.size === 0) return;
    if (!confirm(`Delete ${selectedIds.size} row(s)?`)) return;
    try {
      await deleteRows([...selectedIds]);
      tableStore.removeRows([...selectedIds]);
      tableStore.update(s => ({ ...s, modified: true }));
      tableStore.clearSelection();
    } catch (err) {
      console.error('Failed to delete rows:', err);
    }
  }

  async function handleUndo() {
    try {
      await undo();
      const mod = await import('./lib/commands');
      const [rows, columns] = await Promise.all([mod.getAllRows(), mod.getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns, sqlResult: null, savedTable: null }));
    } catch (err) {
      console.error('Failed to undo:', err);
    }
  }

  async function handleRedo() {
    try {
      await redo();
      const mod = await import('./lib/commands');
      const [rows, columns] = await Promise.all([mod.getAllRows(), mod.getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns, sqlResult: null, savedTable: null }));
    } catch (err) {
      console.error('Failed to redo:', err);
    }
  }

  function handleContextMenu(e: MouseEvent) {
    const target = e.target as HTMLElement;
    const cellValue = target.closest('.cell-value')?.textContent || '';
    const colName = target.closest('.cell')?.getAttribute('data-col') || '';
    contextMenu = { show: true, x: e.clientX, y: e.clientY, value: cellValue, colName };
  }

  function closeContextMenu() {
    contextMenu.show = false;
  }

  async function copyAsWhere() {
    if (contextMenu.colName && contextMenu.value) {
      const whereClause = `${contextMenu.colName} = '${contextMenu.value}'`;
      try {
        await navigator.clipboard.writeText(whereClause);
      } catch (e) {
        console.error('Failed to copy:', e);
      }
    }
    closeContextMenu();
  }

  async function copyValue() {
    if (contextMenu.value) {
      try {
        await navigator.clipboard.writeText(contextMenu.value);
      } catch (e) {
        console.error('Failed to copy:', e);
      }
    }
    closeContextMenu();
  }

  onMount(() => {
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      let modified = false;
      const unsub = tableStore.subscribe(v => modified = v.modified);
      unsub();
      if (modified) {
        const confirmed = await confirm('You have unsaved changes. Close anyway?');
        if (!confirmed) {
          event.preventDefault();
        }
      }
    });

    // Close context menu on click outside
    document.addEventListener('click', closeContextMenu);

    return () => {
      unlisten.then(fn => fn());
      document.removeEventListener('click', closeContextMenu);
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="app" oncontextmenu={handleContextMenu}>
  <Toolbar />
  <div class="main-content">
    {#if hasData}
      <DataTable />
      <RightSidebar />
    {:else}
      <div class="welcome">
        <h1>Parquet Viewer</h1>
        <p>Open a .parquet file to get started.</p>
        <p class="subtitle">Powered by DuckDB — Full SQL support for Parquet files</p>
        <div class="welcome-actions">
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
                totalRows: meta.total_rows,
                pageSize: 500,
                currentPage: 0,
                sqlResult: null,
                savedTable: null,
                activeTab: 'schema',
              });
            }}
            class="open-btn"
          >
            Open File
          </button>
          <button
            onclick={async () => {
              const { open } = await import('@tauri-apps/plugin-dialog');
              const selected = await open({ directory: true, multiple: false });
              if (!selected) return;
              const { openFolder, getAllRows } = await import('./lib/commands');
              const meta = await openFolder(selected);
              const rows = await getAllRows();
              tableStore.set({
                columns: meta.columns,
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
            }}
            class="open-btn secondary"
          >
            Open Folder
          </button>
        </div>
      </div>
    {/if}
  </div>
  <StatusBar />

  {#if contextMenu.show}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="context-menu"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
      onclick={(e) => e.stopPropagation()}
    >
      <button onclick={copyValue}>Copy value</button>
      <button onclick={copyAsWhere}>Copy as WHERE clause</button>
    </div>
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    color: var(--text-primary, #333);
    background: var(--bg, white);
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
    gap: 8px;
    color: var(--text-secondary, #888);
  }

  .welcome h1 {
    font-size: 24px;
    font-weight: 600;
    color: var(--text-primary, #333);
    margin: 0;
  }

  .welcome p {
    margin: 0;
    font-size: 14px;
  }

  .welcome .subtitle {
    font-size: 12px;
    color: var(--text-secondary, #aaa);
    margin-top: 4px;
  }

  .welcome-actions {
    display: flex;
    gap: 12px;
    margin-top: 16px;
  }

  .open-btn {
    padding: 10px 28px;
    border: 1px solid var(--accent-color, #1a73e8);
    border-radius: 6px;
    background: var(--accent-color, #1a73e8);
    color: white;
    cursor: pointer;
    font-size: 14px;
    font-family: inherit;
  }

  .open-btn:hover {
    background: var(--accent-hover, #1557b0);
  }

  .open-btn.secondary {
    background: var(--panel-bg, white);
    color: var(--text-primary, #333);
    border-color: var(--border-color, #ddd);
  }

  .open-btn.secondary:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .context-menu {
    position: fixed;
    background: var(--panel-bg, white);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    padding: 4px 0;
    z-index: 1000;
    min-width: 180px;
  }

  .context-menu button {
    display: block;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: none;
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
    color: var(--text-primary, #333);
    text-align: left;
  }

  .context-menu button:hover {
    background: var(--hover-bg, #f0f0f0);
  }
</style>
