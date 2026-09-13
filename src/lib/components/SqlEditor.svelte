<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, placeholder as ph } from '@codemirror/view';
  import { Compartment, EditorState } from '@codemirror/state';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { sql, MySQL, PostgreSQL } from '@codemirror/lang-sql';
  import { oneDark } from '@codemirror/theme-one-dark';
  import { autocompletion } from '@codemirror/autocomplete';
  import { bracketMatching, syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language';
  import { tableStore } from '../stores/table';
  import { workspaceStore } from '../stores/workspace';
  import { settings } from '../stores/settings';
  import { queryHistoryStore } from '../stores/ui';
  import { executeSql } from '../commands';
  import { copyAllResultToClipboard, exportResultFlow } from '../actions';
  import type { QueryResult } from '../types';

  let editorEl: HTMLDivElement | undefined = $state();
  let editorView: EditorView | undefined = $state();
  let isRunning = $state(false);
  let lastError = $state('');
  let queryHistory = $derived($queryHistoryStore);
  let exportOpen = $state(false);

  let columns = $derived($tableStore.columns);
  let sqlResult = $derived($tableStore.sqlResult);
  let dark = $derived($settings.darkMode);
  let wsTables = $derived($workspaceStore.doc?.tables ?? []);

  const themeCompartment = new Compartment();
  const schemaCompartment = new Compartment();
  const lightTheme = [syntaxHighlighting(defaultHighlightStyle, { fallback: true })];

  function defaultQueryText(): string {
    if (wsTables.length > 0) {
      const name = wsTables[0].name.replace(/"/g, '""');
      return `SELECT * FROM "${name}" LIMIT 100;`;
    }
    return 'SELECT * FROM working LIMIT 100;';
  }

  function schemaConfig(): { schema: Record<string, string[]> } | undefined {
    if (wsTables.length > 0) {
      const schema: Record<string, string[]> = {};
      for (const t of wsTables) schema[t.name] = [];
      return { schema };
    }
    return columns.length > 0 ? { schema: { working: columns.map(col => col.name) } } : undefined;
  }

  onMount(() => {
    if (!editorEl) return;

    const schemaCompletion = sql(schemaConfig());

    const runKeymap = keymap.of([{
      key: 'Mod-Enter',
      run: () => { runQuery(); return true; },
    }]);

    const state = EditorState.create({
      doc: defaultQueryText(),
      extensions: [
        history(),
        bracketMatching(),
        autocompletion(),
        schemaCompartment.of(schemaCompletion),
        ph('Type SQL query... (Ctrl+Enter to run)'),
        runKeymap,
        keymap.of([...defaultKeymap, ...historyKeymap]),
        themeCompartment.of(dark ? oneDark : lightTheme),
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

  // Reconfigure theme when dark mode changes
  $effect(() => {
    const view = editorView;
    if (!view) return;
    view.dispatch({ effects: themeCompartment.reconfigure(dark ? oneDark : lightTheme) });
  });

  // Refresh schema completions when columns change (file opened, schema edited)
  $effect(() => {
    const view = editorView;
    if (!view) return;
    const schemaCompletion = sql(schemaConfig());
    view.dispatch({ effects: schemaCompartment.reconfigure(schemaCompletion) });
  });

  async function runQuery() {
    if (!editorView || isRunning) return;

    const sql_text = editorView.state.doc.toString().trim();
    if (!sql_text) return;

    isRunning = true;
    lastError = '';

    try {
      const result = await executeSql(sql_text);
      tableStore.applyQueryResult(result, sql_text);

      // Add to history
      queryHistoryStore.push(sql_text);

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
      <span class="result-count">{sqlResult.rows.length} rows returned</span>
      <div class="result-actions">
        <button class="result-btn" onclick={copyAllResultToClipboard}>Copy all</button>
        <div class="export-wrap">
          <button class="result-btn" onclick={() => (exportOpen = !exportOpen)}>Export ▾</button>
          {#if exportOpen}
            <div class="export-menu">
              <button onclick={() => { exportOpen = false; exportResultFlow('csv'); }}>CSV (.csv)</button>
              <button onclick={() => { exportOpen = false; exportResultFlow('excel'); }}>Excel (.xlsx)</button>
              <button onclick={() => { exportOpen = false; exportResultFlow('parquet'); }}>Parquet (.parquet)</button>
            </div>
          {/if}
        </div>
        <button class="clear-btn" onclick={clearResults}>Clear Results</button>
      </div>
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
    color: var(--accent-text, #fff);
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
    background: var(--error-bg, #fef2f2);
    border-top: 1px solid var(--error-border, #fecaca);
    font-size: 12px;
    color: var(--error-text, #991b1b);
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
    gap: 8px;
    padding: 4px 10px;
    border-top: 1px solid var(--border-color, #e0e0e0);
    background: transparent;
    font-size: 11px;
    color: var(--text-secondary, #888);
  }

  .result-count {
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  .result-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .result-btn {
    padding: 2px 8px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: none;
    cursor: pointer;
    font-size: 11px;
    font-family: inherit;
    color: var(--text-secondary, #777);
  }

  .result-btn:hover {
    border-color: var(--border-color, #ddd);
    background: var(--hover-bg, #f5f5f5);
    color: var(--text-primary, #333);
  }

  .export-wrap {
    position: relative;
  }

  .export-menu {
    position: absolute;
    right: 0;
    top: 100%;
    margin-top: 2px;
    min-width: 140px;
    background: var(--bg, #fff);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
    padding: 4px 0;
    z-index: 30;
  }

  .export-menu button {
    display: block;
    width: 100%;
    border: none;
    background: none;
    padding: 6px 12px;
    text-align: left;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary, #333);
    cursor: pointer;
  }

  .export-menu button:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .clear-btn {
    padding: 3px 12px;
    border: 1px solid var(--border-color, #ddd);
    border-radius: 4px;
    background: var(--panel-bg, #fff);
    cursor: pointer;
    font-size: 11px;
    color: var(--text-primary, #333);
    font-family: inherit;
  }

  .clear-btn:hover {
    background: var(--hover-bg, #f0f0f0);
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
