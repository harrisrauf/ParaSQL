<script lang="ts">
  import { tableStore, displayedRows, selectedRowIds } from '../stores/table';

  let rows = $derived($tableStore.rows);
  let columns = $derived($tableStore.columns);
  let totalRows = $derived($tableStore.totalRows);
  let modified = $derived($tableStore.modified);
  let filePath = $derived($tableStore.filePath);
  let sort = $derived($tableStore.sort);
  let filters = $derived($tableStore.filters);
  let search = $derived($tableStore.search);
  let visibleRows = $derived($displayedRows);
  let selected = $derived($selectedRowIds);
  let sqlResult = $derived($tableStore.sqlResult);

  let fileName = $derived(filePath ? filePath.split(/[\\/]/).pop() : '');
  let filtered = $derived(
    Object.keys(filters).some(k => (filters[k]?.size ?? 0) > 0) || (search.query.length > 0)
  );
</script>

<div class="statusbar">
  <span class="status-item">
    {visibleRows.length.toLocaleString()} of {totalRows.toLocaleString()} rows
    {#if filtered}
      <span class="filtered-badge">filtered</span>
    {/if}
  </span>
  <span class="status-item">{columns.length} cols</span>
  {#if selected.size > 0}
    <span class="status-item selected">{selected.size.toLocaleString()} selected</span>
  {/if}
  {#if sort.column}
    <span class="status-item">
      sorted: {sort.column} {sort.direction === 'asc' ? '↑' : '↓'}
    </span>
  {/if}
  {#if sqlResult}
    <span class="status-item query-badge">query results</span>
  {/if}
  <span class="status-spacer"></span>
  {#if fileName}
    <span class="status-item filename" title={filePath ?? ''}>{fileName}</span>
  {/if}
  <span class="status-item" class:modified={modified} title={modified ? 'Unsaved changes' : 'Saved'}>
    {modified ? '● Modified' : '○ Saved'}
  </span>
</div>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 3px 10px;
    background: var(--panel-bg, #fafafa);
    border-top: 1px solid var(--border-color, #e0e0e0);
    font-size: 11px;
    color: var(--text-secondary, #888);
    flex-shrink: 0;
    user-select: none;
    white-space: nowrap;
  }

  .status-item.selected {
    color: var(--accent-color, #1a73e8);
    font-weight: 600;
  }

  .status-item.modified {
    color: #e67e22;
  }

  .filtered-badge,
  .query-badge {
    margin-left: 4px;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 10px;
    background: var(--selected-bg, #e8f0fe);
    color: var(--accent-color, #1a73e8);
  }

  .status-spacer {
    flex: 1;
  }

  .filename {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>