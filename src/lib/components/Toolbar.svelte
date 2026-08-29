<script lang="ts">
  import { onMount } from 'svelte';
  import SearchBar from './SearchBar.svelte';
  import { settings } from '../stores/settings';
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
  let darkMode = $derived($settings.darkMode);
  let sidebarVisible = $derived($uiStore.sidebarVisible);

  async function refreshUndoRedo() {
    const [u, r] = await Promise.all([canUndo(), canRedo()]);
    undoAvail = u;
    redoAvail = r;
  }

  onMount(() => {
    const close = () => {
      dropdown = null;
    };
    document.addEventListener('click', close);
    return () => document.removeEventListener('click', close);
  });

  function toggleDropdown(name: 'open' | 'export') {
    dropdown = dropdown === name ? null : name;
  }

  async function runAction(fn: () => Promise<void>) {
    dropdown = null;
    await fn();
    refreshUndoRedo();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="toolbar" role="toolbar" aria-label="Toolbar" tabindex="-1" onclick={(e) => e.stopPropagation()}>
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
    <button class="tb-btn" disabled={!undoAvail} title="Undo (Ctrl+Z)" onclick={() => runAction(() => undoFlow())}>↩</button>
    <button class="tb-btn" disabled={!redoAvail} title="Redo (Ctrl+Y)" onclick={() => runAction(() => redoFlow())}>↪</button>
  </div>

  <div class="toolbar-group">
    <button class="tb-btn" disabled={!hasData} title="Insert Row" onclick={() => runAction(() => insertRowFlow())}>+ Row</button>
    <button class="tb-btn" disabled={selectedCount === 0} title="Delete Selected Rows (Del)" onclick={() => runAction(() => deleteSelectedFlow())}>− Rows</button>
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
  <button
    class="tb-btn"
    title="Toggle Sidebar"
    class:active={sidebarVisible}
    onclick={() => uiStore.toggleSidebar()}
  >▤</button>
  <button
    class="tb-btn"
    title="Toggle Dark Mode"
    onclick={() => settings.toggleDarkMode()}
  >{darkMode ? '☀️' : '🌙'}</button>
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
    overflow: hidden;
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