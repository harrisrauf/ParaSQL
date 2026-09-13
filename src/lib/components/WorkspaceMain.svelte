<script lang="ts">
import SqlEditor from './SqlEditor.svelte';
import DataGrid from './DataGrid.svelte';
import { workspaceStore } from '../stores/workspace';
import { tableStore } from '../stores/table';
import * as a from '../actions';

let view = $derived($workspaceStore.activeView);

function setView(v: 'query' | 'data' | 'charts') {
  if (v === 'data') {
    a.showEditorDataFlow();
  } else {
    workspaceStore.update(s => ({ ...s, activeView: v }));
  }
}
</script>

<div class="ws-main">
  <div class="tabbar" role="tablist" aria-label="Workspace views">
    <button
      class="tab"
      class:active={view === 'data'}
      role="tab"
      aria-selected={view === 'data'}
      onclick={() => setView('data')}
    >
      Data
    </button>
    <button
      class="tab"
      class:active={view === 'query'}
      role="tab"
      aria-selected={view === 'query'}
      onclick={() => setView('query')}
    >
      Query
    </button>
    <button
      class="tab"
      class:active={view === 'charts'}
      role="tab"
      aria-selected={view === 'charts'}
      onclick={() => setView('charts')}
    >
      Charts
    </button>
  </div>

  <div class="content">
    {#if view === 'query'}
      <div class="query-col">
        <div class="editor-wrap">
          <SqlEditor />
        </div>
        {#if $tableStore.sqlResult}
          <div class="results">
            <DataGrid />
          </div>
        {:else}
          <div class="hint">
            Every sidebar table is a view over its parquet file — run queries
            that join across tables (Ctrl+Enter to run). Double-click a table
            to open it for editing.
          </div>
        {/if}
      </div>
    {:else if view === 'data'}
      {#if $tableStore.columns.length > 0}
        <DataGrid />
      {:else}
        <div class="hint">
          Click a table in the sidebar to preview it, or double-click to open it
          for editing. Query results appear in the Query tab.
        </div>
      {/if}
    {:else}
      <div class="placeholder">
        <div class="placeholder-title">Charts</div>
        <div class="placeholder-body">
          The chart builder lands next: pick a query result, choose a chart
          type (bar / line / scatter / pie), and save it to the workspace.
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .ws-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .tabbar {
    display: flex;
    gap: 2px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border-color, #e0e0e0);
    background: var(--toolbar-bg, #f5f5f5);
  }

  .tab {
    padding: 6px 14px;
    border: none;
    background: none;
    font-size: 12px;
    color: var(--text-secondary, #888);
    cursor: pointer;
    border-bottom: 2px solid transparent;
  }

  .tab:hover {
    color: var(--text-primary, #333);
  }

  .tab.active {
    color: var(--accent-color, #1a73e8);
    border-bottom-color: var(--accent-color, #1a73e8);
    font-weight: 600;
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .query-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .editor-wrap {
    padding: 8px;
    border-bottom: 1px solid var(--border-color, #e0e0e0);
  }

  .results {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 8px;
  }

  .hint {
    margin: 24px 16px;
    padding: 14px 16px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-secondary, #888);
    border: 1px dashed var(--border-color, #d0d0d0);
    border-radius: 6px;
  }

  .placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text-secondary, #888);
  }

  .placeholder-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary, #333);
  }

  .placeholder-body {
    font-size: 12px;
    max-width: 420px;
    text-align: center;
    line-height: 1.6;
  }
</style>
