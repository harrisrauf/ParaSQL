<script lang="ts">
  import { tableStore } from '../stores/table';

  let columns = $derived($tableStore.columns);
  let rows = $derived($tableStore.rows);
  let modified = $derived($tableStore.modified);
  let filePath = $derived($tableStore.filePath);
  let totalRows = $derived($tableStore.totalRows);
  let sort = $derived($tableStore.sort);

  let fileName = $derived(
    filePath ? filePath.split(/[/\\]/).pop() : 'Untitled'
  );
  let sortInfo = $derived(
    sort.direction ? `Sorted by: ${sort.column} (${sort.direction})` : ''
  );
</script>

<footer class="status-bar">
  <span class="status-item">
    {rows.length.toLocaleString()} of {totalRows.toLocaleString()} rows
  </span>
  <span class="status-item">{columns.length} cols</span>
  {#if sortInfo}
    <span class="status-item">{sortInfo}</span>
  {/if}
  <span class="status-spacer"></span>
  <span class="status-item" class:modified={modified}>
    {modified ? '● Modified' : '○ Saved'}
  </span>
  <span class="status-item file-name">{fileName}</span>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    padding: 4px 12px;
    border-top: 1px solid var(--border-color, #e0e0e0);
    background: var(--toolbar-bg, #fafafa);
    font-size: 12px;
    color: var(--text-secondary, #888);
    gap: 16px;
  }

  .status-item {
    white-space: nowrap;
  }

  .status-spacer {
    flex: 1;
  }

  .modified {
    color: #e67e22;
  }

  .file-name {
    color: var(--text-primary, #555);
    font-weight: 500;
  }
</style>
