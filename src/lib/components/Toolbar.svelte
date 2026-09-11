<script lang="ts">
  import { onMount } from 'svelte';
  import SearchBar from './SearchBar.svelte';
  import { uiStore } from '../stores/ui';
  import { tableStore, selectedRowIds } from '../stores/table';
  import {
    openFileFlow,
    openFolderFlow,
    saveFlow,
    saveAsFlow,
    undoFlow,
    redoFlow,
    deleteSelectedFlow,
    insertRowFlow,
    exportFlow,
  } from '../actions';
  import { canUndo, canRedo } from '../commands';

  let undoAvail = $state(false);
  let redoAvail = $state(false);
  let dropdown: 'open' | 'export' | null = $state(null);

  let hasData = $derived($tableStore.columns.length > 0);
  let modified = $derived($tableStore.modified);
  let selectedCount = $derived($selectedRowIds.size);
  let sidebarVisible = $derived($uiStore.sidebarVisible);
  let editable = $derived($tableStore.editable);

  async function refreshUndoRedo() {
    const [u, r] = await Promise.all([canUndo(), canRedo()]);
    undoAvail = u;
    redoAvail = r;
  }

  onMount(() => {
    refreshUndoRedo();
    const close = () => {
      dropdown = null;
    };
    document.addEventListener('click', close);
    return () => document.removeEventListener('click', close);
  });

  // Keep undo/redo availability in sync with engine-side state changes.
  $effect(() => {
    void $tableStore.rows;
    void $tableStore.editable;
    refreshUndoRedo();
  });

  function toggleDropdown(name: 'open' | 'export') {
    dropdown = dropdown === name ? null : name;
  }

  async function runAction(fn: () => Promise<unknown>) {
    dropdown = null;
    await fn();
    refreshUndoRedo();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="toolbar" role="toolbar" aria-label="Toolbar" tabindex="-1" onclick={(e) => e.stopPropagation()}>
  <div class="toolbar-group">
    <button
      class="tb-btn"
      title="Toggle Sidebar"
      class:active={sidebarVisible}
      onclick={() => uiStore.toggleSidebar()}
    >
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <rect x="1.5" y="2.5" width="13" height="11" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.4" />
        <line x1="10.5" y1="2.5" x2="10.5" y2="13.5" stroke="currentColor" stroke-width="1.4" />
        <line x1="1.5" y1="6" x2="10.5" y2="6" stroke="currentColor" stroke-width="1.4" />
      </svg>
    </button>
  </div>

  <div class="toolbar-group">
    <button class="tb-btn" onclick={(e) => { e.stopPropagation(); toggleDropdown('open'); }}>
      Open <span class="caret">▾</span>
    </button>
    {#if dropdown === 'open'}
      <div class="tb-dropdown">
        <button class="menu-item" onclick={() => runAction(() => openFileFlow())}>Open File…</button>
        <button class="menu-item" onclick={() => runAction(() => openFolderFlow())}>Open Folder…</button>
      </div>
    {/if}
    <button class="tb-btn" disabled={!modified || !hasData} onclick={() => runAction(() => saveFlow())}>Save</button>
    <button class="tb-btn" disabled={!hasData} onclick={() => runAction(() => saveAsFlow())}>Save As</button>
  </div>

  <div class="toolbar-group">
    <button class="tb-btn icon-btn" disabled={!undoAvail || !editable} title="Undo (Ctrl+Z)" onclick={() => runAction(() => undoFlow())}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path d="M6.5 4 3 7.5l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
        <path d="M3 7.5h6.5a3.5 3.5 0 0 1 0 7H7" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      </svg>
    </button>
    <button class="tb-btn icon-btn" disabled={!redoAvail || !editable} title="Redo (Ctrl+Y)" onclick={() => runAction(() => redoFlow())}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path d="m9.5 4 3.5 3.5-3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
        <path d="M13 7.5H6.5a3.5 3.5 0 0 0 0 7H9" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      </svg>
    </button>
  </div>

  <div class="toolbar-group">
    <button class="tb-btn" disabled={!hasData || !editable} title="Insert Row" onclick={() => runAction(() => insertRowFlow())}>+ Row</button>
    <button class="tb-btn" disabled={selectedCount === 0 || !editable} title="Delete Selected Rows (Del)" onclick={() => runAction(() => deleteSelectedFlow())}>− Rows</button>
  </div>

  <div class="toolbar-group">
    <button class="tb-btn" disabled={!hasData} onclick={(e) => { e.stopPropagation(); toggleDropdown('export'); }}>
      Export <span class="caret">▾</span>
    </button>
    {#if dropdown === 'export'}
      <div class="tb-dropdown">
        <button class="menu-item" onclick={() => runAction(() => exportFlow('json'))}>JSON</button>
        <button class="menu-item" onclick={() => runAction(() => exportFlow('csv'))}>CSV</button>
        <button class="menu-item" onclick={() => runAction(() => exportFlow('excel'))}>Excel (XLSX)</button>
      </div>
    {/if}
  </div>

  <div class="toolbar-spacer"></div>

  <SearchBar />
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: var(--toolbar-bg, #fafafa);
    border-bottom: 1px solid var(--border-color, #e0e0e0);
    flex-shrink: 0;
    overflow: visible;
    position: relative;
    z-index: 50;
  }

  .toolbar-group {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 6px;
    margin-right: 6px;
    border-right: 1px solid var(--border-subtle, #e0e0e0);
    position: relative;
  }

  .tb-btn {
    border: 1px solid transparent;
    background: none;
    font-family: inherit;
    font-size: 12px;
    color: var(--text-primary, #333);
    padding: 4px 8px;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
  }

  .tb-btn:hover:not(:disabled) {
    background: var(--hover-bg, #e8e8e8);
  }

  .tb-btn.icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px 6px;
  }

  .tb-btn:disabled {
    color: var(--text-secondary, #aaa);
    cursor: default;
  }

  .tb-btn.active {
    background: var(--selected-bg, #e8f0fe);
    color: var(--accent-color, #1a73e8);
  }

  .caret {
    font-size: 9px;
    opacity: 0.7;
  }

  .tb-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    min-width: 150px;
    background: var(--bg, #fff);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
    padding: 4px 0;
    z-index: 1000;
  }

  .menu-item {
    display: block;
    width: 100%;
    border: none;
    background: none;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary, #333);
    padding: 6px 14px;
    cursor: pointer;
    text-align: left;
    white-space: nowrap;
  }

  .menu-item:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .toolbar-spacer {
    flex: 1;
  }
</style>