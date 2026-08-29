<script lang="ts">
  import { onMount } from 'svelte';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import TableHeader from './TableHeader.svelte';
  import CellEditor from './CellEditor.svelte';
  import { tableStore, displayedRows } from '../stores/table';
  import { editCell } from '../commands';
  import type { RowData } from '../types';

  let editingCell: { rowId: number; colIdx: number } | null = $state(null);
  let parentEl: HTMLElement | undefined = $state();

  let columns = $derived($tableStore.columns);
  let rows = $derived($displayedRows);
  let sort = $derived($tableStore.sort);
  let selectedRowIds = $derived($tableStore.selectedRowIds);

  let virtualizer = $derived(
    parentEl
      ? createVirtualizer({
          count: rows.length,
          getScrollElement: () => parentEl,
          estimateSize: () => 36,
          overscan: 20,
        })
      : null
  );

  let virtualItems = $derived($virtualizer?.getVirtualItems() ?? []);
  let totalSize = $derived($virtualizer?.getTotalSize() ?? 0);

  function handleSort(col: string) {
    tableStore.toggleSort(col);
  }

  function handleRowSelect(row: RowData, e: MouseEvent) {
    tableStore.selectRow(row.row_id, e.shiftKey, e.ctrlKey || e.metaKey);
  }

  function handleEdit(row: RowData, colIdx: number) {
    editingCell = { rowId: row.row_id, colIdx };
  }

  async function handleSave(rowId: number, colIdx: number, value: string | number | boolean | null) {
    try {
      const result = await editCell(rowId, colIdx, value);
      // Update the row in the store with the new values from the result
      if (result.rows.length > 0) {
        const updatedRow = result.rows.find(r => r.row_id === rowId) || result.rows[0];
        tableStore.updateRow(rowId, updatedRow);
        tableStore.update(s => ({ ...s, modified: true }));
      }
    } catch (err) {
      console.error('Failed to edit cell:', err);
    }
    editingCell = null;
  }

  function handleCancel() {
    editingCell = null;
  }

  function formatCell(value: string | number | boolean | null): string {
    if (value === null || value === undefined) return '—';
    if (typeof value === 'boolean') return value ? '✓' : '✗';
    if (typeof value === 'string' && value.startsWith('[') && value.includes('bytes:')) {
      return value; // Binary data
    }
    return String(value);
  }

  function isTruncated(value: string | number | boolean | null): boolean {
    if (value === null || value === undefined) return false;
    return String(value).length > 50;
  }
</script>

<div class="table-wrapper">
  <div class="table-scroll" bind:this={parentEl}>
    <table class="data-table" role="grid">
      <TableHeader {columns} {sort} onSort={handleSort} />
      <tbody>
        <tr style="height: {totalSize}px; padding: 0;">
          <td colspan={columns.length + 2} style="padding: 0; border: none;">
            <div style="position: relative; width: 100%; height: 100%;">
              {#each virtualItems as item (item.key)}
                {@const row = rows[item.index]}
                {@const realIndex = $tableStore.rows.findIndex(r => r.row_id === row.row_id)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="virtual-row"
                  class:selected={selectedRowIds.has(row.row_id)}
                  style="position: absolute; top: {item.start}px; left: 0; right: 0; height: {item.size}px;"
                  onclick={(e) => handleRowSelect(row, e)}
                  ondblclick={() => handleEdit(row, 0)}
                  role="row"
                  tabindex="0"
                >
                  <div class="row-id-cell">{realIndex + 1}</div>
                  {#each columns as col, colIdx}
                    <!-- svelte-ignore a11y_interactive_supports_focus -->
                    <div
                      class="cell"
                      class:editing={editingCell?.rowId === row.row_id && editingCell?.colIdx === colIdx}
                      ondblclick={() => handleEdit(row, colIdx)}
                      role="gridcell"
                    >
                      {#if editingCell?.rowId === row.row_id && editingCell?.colIdx === colIdx}
                        <CellEditor
                          value={row.values[colIdx] ?? null}
                          dtype={col.dtype}
                          onSave={(val) => handleSave(row.row_id, colIdx, val)}
                          onCancel={handleCancel}
                        />
                      {:else}
                        <span
                          class="cell-value"
                          title={isTruncated(row.values[colIdx]) ? String(row.values[colIdx]) : ''}
                        >
                          {formatCell(row.values[colIdx])}
                        </span>
                      {/if}
                    </div>
                  {/each}
                  <div class="actions-cell"></div>
                </div>
              {/each}
            </div>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</div>

<style>
  .table-wrapper {
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .table-scroll {
    height: 100%;
    overflow: auto;
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  .virtual-row {
    display: flex;
    align-items: center;
    cursor: pointer;
    border-bottom: 1px solid var(--border-subtle, #f0f0f0);
  }

  .virtual-row:hover {
    background: var(--hover-bg, #f8f9fa);
  }

  .virtual-row.selected {
    background: var(--selected-bg, #e8f0fe);
  }

  .row-id-cell {
    width: 48px;
    min-width: 48px;
    text-align: center;
    color: var(--text-secondary, #aaa);
    font-size: 12px;
    padding: 4px 8px;
    flex-shrink: 0;
  }

  .cell {
    flex: 1;
    min-width: 0;
    padding: 4px 12px;
    font-size: 13px;
    overflow: hidden;
  }

  .cell.editing {
    padding: 1px;
  }

  .cell-value {
    display: block;
    min-height: 20px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: default;
  }

  .actions-cell {
    width: 40px;
    flex-shrink: 0;
  }
</style>
