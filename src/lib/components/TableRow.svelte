<script lang="ts">
  import CellEditor from './CellEditor.svelte';
  import type { RowData, ColumnInfo } from '../types';

  let {
    row,
    columns,
    rowIndex,
    isSelected,
    editingCol,
    onSelect,
    onEdit,
    onSave,
    onCancel,
  }: {
    row: RowData;
    columns: ColumnInfo[];
    rowIndex: number;
    isSelected: boolean;
    editingCol: number | null;
    onSelect: (e: MouseEvent) => void;
    onEdit: (colIdx: number) => void;
    onSave: (colIdx: number, val: string | number | boolean | null) => void;
    onCancel: () => void;
  } = $props();

  function formatCell(value: string | number | boolean | null, _dtype: string): string {
    if (value === null || value === undefined) return '\u2014';
    if (typeof value === 'boolean') return value ? '\u2713' : '\u2717';
    return String(value);
  }
</script>

<tr
  class:selected={isSelected}
  onclick={onSelect}
  tabindex="0"
>
  <td class="row-id">{rowIndex + 1}</td>
  {#each columns as col, colIdx}
    <td
      class="cell"
      class:editing={editingCol === colIdx}
      ondblclick={() => onEdit(colIdx)}
      role="gridcell"
    >
      {#if editingCol === colIdx}
        <CellEditor
          value={row.values[colIdx] ?? null}
          dtype={col.dtype}
          onSave={(val) => onSave(colIdx, val)}
          onCancel={onCancel}
        />
      {:else}
        <span class="cell-value">
          {formatCell(row.values[colIdx], col.dtype)}
        </span>
      {/if}
    </td>
  {/each}
  <td class="actions-col"></td>
</tr>

<style>
  tr {
    cursor: pointer;
  }
  tr:hover {
    background: #f8f9fa;
  }
  tr.selected {
    background: #e8f0fe;
  }
  .row-id {
    text-align: center;
    color: #aaa;
    font-size: 12px;
    padding: 4px 8px;
  }
  .cell {
    padding: 4px 12px;
    border-bottom: 1px solid #f0f0f0;
    font-size: 13px;
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cell.editing {
    padding: 1px;
    overflow: visible;
  }
  .cell-value {
    display: block;
    min-height: 20px;
  }
  .actions-col {
    width: 40px;
  }
</style>
