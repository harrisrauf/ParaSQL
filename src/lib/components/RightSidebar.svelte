<script lang="ts">
  import { tableStore } from '../stores/table';
  import { generateSchema, getColumns, getAllRows, dropColumn, renameColumn, addColumn } from '../commands';
  import SqlEditor from './SqlEditor.svelte';

  let activeTab = $derived($tableStore.activeTab);
  let columns = $derived($tableStore.columns);
  let rows = $derived($tableStore.rows);
  let totalRows = $derived($tableStore.totalRows);
  let filePath = $derived($tableStore.filePath);
  let modified = $derived($tableStore.modified);

  let renamingCol: string | null = $state(null);
  let renameValue = $state('');
  let addingColumn = $state(false);
  let newColName = $state('');
  let newColType = $state('utf8');
  let schemaSql = $state('');
  let copiedSchema = $state(false);

  async function loadSchema() {
    try {
      schemaSql = await generateSchema();
    } catch (e) {
      schemaSql = '-- Error generating schema';
    }
  }

  async function copySchema() {
    try {
      await navigator.clipboard.writeText(schemaSql);
      copiedSchema = true;
      setTimeout(() => copiedSchema = false, 2000);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  }

  async function handleDrop(name: string) {
    if (!confirm(`Delete column "${name}"? This can be undone.`)) return;
    try {
      await dropColumn(name);
      const [rows, columns] = await Promise.all([getAllRows(), getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns, modified: true }));
    } catch (err) {
      console.error('Failed to drop column:', err);
    }
  }

  function startRename(name: string) {
    renamingCol = name;
    renameValue = name;
  }

  async function finishRename() {
    if (renamingCol && renameValue && renameValue !== renamingCol) {
      try {
        await renameColumn(renamingCol, renameValue);
        const [rows, columns] = await Promise.all([getAllRows(), getColumns()]);
        tableStore.update(s => ({ ...s, rows, columns, modified: true }));
      } catch (err) {
        console.error('Failed to rename column:', err);
      }
    }
    renamingCol = null;
  }

  function handleRenameKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') finishRename();
    else if (e.key === 'Escape') renamingCol = null;
  }

  async function handleAddColumn() {
    if (!newColName) return;
    try {
      await addColumn(newColName, newColType);
      const [rows, columns] = await Promise.all([getAllRows(), getColumns()]);
      tableStore.update(s => ({ ...s, rows, columns, modified: true }));
      newColName = '';
      addingColumn = false;
    } catch (err) {
      console.error('Failed to add column:', err);
    }
  }

  // Props available but not used yet
  // const { activeTab: initialTab } = $props();
</script>

