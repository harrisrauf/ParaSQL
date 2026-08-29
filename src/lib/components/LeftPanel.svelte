<script lang="ts">
  import SqlEditor from './SqlEditor.svelte';
  import { tableStore } from '../stores/table';
  import { settings, type SchemaDialect } from '../stores/settings';
  import { generateSchema, getColumns, getAllRows, dropColumn, renameColumn, addColumn } from '../commands';

  let columns = $derived($tableStore.columns);
  let activeTab = $derived($tableStore.activeTab);

  let renamingCol: string | null = $state(null);
  let renameValue = $state('');
  let addingColumn = $state(false);
  let newColName = $state('');
  let newColType = $state('utf8');
  let schemaSql = $state('');
  let loadingSchema = $state(false);

  let filePath = $derived($tableStore.filePath);
  let totalRows = $derived($tableStore.totalRows);
  let modified = $derived($tableStore.modified);
  let dialect = $derived($settings.schemaDialect);

  const COLUMN_TYPES = ['utf8', 'int64', 'int32', 'float64', 'float32', 'boolean'];

  function switchTab(tab: 'data' | 'schema' | 'query' | 'metadata') {
    tableStore.setActiveTab(tab);
  }

  function startRename(col: string) {
    renamingCol = col;
    renameValue = col;
  }

  async function finishRename() {
    if (renamingCol && renameValue && renameValue !== renamingCol) {
      try {
        await renameColumn(renamingCol, renameValue);
        const [rows, cols] = await Promise.all([getAllRows(), getColumns()]);
        tableStore.update(s => ({ ...s, rows, columns: cols, modified: true }));
      } catch (err) {
        console.error('Failed to rename column:', err);
      }
    }
    renamingCol = null;
  }

  async function handleDrop(name: string) {
    if (!confirm(`Delete column "${name}"? This can be undone.`)) return;
    try {
      await dropColumn(name);
      const [rows, cols] = await Promise.all([getAllRows(), getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns: cols, modified: true }));
    } catch (err) {
      console.error('Failed to drop column:', err);
    }
  }

  async function handleAddColumn() {
    if (!newColName) return;
    try {
      await addColumn(newColName, newColType);
      const [rows, cols] = await Promise.all([getAllRows(), getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns: cols, modified: true }));
      newColName = '';
      addingColumn = false;
    } catch (err) {
      console.error('Failed to add column:', err);
    }
  }

  async function loadSchema() {
    loadingSchema = true;
    try {
      schemaSql = await generateSchema(dialect);
    } catch (err) {
      console.error('Failed to generate schema:', err);
    }
    loadingSchema = false;
  }

  async function copySchema() {
    if (!schemaSql) await loadSchema();
    try {
      await navigator.clipboard.writeText(schemaSql);
    } catch (err) {
      console.error('Failed to copy schema:', err);
    }
  }

  function setDialect(d: SchemaDialect) {
    settings.setSchemaDialect(d);
    loadSchema();
  }

  let fileName = $derived(filePath ? filePath.split(/[\\/]/).pop() : '');
</script>

