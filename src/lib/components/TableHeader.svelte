<script lang="ts">
  import type { ColumnInfo, SortConfig } from '../types';

  let { columns, sort, onSort }: {
    columns: ColumnInfo[];
    sort: SortConfig;
    onSort: (col: string) => void;
  } = $props();
</script>

<thead>
  <tr>
    <th class="row-id-col">#</th>
    {#each columns as col}
      <th
        class="col-header"
        class:sort-active={sort.column === col.name}
        onclick={() => onSort(col.name)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === 'Enter' && onSort(col.name)}
      >
        <span class="col-name">{col.name}</span>
        <span class="col-type">{col.dtype}</span>
        {#if sort.column === col.name}
          <span class="sort-indicator">
            {sort.direction === 'asc' ? '▲' : '▼'}
          </span>
        {/if}
      </th>
    {/each}
    <th class="actions-col"></th>
  </tr>
</thead>

<style>
  .row-id-col {
    width: 48px;
    min-width: 48px;
    text-align: center;
    color: #888;
    font-size: 12px;
  }
  .col-header {
    position: relative;
    cursor: pointer;
    user-select: none;
    padding: 8px 12px;
    text-align: left;
    border-bottom: 2px solid #e0e0e0;
    white-space: nowrap;
  }
  .col-header:hover {
    background: #f5f5f5;
  }
  .col-header.sort-active {
    color: #1a73e8;
    border-bottom-color: #1a73e8;
  }
  .col-name {
    display: block;
    font-weight: 600;
    font-size: 13px;
  }
  .col-type {
    display: block;
    font-size: 11px;
    color: #999;
    font-weight: 400;
  }
  .sort-indicator {
    margin-left: 4px;
    font-size: 10px;
  }
  .actions-col {
    width: 40px;
  }
</style>
