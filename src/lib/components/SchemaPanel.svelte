<script lang="ts">
  import { tableStore } from '../stores/table';
  import { dropColumn, renameColumn, addColumn } from '../commands';

  let columns = $derived($tableStore.columns);
  let renamingCol: string | null = $state(null);
  let renameValue = $state('');
  let addingColumn = $state(false);
  let newColName = $state('');
  let newColType = $state('utf8');

  async function handleDrop(name: string) {
    if (!confirm(`Delete column "${name}"? This can be undone.`)) return;
    try {
      const result = await dropColumn(name);
      tableStore.applyMutation(result);
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
        const result = await renameColumn(renamingCol, renameValue);
        tableStore.applyMutation(result);
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
      const result = await addColumn(newColName, newColType);
      tableStore.applyMutation(result);
      newColName = '';
      addingColumn = false;
    } catch (err) {
      console.error('Failed to add column:', err);
    }
  }
</script>

<aside class="schema-panel">
  <div class="panel-header">
    <h3>Schema</h3>
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
          <span class="column-type">{col.dtype}</span>
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
      <button onclick={handleAddColumn} class="add-btn">Add</button>
      <button onclick={() => addingColumn = false} class="cancel-btn">Cancel</button>
    </div>
  {:else}
    <button onclick={() => addingColumn = true} class="add-column-btn">
      + Add column
    </button>
  {/if}
</aside>

<style>
  .schema-panel {
    width: 220px;
    min-width: 220px;
    border-right: 1px solid #e0e0e0;
    display: flex;
    flex-direction: column;
    background: #fafafa;
    overflow-y: auto;
  }
  .panel-header {
    padding: 12px;
    border-bottom: 1px solid #e0e0e0;
  }
  .panel-header h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: #555;
  }
  .column-list {
    flex: 1;
    overflow-y: auto;
  }
  .column-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    border-bottom: 1px solid #f0f0f0;
  }
  .column-item:hover {
    background: #f0f0f0;
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
    color: #999;
  }
  .drop-btn {
    border: none;
    background: none;
    cursor: pointer;
    color: #ccc;
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
    border: 1px solid #1a73e8;
    outline: none;
    border-radius: 2px;
    font-family: inherit;
    box-sizing: border-box;
  }
  .add-column-btn {
    margin: 8px 12px;
    padding: 6px;
    border: 1px dashed #ccc;
    border-radius: 4px;
    background: none;
    cursor: pointer;
    font-size: 12px;
    color: #666;
    font-family: inherit;
  }
  .add-column-btn:hover {
    border-color: #1a73e8;
    color: #1a73e8;
  }
  .add-column-form {
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-top: 1px solid #e0e0e0;
  }
  .add-input, .add-select {
    padding: 4px 6px;
    font-size: 12px;
    border: 1px solid #ddd;
    border-radius: 3px;
    font-family: inherit;
  }
  .add-btn, .cancel-btn {
    padding: 4px 8px;
    border: 1px solid #ddd;
    border-radius: 3px;
    background: white;
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
  }
  .add-btn {
    background: #1a73e8;
    color: white;
    border-color: #1a73e8;
  }
  .cancel-btn {
    margin-left: 4px;
  }
</style>
