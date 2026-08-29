<script lang="ts">
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import CellEditor from './CellEditor.svelte';
  import FilterPopover from './FilterPopover.svelte';
  import { tableStore, displayedRows, selectedRowIds, allLoaded } from '../stores/table';
  import { settings } from '../stores/settings';
  import { editCell } from '../commands';
  import type { ColumnInfo, RowData } from '../types';

  const ROW_ID_W = 56;
  const MIN_COL_W = 60;
  const MAX_COL_W = 800;

  let scrollEl: HTMLElement | null = $state(null);
  let containerEl: HTMLElement | null = $state(null);
  let editingCell: { rowId: number; colIdx: number } | null = $state(null);
  let filterAnchor: { colName: string; x: number; y: number } | null = $state(null);

  let columns = $derived($tableStore.columns);
  let rows = $derived($displayedRows);
  let widths = $derived($tableStore.columnWidths);
  let filters = $derived($tableStore.filters);
  let sort = $derived($tableStore.sort);
  let sel = $derived($tableStore.selection);
  let selected = $derived($selectedRowIds);
  let loaded = $derived($allLoaded);
  let density = $derived($settings.rowDensity);
  let rowHeight = $derived(density === 'compact' ? 26 : 36);

  let rowIndexMap = $derived(new Map($tableStore.rows.map((r, i) => [r.row_id, i])));

  let effectiveWidths = $derived(
    Object.fromEntries(columns.map(c => [c.name, widths[c.name] ?? guessWidth(c, rows)]))
  );
  let gridWidth = $derived(
    columns.reduce((acc, c) => acc + (effectiveWidths[c.name] ?? 120), 0) + ROW_ID_W
  );

  // Created once — never recreated — so scroll position survives store updates
  const vizerStore = createVirtualizer({
    count: 0,
    getScrollElement: () => scrollEl,
    estimateSize: () => rowHeight,
    overscan: 20,
  });

  let virtualItems = $derived($vizerStore.getVirtualItems());
  let totalSize = $derived($vizerStore.getTotalSize());

  // Keep the virtualizer's options in sync without recreating it
  $effect(() => {
    $vizerStore.setOptions({ count: rows.length, estimateSize: () => rowHeight, overscan: 20 });
    $vizerStore.measure();
  });

  // Progressive loading for large files: fetch the next page when the user
  // approaches the end of the loaded rows.
  $effect(() => {
    if (loaded) return;
    const items = virtualItems;
    const last = items[items.length - 1];
    if (last && last.index >= $tableStore.rows.length - 20) {
      tableStore.loadMore();
    }
  });

  // Seed column widths when new columns appear
  $effect(() => {
    if (!columns.length) return;
    const missing = columns.filter(c => widths[c.name] === undefined);
    for (const col of missing) {
      tableStore.setColumnWidth(col.name, guessWidth(col, rows));
    }
  });

  function guessWidth(col: ColumnInfo, sampleRows: RowData[]): number {
    const idx = columns.findIndex(c => c.name === col.name);
    const dtype = col.dtype.toLowerCase();
    if (dtype.includes('bool')) return 70;
    if (
      dtype.includes('int') || dtype.includes('float') || dtype.includes('double') ||
      dtype.includes('decimal') || dtype.includes('date') || dtype.includes('timestamp')
    ) {
      return 120;
    }
    let maxLen = Math.max(col.name.length, 8);
    for (let i = 0; i < Math.min(200, sampleRows.length); i++) {
      const v = sampleRows[i].values[idx];
      if (v !== null && v !== undefined) maxLen = Math.max(maxLen, String(v).length);
    }
    return Math.min(300, Math.max(80, maxLen * 7.5 + 24));
  }

  function autoFit(col: ColumnInfo) {
    tableStore.setColumnWidth(col.name, guessWidth(col, rows));
  }

  // --- Column resize ---

  let resizing: { name: string; startX: number; startW: number } | null = null;

  function startResize(e: PointerEvent, name: string) {
    e.preventDefault();
    e.stopPropagation();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    resizing = { name, startX: e.clientX, startW: widths[name] ?? 120 };
  }

  function onResizeMove(e: PointerEvent) {
    if (!resizing) return;
    const w = Math.min(MAX_COL_W, Math.max(MIN_COL_W, resizing.startW + (e.clientX - resizing.startX)));
    tableStore.setColumnWidth(resizing.name, w);
  }

  function endResize() {
    resizing = null;
  }

  // --- Selection ---

  function handleCellClick(row: RowData, colIdx: number, e: MouseEvent) {
    containerEl?.focus();
    if (e.shiftKey) {
      const anchor = sel.anchor;
      if (anchor) {
        tableStore.setRange(anchor.rowId, anchor.colIdx, row.row_id, colIdx, rows.map(r => r.row_id), columns.length);
      } else {
        tableStore.selectCell(row.row_id, colIdx);
      }
    } else if (e.ctrlKey || e.metaKey) {
      tableStore.toggleCell(row.row_id, colIdx);
    } else {
      tableStore.selectCell(row.row_id, colIdx);
    }
  }

  function handleRowIdClick(row: RowData, e: MouseEvent) {
    containerEl?.focus();
    if (e.shiftKey) {
      const anchor = sel.anchor;
      if (anchor) {
        tableStore.setRange(anchor.rowId, anchor.colIdx, row.row_id, columns.length - 1, rows.map(r => r.row_id), columns.length);
        return;
      }
    }
    tableStore.selectRow(row.row_id);
  }

  // --- Editing ---

  function handleEdit(row: RowData, colIdx: number) {
    editingCell = { rowId: row.row_id, colIdx };
  }

  async function handleSave(rowId: number, colIdx: number, value: string | number | boolean | null) {
    try {
      const result = await editCell(rowId, colIdx, value);
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

  // --- Keyboard navigation ---

  function handleKeydown(e: KeyboardEvent) {
    if (editingCell) {
      if (e.key === 'Escape') {
        editingCell = null;
        e.preventDefault();
      }
      return;
    }

    const anchor = sel.anchor;
    if (!anchor) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowRight' || e.key === 'Tab' || e.key === 'Enter') {
        if (rows.length > 0 && columns.length > 0) {
          e.preventDefault();
          tableStore.selectCell(rows[0].row_id, 0);
        }
      }
      return;
    }

    const rowIdx = rows.findIndex(r => r.row_id === anchor.rowId);
    const colIdx = anchor.colIdx;
    if (rowIdx < 0) return;

    let nr = rowIdx;
    let nc = colIdx;
    let handled = true;

    switch (e.key) {
      case 'ArrowDown': nr = Math.min(rows.length - 1, rowIdx + 1); break;
      case 'ArrowUp': nr = Math.max(0, rowIdx - 1); break;
      case 'ArrowRight': nc = Math.min(columns.length - 1, colIdx + 1); break;
      case 'ArrowLeft': nc = Math.max(0, colIdx - 1); break;
      case 'Enter':
      case 'F2':
        if (rows[rowIdx] && columns[colIdx]) {
          editingCell = { rowId: rows[rowIdx].row_id, colIdx };
          e.preventDefault();
        }
        return;
      case 'Tab':
        nc = Math.min(columns.length - 1, colIdx + 1);
        e.preventDefault();
        break;
      case 'Escape':
        tableStore.clearSelection();
        break;
      default:
        handled = false;
    }

    if (!handled) return;
    e.preventDefault();
    const target = rows[nr];
    if (!target) return;

    if (e.shiftKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
      tableStore.setRange(rows[rowIdx].row_id, colIdx, target.row_id, nc, rows.map(r => r.row_id), columns.length);
    } else {
      tableStore.selectCell(target.row_id, nc);
      $vizerStore.scrollToIndex(nr, { align: 'auto' });
    }
  }

  // --- Filter popover ---

  function openFilter(e: MouseEvent, name: string) {
    e.stopPropagation();
    if (filterAnchor?.colName === name) {
      filterAnchor = null;
      return;
    }
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    filterAnchor = { colName: name, x: rect.left, y: rect.bottom + 4 };
  }

  // --- Cell display helpers ---

  function formatCell(value: string | number | boolean | null): string {
    if (value === null || value === undefined) return '—';
    if (typeof value === 'boolean') return value ? '✓' : '✗';
    if (typeof value === 'string' && value.startsWith('[') && value.includes('bytes:')) {
      return value;
    }
    return String(value);
  }

  function isTruncated(value: string | number | boolean | null): boolean {
    if (value === null || value === undefined) return false;
    return String(value).length > 50;
  }
