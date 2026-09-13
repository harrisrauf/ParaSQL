<script lang="ts">
  import { onMount } from 'svelte';
  import MenuBar from './lib/components/MenuBar.svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import DataGrid from './lib/components/DataGrid.svelte';
  import LeftPanel from './lib/components/LeftPanel.svelte';
  import WorkspacePanel from './lib/components/WorkspacePanel.svelte';
  import WorkspaceMain from './lib/components/WorkspaceMain.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import Toast from './lib/components/Toast.svelte';
  import { tableStore, selectedRowIds } from './lib/stores/table';
  import { workspaceStore } from './lib/stores/workspace';
  import { uiStore } from './lib/stores/ui';
  import { settings } from './lib/stores/settings';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { confirmDialog } from './lib/dialogs';
  import {
    openFileFlow,
    openFolderFlow,
    saveFlow,
    undoFlow,
    redoFlow,
    deleteSelectedFlow,
    copySelectionToClipboard,
    copySelectionAsJson,
    copyValue,
    copyAsWhere,
    copyRowAsJson,
    newWorkspaceFlow,
    openWorkspaceFlow,
    saveWorkspaceFlow,
  } from './lib/actions';
  import type { RowData } from './lib/types';

  let hasData = $derived($tableStore.columns.length > 0);
  let selectedIds = $derived($selectedRowIds);
  let sidebarVisible = $derived($uiStore.sidebarVisible);
  let wsOpen = $derived($workspaceStore.doc != null);
  let editable = $derived($tableStore.editable);

  // Context menu state
  let contextMenu = $state<{
    show: boolean; x: number; y: number; value: string; colName: string; row: RowData | null;
    kind: 'cell' | 'row' | 'column';
  }>({ show: false, x: 0, y: 0, value: '', colName: '', row: null, kind: 'cell' });

  function handleKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    const tag = target?.tagName?.toLowerCase();
    const mod = e.ctrlKey || e.metaKey;

    // Save works everywhere — including while typing in the SQL editor or a cell
    if (mod && (e.key === 's' || e.key === 'S')) {
      e.preventDefault();
      if ($workspaceStore.doc) saveWorkspaceFlow();
      else if ($tableStore.filePath) saveFlow();
      return;
    }

    // Don't hijack other shortcuts while typing in inputs, editors, or selects
    if (
      tag === 'input' || tag === 'textarea' || tag === 'select' ||
      target?.isContentEditable || !!target?.closest('.cm-editor')
    ) {
      return;
    }
    if (e.key === 'Escape') {
      closeContextMenu();
      return;
    }
    // Undo: Ctrl+Z
    if (mod && e.key === 'z' && !e.shiftKey) {
      e.preventDefault();
      if (editable) undoFlow();
      return;
    }
    // Redo: Ctrl+Y or Ctrl+Shift+Z
    if (mod && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) {
      e.preventDefault();
      if (editable) redoFlow();
      return;
    }
    // Copy: Ctrl+C (with grid focus)
    if (mod && e.key === 'c') {
      if (selectedIds.size > 0 || $tableStore.selection.allRows) {
        copySelectionToClipboard();
      }
      return;
    }
    // Select all: Ctrl+A
    if (mod && e.key === 'a') {
      if ($tableStore.columns.length > 0) {
        e.preventDefault();
        tableStore.selectAll();
      }
      return;
    }
    // Delete selected rows (grid focus only — not while a button is focused)
    if (e.key === 'Delete' && tag !== 'button' && selectedIds.size > 0 && editable) {
      deleteSelectedFlow();
    }
  }

  function handleContextMenu(e: MouseEvent) {
    const target = e.target as HTMLElement;
    // Editable elements keep their native menu (paste, cut, spellcheck)
    if (target.closest('input, textarea, [contenteditable="true"], .cm-editor')) return;
    // Suppress the native webview context menu entirely
    e.preventDefault();
    const cellValue = target.closest('.cell-value')?.textContent || '';
    const colEl = target.closest('.col-header') as HTMLElement | null;
    const colName = target.closest('.cell')?.getAttribute('data-col') || colEl?.getAttribute('data-col') || '';
    const rowEl = target.closest('.grid-row') as HTMLElement | null;
    const rowId = rowEl ? Number(rowEl.dataset.row) : NaN;
    const row = Number.isNaN(rowId) ? null : $tableStore.rows.find(r => r.row_id === rowId) ?? null;
    // Excel-style: right-clicking a row that isn't selected selects it first
    if (row && !$selectedRowIds.has(row.row_id)) {
      tableStore.selectRow(row.row_id);
    }
    // Prefer the raw cell value over the rendered text (null renders as "—").
    const colIdx = colName ? $tableStore.columns.findIndex(c => c.name === colName) : -1;
    let value = cellValue;
    if (row && colIdx >= 0) {
      const raw = row.values[colIdx];
      value = raw === null || raw === undefined
        ? ''
        : typeof raw === 'object' ? JSON.stringify(raw) : String(raw);
    }
    const kind: 'cell' | 'row' | 'column' =
      target.closest('.col-header') ? 'column' :
      rowEl ? 'row' : 'cell';
    if (!row && !cellValue && kind !== 'column') {
      closeContextMenu();
      return;
    }
    const menuW = 260;
    const menuH = 240;
    contextMenu = {
      show: true,
      x: Math.min(e.clientX, window.innerWidth - menuW),
      y: Math.min(e.clientY, window.innerHeight - menuH),
      value,
      colName,
      row,
      kind,
    };
  }

  function closeContextMenu() {
    contextMenu.show = false;
  }

  async function handleCopyValue() {
    if (contextMenu.value) await copyValue(contextMenu.value);
    closeContextMenu();
  }

  async function handleCopyAsWhere() {
    if (contextMenu.colName && contextMenu.value) {
      await copyAsWhere(contextMenu.colName, contextMenu.value);
    }
    closeContextMenu();
  }

  async function handleCopyRowJson() {
    if (contextMenu.row) await copyRowAsJson(contextMenu.row);
    closeContextMenu();
  }

  async function handleCopySelectionTsv() {
    await copySelectionToClipboard();
    closeContextMenu();
  }

  async function handleCopySelectionJson() {
    await copySelectionAsJson();
    closeContextMenu();
  }

  function handleSortColumn(direction: 'asc' | 'desc') {
    if (contextMenu.colName) {
      tableStore.update(s => ({ ...s, sort: { column: contextMenu.colName, direction } }));
    }
    closeContextMenu();
  }

  function handleClearSortMenu() {
    tableStore.clearSort();
    closeContextMenu();
  }

  async function handleDeleteRow() {
    if (contextMenu.row) {
      tableStore.selectRow(contextMenu.row.row_id);
      await deleteSelectedFlow();
    }
    closeContextMenu();
  }

  onMount(() => {
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      let modified = false;
      const unsub = tableStore.subscribe(v => modified = v.modified);
      unsub();
      if (modified) {
        const confirmed = await confirmDialog('You have unsaved changes. Close anyway?');
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
  <MenuBar />
  <Toolbar />
  <div class="main-content">
    {#if wsOpen}
      {#if sidebarVisible}
        <WorkspacePanel />
      {/if}
      <WorkspaceMain />
    {:else if hasData}
      {#if sidebarVisible}
        <LeftPanel />
      {/if}
      <DataGrid />
    {:else}
      <div class="welcome">
        <h1>ParaSQL</h1>
        <p>Open a .parquet file, or create a workspace to query many files as tables.</p>
        <div class="welcome-actions">
          <button onclick={() => openFileFlow()} class="open-btn">Open File</button>
          <button onclick={() => openFolderFlow()} class="open-btn secondary">Open Folder</button>
          <button onclick={() => newWorkspaceFlow()} class="open-btn secondary">New Workspace</button>
          <button onclick={() => openWorkspaceFlow()} class="open-btn secondary">Open Workspace</button>
        </div>
        {#if $settings.recentFiles.length > 0}
          <div class="welcome-recent">
            <span class="recent-label">Recent files:</span>
            {#each $settings.recentFiles.slice(0, 5) as path (path)}
              <button class="recent-btn" onclick={() => openFileFlow(path)}>
                {path.split(/[\\/]/).pop()}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
  <StatusBar />

  {#if contextMenu.show}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div
      class="context-menu"
      role="menu"
      aria-label="Context menu"
      tabindex="-1"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
      onclick={(e) => e.stopPropagation()}
    >
      {#if contextMenu.kind === 'column'}
        <button onclick={() => handleSortColumn('asc')}>Sort Ascending</button>
        <button onclick={() => handleSortColumn('desc')}>Sort Descending</button>
        <button onclick={handleClearSortMenu}>Clear Sort</button>
        <div class="menu-sep"></div>
      {/if}
      {#if contextMenu.kind === 'row' && editable}
        <button onclick={handleDeleteRow}>Delete row</button>
        <div class="menu-sep"></div>
      {/if}
      {#if contextMenu.kind === 'cell' || contextMenu.value}
        <button onclick={handleCopyValue}>Copy</button>
      {/if}
      {#if contextMenu.colName && contextMenu.value}
        <button onclick={handleCopyAsWhere}>Copy as WHERE clause</button>
      {/if}
      {#if contextMenu.row}
        <button onclick={handleCopyRowJson}>Copy row as JSON</button>
      {/if}
      {#if selectedIds.size > 0}
        <div class="menu-sep"></div>
        <button onclick={handleCopySelectionTsv}>
          Copy {selectedIds.size} selected {selectedIds.size === 1 ? 'row' : 'rows'} (TSV)
        </button>
        <button onclick={handleCopySelectionJson}>
          Copy {selectedIds.size} selected {selectedIds.size === 1 ? 'row' : 'rows'} (JSON)
        </button>
      {/if}
    </div>
  {/if}
</div>

<Toast />

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
    min-height: 0;
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
    color: var(--accent-text, #fff);
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

  .welcome-recent {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    margin-top: 20px;
  }

  .recent-label {
    font-size: 11px;
    color: var(--text-secondary, #aaa);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .recent-btn {
    border: none;
    background: none;
    color: var(--accent-color, #1a73e8);
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
    padding: 2px 8px;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .recent-btn:hover {
    text-decoration: underline;
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

  .menu-sep {
    height: 1px;
    margin: 4px 0;
    background: var(--border-color, #e0e0e0);
  }
</style>