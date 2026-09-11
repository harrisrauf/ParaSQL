<script lang="ts">
import { get } from 'svelte/store';
import { onDestroy } from 'svelte';
import { workspaceStore } from '../stores/workspace';
import { tableStore } from '../stores/table';
import * as a from '../actions';
import type { WorkspaceTable } from '../types';

let search = $state('');
let selectedTable = $state<string | null>(null);

let doc = $derived($workspaceStore.doc);
let hasEditor = $derived($workspaceStore.editorOpen);
let editorLabel = $derived(
  hasEditor ? (get(tableStore).filePath?.split(/[\\/]/).pop() ?? 'editor') : null
);

let filtered = $derived(
  search.trim() === ''
    ? doc?.tables ?? []
    : (doc?.tables ?? []).filter(t => t.name.toLowerCase().includes(search.trim().toLowerCase()))
);

function fmtSize(size: number | null): string {
  if (size == null) return '';
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1024 * 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  return `${(size / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

let clickTimer: ReturnType<typeof setTimeout> | undefined;
onDestroy(() => clearTimeout(clickTimer));

function handleTableClick(t: WorkspaceTable) {
  selectedTable = t.name;
  if (t.missing) return;
  // Defer the single-click action so a double-click does not also fire a query.
  clearTimeout(clickTimer);
  clickTimer = setTimeout(() => {
    clickTimer = undefined;
    void a.runTableQueryFlow(t.name);
  }, 220);
}

async function handleDoubleClick(t: WorkspaceTable) {
  clearTimeout(clickTimer);
  clickTimer = undefined;
  if (t.missing) return;
  await a.openEditorFlow(t.name);
}

async function handleKeydown(e: KeyboardEvent, t: WorkspaceTable) {
  if (e.key !== 'Enter') return;
  e.preventDefault();
  if (e.ctrlKey || e.metaKey) {
    await a.openEditorFlow(t.name);
  } else {
    await handleTableClick(t);
  }
}

// --- Table context menu ---

let ctx = $state<{ show: boolean; x: number; y: number; table: WorkspaceTable | null }>({
  show: false, x: 0, y: 0, table: null,
});

function openCtx(e: MouseEvent, t: WorkspaceTable) {
  e.preventDefault();
  e.stopPropagation();
  selectedTable = t.name;
  ctx = { show: true, x: Math.min(e.clientX, window.innerWidth - 240), y: Math.min(e.clientY, window.innerHeight - 260), table: t };
}

function closeCtx() {
  ctx.show = false;
}

async function ctxOpen() {
  if (ctx.table && !ctx.table.missing) await a.openEditorFlow(ctx.table.name);
  closeCtx();
}

async function ctxQuery() {
  if (ctx.table && !ctx.table.missing) await a.runTableQueryFlow(ctx.table.name);
  closeCtx();
}

async function ctxToggleMode() {
  const t = ctx.table;
  if (!t) return closeCtx();
  await a.setTableModeFlow(t.name, t.mode === 'editable' ? 'query' : 'editable');
  closeCtx();
}

async function ctxRename() {
  const t = ctx.table;
  if (!t) return closeCtx();
  const name = prompt('New table name', t.name);
  if (name) await a.renameTableFlow(t.name, name);
  closeCtx();
}

async function ctxRemove() {
  const t = ctx.table;
  if (!t) return closeCtx();
  if (confirm(`Remove "${t.name}" from the workspace? The file stays on disk.`)) {
    await a.removeTableFlow(t.name);
  }
  closeCtx();
}

async function ctxExport() {
  const t = ctx.table;
  if (!t) return closeCtx();
  await a.exportTableFlow(t.name);
  closeCtx();
}

async function ctxCloseEditor() {
  await a.closeEditorFlow();
  closeCtx();
}

</script>

<section class="panel">
  <div class="panel-header">
    <span class="title">Tables</span>
    <span class="count">{doc?.tables.length ?? 0}</span>
    <button class="add-btn" title="Add a parquet file" onclick={() => a.addTableFlow()} aria-label="Add table">+</button>
  </div>

  <input
    class="search"
    type="search"
    placeholder="Search tables…"
    bind:value={search}
    aria-label="Search tables"
  />

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="tree" role="tree" aria-label="Workspace tables" tabindex="0" onclick={() => closeCtx()}>
    {#if hasEditor}
      <div class="editor-entry" role="treeitem" tabindex="0" aria-selected={false}>
        <span class="editor-icon">✎</span>
        <span class="name" title="{$tableStore.filePath ?? ''}">Editor: {editorLabel}</span>
        <button class="close-btn" title="Close editor" onclick={(e) => { e.stopPropagation(); a.closeEditorFlow(); }} aria-label="Close editor">×</button>
      </div>
    {/if}

    {#if filtered.length === 0}
      <div class="empty">
        {#if search}
          No tables match "{search}"
        {:else}
          No tables yet — use Workspace → Add Table / Add Folder.
        {/if}
      </div>
    {/if}

    {#each filtered as t (t.name)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="table-entry"
        class:selected={selectedTable === t.name}
        class:missing={t.missing}
        role="treeitem"
        tabindex="0"
        aria-selected={selectedTable === t.name}
        onclick={() => handleTableClick(t)}
        ondblclick={() => handleDoubleClick(t)}
        onkeydown={(e) => handleKeydown(e, t)}
        oncontextmenu={(e) => openCtx(e, t)}
      >
        <span class="name" title="{t.path}{t.missing ? ' (missing)' : ''}">
          {t.name}
          {#if t.mode === 'editable'}<span class="badge">edit</span>{/if}
        </span>
        <span class="meta">
          {fmtSize(t.size)}{t.size != null ? ' · ' : ''}{t.compression}
        </span>
      </div>
    {/each}
  </div>

  {#if ctx.show && ctx.table}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
    <div
      class="ctx-menu"
      role="menu"
      tabindex="-1"
      style="left: {ctx.x}px; top: {ctx.y}px;"
      onclick={(e) => e.stopPropagation()}
    >
      <button role="menuitem" onclick={ctxQuery}>Query table</button>
      <button role="menuitem" onclick={ctxOpen}>Open for editing</button>
      <div class="sep"></div>
      <button role="menuitem" onclick={ctxToggleMode}>
        {ctx.table.mode === 'editable' ? 'Mark query-only' : 'Mark editable'}
      </button>
      <button role="menuitem" onclick={ctxExport}>Export to parquet…</button>
      <div class="sep"></div>
      <button role="menuitem" onclick={ctxRename}>Rename…</button>
      <button role="menuitem" onclick={ctxRemove}>Remove from workspace</button>
      {#if hasEditor}
        <div class="sep"></div>
        <button role="menuitem" onclick={ctxCloseEditor}>Close editor</button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .panel {
    width: 240px;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    background: var(--panel-bg, #fafafa);
    border-right: 1px solid var(--border-color, #e0e0e0);
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-color, #e0e0e0);
  }

  .title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary, #333);
    flex: 1;
  }

  .count {
    font-size: 11px;
    color: var(--text-secondary, #888);
  }

  .add-btn {
    width: 20px;
    height: 20px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 4px;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
  }

  .add-btn:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .search {
    margin: 8px 10px;
    padding: 4px 8px;
    font-size: 12px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 4px;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
    outline: none;
  }

  .search:focus {
    border-color: var(--accent-color, #1a73e8);
  }

  .tree {
    flex: 1;
    overflow: auto;
    padding: 0 4px 8px;
    min-height: 0;
  }

  .table-entry,
  .editor-entry {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
    user-select: none;
  }

  .table-entry:hover,
  .editor-entry:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .table-entry.selected {
    background: color-mix(in srgb, var(--accent-color, #1a73e8) 12%, transparent);
  }

  .table-entry.missing .name {
    color: var(--text-secondary, #888);
    text-decoration: line-through;
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary, #333);
  }

  .meta {
    font-size: 10px;
    color: var(--text-secondary, #888);
    white-space: nowrap;
  }

  .badge {
    font-size: 9px;
    padding: 0 4px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent-color, #1a73e8) 15%, transparent);
    color: var(--accent-color, #1a73e8);
    margin-left: 4px;
    vertical-align: 1px;
  }

  .editor-icon {
    color: var(--accent-color, #1a73e8);
    font-size: 12px;
  }

  .close-btn {
    border: none;
    background: none;
    color: var(--text-secondary, #888);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    padding: 0 2px;
  }

  .close-btn:hover {
    color: var(--text-primary, #333);
  }

  .empty {
    padding: 16px 10px;
    font-size: 11px;
    color: var(--text-secondary, #888);
    line-height: 1.5;
  }

  .ctx-menu {
    position: fixed;
    z-index: 1000;
    min-width: 190px;
    padding: 4px;
    background: var(--bg, #fff);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
  }

  .ctx-menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 10px;
    border: none;
    background: none;
    font-size: 12px;
    color: var(--text-primary, #333);
    cursor: pointer;
    border-radius: 4px;
  }

  .ctx-menu button:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .ctx-menu .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border-color, #e0e0e0);
  }
</style>