</script>

<div class="grid" bind:this={containerEl} tabindex="0" role="grid" aria-label="Data grid" onkeydown={handleKeydown}>
  <div class="grid-scroll" bind:this={scrollEl}>
    <div class="grid-inner" style="width: {gridWidth}px; min-width: 100%;">
      <div class="grid-header">
        <div class="row-id-col" style="width: {ROW_ID_W}px;">#</div>
        {#each columns as col (col.name)}
          <div
            class="col-header"
            style="width: {effectiveWidths[col.name]}px;"
            onclick={() => {
              filterAnchor = null;
              tableStore.toggleSort(col.name);
            }}
            role="button"
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && tableStore.toggleSort(col.name)}
          >
            <span class="col-name">{col.name}</span>
            <span class="col-type">{col.dtype}</span>
            {#if sort.column === col.name}
              <span class="sort-indicator">{sort.direction === 'asc' ? '▲' : '▼'}</span>
            {/if}
            <button
              class="filter-btn"
              class:active={filters[col.name]?.size > 0}
              onclick={(e) => openFilter(e, col.name)}
              title="Filter"
              tabindex="-1"
            >▾</button>
            <div
              class="resize-handle"
              role="separator"
              aria-orientation="vertical"
              onpointerdown={(e) => startResize(e, col.name)}
              onpointermove={onResizeMove}
              onpointerup={endResize}
              ondblclick={(e) => {
                e.stopPropagation();
                autoFit(col);
              }}
            ></div>
          </div>
        {/each}
      </div>
      <div class="grid-body" style="height: {totalSize}px;">
        {#each virtualItems as item (item.key)}
          {@const row = rows[item.index]}
          {@const isRowSelected = sel.allRows || selected.has(row.row_id)}
          <div
            class="grid-row"
            class:selected={isRowSelected}
            style="top: {item.start}px; height: {item.size}px;"
            role="row"
            data-row={row.row_id}
          >
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              class="row-id-cell"
              style="width: {ROW_ID_W}px;"
              role="button"
              tabindex="-1"
              onclick={(e) => handleRowIdClick(row, e)}
              title={isRowSelected ? 'Selected' : ''}
            >{(rowIndexMap.get(row.row_id) ?? 0) + 1}</div>
            {#each columns as col, colIdx (col.name)}
              {@const key = `${row.row_id}:${colIdx}`}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                class="cell"
                class:selected={sel.cells.has(key)}
                class:editing={editingCell?.rowId === row.row_id && editingCell?.colIdx === colIdx}
                style="width: {effectiveWidths[col.name]}px;"
                data-col={col.name}
                onclick={(e) => handleCellClick(row, colIdx, e)}
                ondblclick={() => handleEdit(row, colIdx)}
                role="gridcell"
                tabindex="-1"
              >
                {#if editingCell?.rowId === row.row_id && editingCell?.colIdx === colIdx}
                  <CellEditor
                    value={row.values[colIdx] ?? null}
                    dtype={col.dtype}
                    onSave={(val) => handleSave(row.row_id, colIdx, val)}
                    onCancel={() => (editingCell = null)}
                  />
                {:else}
                  <span
                    class="cell-value"
                    title={isTruncated(row.values[colIdx]) ? String(row.values[colIdx]) : ''}
                  >{formatCell(row.values[colIdx])}</span>
                {/if}
              </div>
            {/each}
          </div>
        {/each}
        {#if !loaded}
          <div class="loading-row" style="top: {totalSize}px; height: {rowHeight}px;">
            Loading more rows…
          </div>
        {/if}
      </div>
    </div>
  </div>

  {#if filterAnchor}
    <FilterPopover
      colName={filterAnchor.colName}
      x={filterAnchor.x}
      y={filterAnchor.y}
      onClose={() => (filterAnchor = null)}
    />
  {/if}
</div>

<style>
  .grid {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
    min-width: 0;
    outline: none;
  }

  .grid-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .grid-inner {
    position: relative;
  }

  .grid-header {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    background: var(--panel-bg, #fafafa);
    border-bottom: 1px solid var(--border-color, #e0e0e0);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
  }

  .col-header {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    border-right: 1px solid var(--border-subtle, #f0f0f0);
    box-sizing: border-box;
    overflow: hidden;
  }

  .col-header:hover {
    background: var(--hover-bg, #f5f5f5);
  }

  .col-name {
    font-weight: 600;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .col-type {
    font-size: 10px;
    color: var(--text-secondary, #999);
    font-weight: 400;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sort-indicator {
    font-size: 9px;
    color: var(--accent-color, #1a73e8);
  }

  .filter-btn {
    margin-left: auto;
    border: none;
    background: none;
    cursor: pointer;
    font-size: 10px;
    color: var(--text-secondary, #999);
    padding: 0 2px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .filter-btn:hover {
    background: var(--hover-bg, #e0e0e0);
  }

  .filter-btn.active {
    color: var(--accent-color, #1a73e8);
  }

  .resize-handle {
    position: absolute;
    top: 0;
    right: -3px;
    width: 7px;
    height: 100%;
    cursor: col-resize;
    z-index: 2;
    touch-action: none;
  }

  .resize-handle:hover {
    background: var(--accent-color, #1a73e8);
    opacity: 0.5;
  }

  .grid-body {
    position: relative;
    min-width: 100%;
  }

  .grid-row {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--border-subtle, #f0f0f0);
    box-sizing: border-box;
  }

  .grid-row:hover {
    background: var(--hover-bg, #f8f9fa);
  }

  .grid-row.selected {
    background: var(--selected-bg, #e8f0fe);
  }

  .row-id-col,
  .row-id-cell {
    width: 56px;
    min-width: 56px;
    text-align: center;
    color: var(--text-secondary, #aaa);
    font-size: 11px;
    padding: 4px 4px;
    flex-shrink: 0;
    box-sizing: border-box;
    border-right: 1px solid var(--border-subtle, #f0f0f0);
    background: var(--panel-bg, #fafafa);
    user-select: none;
  }

  .row-id-cell {
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-id-cell:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .cell {
    flex-shrink: 0;
    min-width: 0;
    padding: 4px 10px;
    font-size: 13px;
    overflow: hidden;
    box-sizing: border-box;
    border-right: 1px solid var(--border-subtle, #f0f0f0);
  }

  .cell.selected {
    outline: 2px solid var(--accent-color, #1a73e8);
    outline-offset: -2px;
    background: var(--selected-bg, #e8f0fe);
  }

  .cell.editing {
    padding: 1px;
  }

  .cell-value {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: default;
    font-variant-numeric: tabular-nums;
  }

  .loading-row {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary, #999);
    font-size: 12px;
    background: var(--panel-bg, #fafafa);
  }

  :global(.grid.compact) .grid-row,
  :global(.grid.compact) .cell {
    font-size: 12px;
  }
</style>