<aside class="right-sidebar">
  <div class="tab-bar">
    <button
      class="tab-btn"
      class:active={activeTab === 'schema'}
      onclick={() => tableStore.setActiveTab('schema')}
    >Schema</button>
    <button
      class="tab-btn"
      class:active={activeTab === 'query'}
      onclick={() => tableStore.setActiveTab('query')}
    >Query</button>
    <button
      class="tab-btn"
      class:active={activeTab === 'metadata'}
      onclick={() => tableStore.setActiveTab('metadata')}
    >Info</button>
  </div>

  <div class="tab-content">
    {#if activeTab === 'schema'}
      <div class="schema-panel">
        <div class="section-header">
          <span>{columns.length} columns</span>
        </div>
        <div class="column-list">
          {#each columns as col}
            <div class="column-item">
              <div class="column-info">
                {#if renamingCol === col.name}
                  <input
                    type="text"
                    bind:value={renameValue}
                    onkeydown={handleRenameKeydown}
                    onblur={finishRename}
                    class="rename-input"
                  />
                {:else}
                  <span
                    class="column-name"
                    ondblclick={() => startRename(col.name)}
                    onkeydown={(e) => e.key === 'Enter' && startRename(col.name)}
                    title="Double-click to rename"
                    role="button"
                    tabindex="0"
                  >
                    {col.name}
                  </span>
                {/if}
                <span class="column-type">{col.dtype}{col.nullable ? '' : ' NOT NULL'}</span>
              </div>
              <button
                class="drop-btn"
                onclick={() => handleDrop(col.name)}
                title="Drop column"
              >
                ✕
              </button>
            </div>
          {/each}
        </div>

        {#if addingColumn}
          <div class="add-column-form">
            <input
              type="text"
              placeholder="Column name"
              bind:value={newColName}
              class="add-input"
            />
            <select bind:value={newColType} class="add-select">
              <option value="utf8">Text</option>
              <option value="int64">Integer (64)</option>
              <option value="int32">Integer (32)</option>
              <option value="float64">Float</option>
              <option value="boolean">Boolean</option>
            </select>
            <div class="add-actions">
              <button onclick={handleAddColumn} class="add-btn">Add</button>
              <button onclick={() => addingColumn = false} class="cancel-btn">Cancel</button>
            </div>
          </div>
        {:else}
          <button onclick={() => addingColumn = true} class="add-column-btn">
            + Add column
          </button>
        {/if}
      </div>

    {:else if activeTab === 'query'}
      <div class="query-panel">
        <SqlEditor />
      </div>

    {:else if activeTab === 'metadata'}
      <div class="metadata-panel">
        <div class="info-group">
          <label>File</label>
          <span class="info-value">{filePath ?? 'Untitled'}</span>
        </div>
        <div class="info-group">
          <label>Rows</label>
          <span class="info-value">{totalRows.toLocaleString()}</span>
        </div>
        <div class="info-group">
          <label>Columns</label>
          <span class="info-value">{columns.length}</span>
        </div>
        <div class="info-group">
          <label>Status</label>
          <span class="info-value" class:modified={modified}>{modified ? 'Modified' : 'Saved'}</span>
        </div>

        <div class="section-header" style="margin-top: 16px;">
          <span>SQL Schema</span>
          <button class="copy-btn" onclick={async () => { await loadSchema(); }}>
            {schemaSql ? 'Refresh' : 'Generate'}
          </button>
        </div>
        {#if schemaSql}
          <pre class="schema-code">{schemaSql}</pre>
          <button class="copy-schema-btn" onclick={copySchema}>
            {copiedSchema ? 'Copied!' : 'Copy to clipboard'}
          </button>
        {/if}
      </div>
    {/if}
  </div>
</aside>

<style>
  .right-sidebar {
    width: 280px;
    min-width: 280px;
    border-left: 1px solid var(--border-color, #e0e0e0);
    display: flex;
    flex-direction: column;
    background: var(--panel-bg, #fafafa);
    overflow: hidden;
  }

  .tab-bar {
    display: flex;
    border-bottom: 1px solid var(--border-color, #e0e0e0);
  }

  .tab-btn {
    flex: 1;
    padding: 10px 0;
    border: none;
    background: none;
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary, #888);
    border-bottom: 2px solid transparent;
    font-family: inherit;
  }

  .tab-btn.active {
    color: var(--accent-color, #1a73e8);
    border-bottom-color: var(--accent-color, #1a73e8);
  }

  .tab-btn:hover:not(.active) {
    color: var(--text-primary, #333);
    background: var(--hover-bg, #f5f5f5);
  }

  .tab-content {
    flex: 1;
    overflow-y: auto;
  }

  .section-header {
    padding: 8px 12px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border-color, #e0e0e0);
  }

  .column-list {
    overflow-y: auto;
  }

  .column-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle, #f0f0f0);
  }

  .column-item:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .column-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .column-name {
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .column-type {
    font-size: 11px;
    color: var(--text-secondary, #999);
  }

  .drop-btn {
    border: none;
    background: none;
    cursor: pointer;
    color: var(--text-secondary, #ccc);
    font-size: 12px;
    padding: 2px 4px;
    visibility: hidden;
  }

  .column-item:hover .drop-btn {
    visibility: visible;
  }

  .drop-btn:hover {
    color: #e74c3c;
  }

  .rename-input {
    width: 100%;
    padding: 2px 4px;
    font-size: 13px;
    border: 1px solid var(--accent-color, #1a73e8);
    outline: none;
    border-radius: 2px;
    font-family: inherit;
    box-sizing: border-box;
  }

  .add-column-btn {
    margin: 8px 12px;
    padding: 6px;
    border: 1px dashed var(--text-secondary, #ccc);
    border-radius: 4px;
    background: none;
    cursor: pointer;
    font-size: 12px;
    color: var(--text-secondary, #666);
    font-family: inherit;
  }

  .add-column-btn:hover {
    border-color: var(--accent-color, #1a73e8);
    color: var(--accent-color, #1a73e8);
  }

  .add-column-form {
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-top: 1px solid var(--border-color, #e0e0e0);
  }

  .add-input, .add-select {
    padding: 4px 6px;
    font-size: 12px;
    border: 1px solid var(--border-color, #ddd);
    border-radius: 3px;
    font-family: inherit;
  }

  .add-actions {
    display: flex;
    gap: 4px;
  }

  .add-btn {
    padding: 4px 8px;
    border: 1px solid var(--accent-color, #1a73e8);
    border-radius: 3px;
    background: var(--accent-color, #1a73e8);
    color: white;
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
  }

  .cancel-btn {
    padding: 4px 8px;
    border: 1px solid var(--border-color, #ddd);
    border-radius: 3px;
    background: var(--panel-bg, white);
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
  }

  .metadata-panel {
    padding: 12px;
  }

  .query-panel {
    padding: 8px;
  }

  .info-group {
    margin-bottom: 12px;
  }

  .info-group label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 2px;
  }

  .info-value {
    font-size: 13px;
    color: var(--text-primary, #333);
    word-break: break-all;
  }

  .modified {
    color: #e67e22;
  }

  .copy-btn {
    padding: 2px 8px;
    border: 1px solid var(--border-color, #ddd);
    border-radius: 3px;
    background: var(--panel-bg, white);
    cursor: pointer;
    font-size: 10px;
    font-family: inherit;
  }

  .schema-code {
    background: var(--code-bg, #f5f5f5);
    padding: 8px;
    border-radius: 4px;
    font-size: 11px;
    overflow-x: auto;
    margin: 4px 0;
    font-family: 'SF Mono', 'Fira Code', monospace;
    border: 1px solid var(--border-color, #e8e8e8);
    white-space: pre-wrap;
  }

  .copy-schema-btn {
    width: 100%;
    padding: 4px 8px;
    border: 1px solid var(--border-color, #ddd);
    border-radius: 3px;
    background: var(--panel-bg, white);
    cursor: pointer;
    font-size: 11px;
    font-family: inherit;
    margin-top: 4px;
  }
</style>
