<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, placeholder as ph } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { sql, MySQL, PostgreSQL } from '@codemirror/lang-sql';
  import { oneDark } from '@codemirror/theme-one-dark';
  import { autocompletion } from '@codemirror/autocomplete';
  import { bracketMatching } from '@codemirror/language';
  import { tableStore } from '../stores/table';
  import { executeSql } from '../commands';
  import type { QueryResult } from '../types';

  let editorEl: HTMLDivElement | undefined = $state();
  let editorView: EditorView | undefined = $state();
  let isRunning = $state(false);
  let lastError = $state('');
  let queryHistory = $state<string[]>([]);

  let columns = $derived($tableStore.columns);
  let sqlResult = $derived($tableStore.sqlResult);

  let darkMode = $state(false);

  onMount(() => {
    // Check for dark mode
    darkMode = document.documentElement.classList.contains('dark');

    if (!editorEl) return;

    const schemaCompletion = sql({
      schema: columns.length > 0 ? {
        working: columns.map(col => col.name),
      } : undefined,
    });

    const runKeymap = keymap.of([{
      key: 'Mod-Enter',
      run: () => { runQuery(); return true; },
    }]);

    const state = EditorState.create({
      doc: 'SELECT * FROM working LIMIT 100;',
      extensions: [
        history(),
        bracketMatching(),
        autocompletion(),
        schemaCompletion,
        ph('Type SQL query... (Ctrl+Enter to run)'),
        runKeymap,
        keymap.of([...defaultKeymap, ...historyKeymap]),
        darkMode ? oneDark : [],
        EditorView.lineWrapping,
        EditorView.theme({
          '&': { fontSize: '13px', minHeight: '80px', maxHeight: '200px', overflow: 'auto' },
          '.cm-content': { fontFamily: "'SF Mono', 'Fira Code', monospace" },
          '.cm-gutters': { fontSize: '12px' },
        }),
      ],
    });

    editorView = new EditorView({
      state,
      parent: editorEl,
    });
  });

  onDestroy(() => {
    editorView?.destroy();
  });

  // Update theme when dark mode changes
  $effect(() => {
    if (!editorView) return;
    // Recreate editor with new theme on dark mode change
    // This is a simplified approach - a full implementation would use reconfigure
  });

  async function runQuery() {
    if (!editorView || isRunning) return;

    const sql_text = editorView.state.doc.toString().trim();
    if (!sql_text) return;

    isRunning = true;
    lastError = '';

    try {
      const result = await executeSql(sql_text);
      tableStore.applyQueryResult(result);

      // Add to history
      queryHistory = [sql_text, ...queryHistory.filter(q => q !== sql_text)].slice(0, 20);

      // Switch to data view to show results
      tableStore.setActiveTab('data');
    } catch (err) {
      lastError = String(err);
      console.error('Query failed:', err);
    } finally {
      isRunning = false;
    }
  }

  function loadFromHistory(query: string) {
    if (!editorView) return;
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: query },
    });
  }

  function clearResults() {
    tableStore.restoreTable();
  }
</script>

<div class="sql-editor-container">
  <div class="editor-header">
    <span class="editor-label">SQL Query</span>
    <div class="editor-actions">
      <button
        class="run-btn"
        onclick={runQuery}
        disabled={isRunning}
      >
        {isRunning ? 'Running...' : '▶ Run (Ctrl+Enter)'}
      </button>
    </div>
  </div>

  <div class="editor-wrapper" bind:this={editorEl}></div>

  {#if lastError}
    <div class="error-bar">
      <span class="error-icon">⚠</span>
      <span class="error-text">{lastError}</span>
    </div>
  {/if}

  {#if sqlResult}
    <div class="result-info">
      <span>{sqlResult.rows.length} rows returned</span>
      <button class="clear-btn" onclick={clearResults}>Clear results</button>
    </div>
  {/if}

  {#if queryHistory.length > 0}
    <div class="history-section">
      <span class="history-label">Recent queries</span>
      <div class="history-list">
        {#each queryHistory.slice(0, 5) as query}
          <button class="history-item" onclick={() => loadFromHistory(query)}>
            {query.slice(0, 60)}{query.length > 60 ? '...' : ''}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .sql-editor-container {
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    overflow: hidden;
    background: var(--panel-bg, white);
  }

  .editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    background: var(--toolbar-bg, #f5f5f5);
    border-bottom: 1px solid var(--border-color, #e0e0e0);
  }

  .editor-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .editor-actions {
    display: flex;
    gap: 6px;
  }

  .run-btn {
    padding: 4px 12px;
    border: 1px solid var(--accent-color, #1a73e8);
    border-radius: 4px;
    background: var(--accent-color, #1a73e8);
    color: white;
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
    font-weight: 500;
  }

  .run-btn:hover:not(:disabled) {
    background: var(--accent-hover, #1557b0);
  }

  .run-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .editor-wrapper {
    min-height: 80px;
  }

  :global(.cm-editor) {
    border: none;
  }

  .error-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: #fef2f2;
    border-top: 1px solid #fecaca;
    font-size: 12px;
    color: #991b1b;
  }

  .error-icon {
    font-size: 14px;
  }

  .error-text {
    word-break: break-all;
  }

  .result-info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px;
    background: #f0fdf4;
    border-top: 1px solid #bbf7d0;
    font-size: 11px;
    color: #166534;
  }

  .clear-btn {
    padding: 2px 8px;
    border: 1px solid #bbf7d0;
    border-radius: 3px;
    background: white;
    cursor: pointer;
    font-size: 10px;
    color: #166534;
    font-family: inherit;
  }

  .history-section {
    padding: 6px 10px;
    border-top: 1px solid var(--border-color, #e0e0e0);
    background: var(--toolbar-bg, #fafafa);
  }

  .history-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-secondary, #999);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    display: block;
    margin-bottom: 4px;
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .history-item {
    padding: 3px 6px;
    border: none;
    border-radius: 3px;
    background: none;
    cursor: pointer;
    font-size: 11px;
    font-family: 'SF Mono', 'Fira Code', monospace;
    color: var(--text-primary, #333);
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .history-item:hover {
    background: var(--hover-bg, #f0f0f0);
  }
</style>
