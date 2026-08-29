<script lang="ts">
  import { onMount } from 'svelte';
  import MenuBar from './lib/components/MenuBar.svelte';
  import Toolbar from './lib/components/Toolbar.svelte';
  import DataGrid from './lib/components/DataGrid.svelte';
  import LeftPanel from './lib/components/LeftPanel.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import { tableStore, selectedRowIds } from './lib/stores/table';
  import { uiStore } from './lib/stores/ui';
  import { settings } from './lib/stores/settings';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import {
    openFileFlow,
    openFolderFlow,
    saveFlow,
    undoFlow,
    redoFlow,
    deleteSelectedFlow,
    copySelectionToClipboard,
    copyValue,
    copyAsWhere,
    copyRowAsJson,
  } from './lib/actions';
  import type { RowData } from './lib/types';

  let hasData = $derived($tableStore.columns.length > 0);
  let selectedIds = $derived(selectedRowIds);
  let sidebarVisible = $derived($uiStore.sidebarVisible);

  // Context menu state
  let contextMenu = $state<{
    show: boolean; x: number; y: number; value: string; colName: string; row: RowData | null;
  }>({ show: false, x: 0, y: 0, value: '', colName: '', row: null });

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
    const mod = e.ctrlKey || e.metaKey;
    // Undo: Ctrl+Z
    if (mod && e.key === 'z' && !e.shiftKey) {
      e.preventDefault();
      undoFlow();
      return;
    }
    // Redo: Ctrl+Y or Ctrl+Shift+Z
    if (mod && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) {
      e.preventDefault();
      redoFlow();
      return;
    }
    // Save: Ctrl+S
    if (mod && e.key === 's') {
      e.preventDefault();
      saveFlow();
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
    // Delete selected rows
    if (e.key === 'Delete' && selectedIds.size > 0) {
      deleteSelectedFlow();
    }
  }

  function handleContextMenu(e: MouseEvent) {
    const target = e.target as HTMLElement;
    const cellValue = target.closest('.cell-value')?.textContent || '';
    const colName = target.closest('.cell')?.getAttribute('data-col') || '';
    const rowEl = target.closest('.grid-row') as HTMLElement | null;
    const rowId = rowEl ? Number(rowEl.dataset.row) : NaN;
    const row = Number.isNaN(rowId) ? null : $tableStore.rows.find(r => r.row_id === rowId) ?? null;
    contextMenu = { show: true, x: e.clientX, y: e.clientY, value: cellValue, colName, row };
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
  <MenuBar />
  <Toolbar />
  <div class="main-content">
    {#if hasData}
      {#if sidebarVisible}
        <LeftPanel />
      {/if}
      <DataGrid />
    {:else}
      <div class="welcome">
        <h1>Parquet Viewer</h1>
        <p>Open a .parquet file to get started.</p>
        <p class="subtitle">Powered by DuckDB — Full SQL support for Parquet files</p>
        <div class="welcome-actions">
          <button onclick={() => openFileFlow()} class="open-btn">Open File</button>
          <button onclick={() => openFolderFlow()} class="open-btn secondary">Open Folder</button>
        </div>
        {#if settings.recentFiles.length > 0}
          <div class="welcome-recent">
            <span class="recent-label">Recent files:</span>
            {#each settings.recentFiles.slice(0, 5) as path (path)}
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
      <button onclick={handleCopyValue}>Copy value</button>
      <button onclick={handleCopyAsWhere}>Copy as WHERE clause</button>
      {#if contextMenu.row}
        <button onclick={handleCopyRowJson}>Copy row as JSON</button>
      {/if}
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
</style>