<aside class="left-panel">
  <div class="panel-tabs">
    <button class:active={activeTab === 'schema'} onclick={() => switchTab('schema')}>Schema</button>
    <button class:active={activeTab === 'query'} onclick={() => switchTab('query')}>Query</button>
    <button class:active={activeTab === 'metadata'} onclick={() => switchTab('metadata')}>Info</button>
  </div>

  <div class="panel-content">
    {#if activeTab === 'schema'}
      <div class="schema-tab">
        <div class="schema-list">
          {#if columns.length === 0}
            <p class="empty-hint">No columns</p>
          {/if}
          {#each columns as col (col.name)}
            <div class="schema-row">
              {#if renamingCol === col.name}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  type="text"
                  bind:value={renameValue}
                  onkeydown={(e) => {
                    if (e.key === 'Enter') finishRename();
                    if (e.key === 'Escape') renamingCol = null;
                  }}
                  onblur={finishRename}
                  autofocus
                />
              {:else}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="schema-col-name" ondblclick={() => startRename(col.name)} title="Double-click to rename">
                  {col.name}
                </div>
              {/if}
              <span class="schema-col-type">{col.dtype}</span>
              <button class="drop-btn" onclick={() => handleDrop(col.name)} title="Drop column">✕</button>
            </div>
          {/each}
        </div>

        {#if addingColumn}
          <div class="add-column">
            <!-- svelte-ignore a11y_autofocus -->
            <input
              type="text"
              placeholder="Column name"
              bind:value={newColName}
              onkeydown={(e) => {
                if (e.key === 'Enter') handleAddColumn();
                if (e.key === 'Escape') addingColumn = false;
              }}
              autofocus
            />
            <select bind:value={newColType}>
              {#each COLUMN_TYPES as t}
                <option value={t}>{t}</option>
              {/each}
            </select>
            <button class="add-btn" onclick={handleAddColumn}>Add</button>
            <button class="cancel-btn" onclick={() => (addingColumn = false)}>Cancel</button>
          </div>
        {:else}
          <button class="add-col-btn" onclick={() => (addingColumn = true)}>+ Add column</button>
        {/if}
      </div>
    {:else if activeTab === 'query'}
      <SqlEditor />
    {:else}
      <div class="info-tab">
        <div class="info-group">
          <span class="info-label">File</span>
          <span class="info-value" title={filePath ?? ''}>{fileName || '—'}</span>
        </div>
        <div class="info-group">
          <span class="info-label">Rows</span>
          <span class="info-value">{totalRows.toLocaleString()}</span>
        </div>
        <div class="info-group">
          <span class="info-label">Columns</span>
          <span class="info-value">{columns.length}</span>
        </div>
        <div class="info-group">
          <span class="info-label">Status</span>
          <span class="info-value" class:modified={modified}>{modified ? 'Modified' : 'Saved'}</span>
        </div>

        <div class="schema-section">
          <div class="schema-header">
            <span>SQL Schema</span>
            <select value={dialect} onchange={(e) => setDialect(e.currentTarget.value as SchemaDialect)} title="SQL dialect">
              <option value="duckdb">DuckDB</option>
              <option value="postgres">PostgreSQL</option>
              <option value="mysql">MySQL</option>
              <option value="sqlite">SQLite</option>
            </select>
          </div>
          <div class="schema-actions">
            <button onclick={loadSchema} disabled={loadingSchema}>{loadingSchema ? '…' : 'Generate'}</button>
            <button onclick={copySchema}>Copy</button>
          </div>
          {#if schemaSql}
            <pre class="schema-sql">{schemaSql}</pre>
          {:else}
            <p class="empty-hint">Generate a CREATE TABLE statement for the current schema.</p>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</aside>

<style>
  .left-panel {
    width: 240px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    background: var(--panel-bg, #fafafa);
    border-right: 1px solid var(--border-color, #e0e0e0);
    min-height: 0;
  }

  .panel-tabs {
    display: flex;
    border-bottom: 1px solid var(--border-color, #e0e0e0);
    flex-shrink: 0;
  }

  .panel-tabs button {
    flex: 1;
    border: none;
    background: none;
    font-family: inherit;
    font-size: 12px;
    color: var(--text-secondary, #888);
    padding: 8px 4px;
    cursor: pointer;
    border-bottom: 2px solid transparent;
  }

  .panel-tabs button:hover {
    color: var(--text-primary, #333);
  }

  .panel-tabs button.active {
    color: var(--accent-color, #1a73e8);
    border-bottom-color: var(--accent-color, #1a73e8);
    font-weight: 600;
  }

  .panel-content {
    flex: 1;
    overflow: auto;
    padding: 8px;
    min-height: 0;
  }

  .schema-tab {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .schema-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .schema-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    border-radius: 4px;
  }

  .schema-row:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .schema-col-name {
    flex: 1;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: default;
  }

  .schema-col-type {
    font-size: 10px;
    color: var(--text-secondary, #999);
    flex-shrink: 0;
  }

  .schema-row input {
    flex: 1;
    font-size: 12px;
    font-family: inherit;
    padding: 2px 4px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 3px;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
    min-width: 0;
  }

  .drop-btn {
    border: none;
    background: none;
    color: var(--text-secondary, #999);
    cursor: pointer;
    font-size: 11px;
    padding: 2px 4px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .drop-btn:hover {
    background: #f5c6c6;
    color: #c0392b;
  }

  .add-col-btn {
    border: 1px dashed var(--border-color, #ccc);
    background: none;
    color: var(--text-secondary, #888);
    font-family: inherit;
    font-size: 12px;
    padding: 6px;
    border-radius: 4px;
    cursor: pointer;
  }

  .add-col-btn:hover {
    color: var(--accent-color, #1a73e8);
    border-color: var(--accent-color, #1a73e8);
  }

  .add-column {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 4px;
  }

  .add-column input,
  .add-column select {
    font-size: 12px;
    font-family: inherit;
    padding: 3px 6px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 3px;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
  }

  .add-btn,
  .cancel-btn {
    border: none;
    border-radius: 3px;
    font-size: 12px;
    font-family: inherit;
    padding: 4px;
    cursor: pointer;
  }

  .add-btn {
    background: var(--accent-color, #1a73e8);
    color: white;
  }

  .cancel-btn {
    background: var(--hover-bg, #e8e8e8);
    color: var(--text-primary, #333);
  }

  .info-tab {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
  }

  .info-group {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .info-group .info-label {
    color: var(--text-secondary, #888);
  }

  .info-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .info-value.modified {
    color: #e67e22;
  }

  .schema-section {
    margin-top: 8px;
    border-top: 1px solid var(--border-color, #e0e0e0);
    padding-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .schema-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-weight: 600;
  }

  .schema-header select {
    font-size: 11px;
    font-family: inherit;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 3px;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
    padding: 2px 4px;
  }

  .schema-actions {
    display: flex;
    gap: 4px;
  }

  .schema-actions button {
    flex: 1;
    border: 1px solid var(--border-color, #e0e0e0);
    background: var(--bg, #fff);
    border-radius: 3px;
    font-size: 12px;
    font-family: inherit;
    padding: 4px;
    cursor: pointer;
  }

  .schema-actions button:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .schema-sql {
    margin: 0;
    padding: 6px;
    background: var(--code-bg, #f5f5f5);
    border-radius: 4px;
    font-size: 11px;
    overflow: auto;
    max-height: 200px;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--text-primary, #333);
  }

  .empty-hint {
    color: var(--text-secondary, #999);
    font-size: 11px;
    margin: 4px 0;
  }
</style>