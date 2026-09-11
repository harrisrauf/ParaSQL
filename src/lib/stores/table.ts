import { writable, derived } from 'svelte/store';
import type { ColumnInfo, RowData, SortConfig, SearchConfig, SortDirection, QueryResult } from '../types';
import { getPage } from '../commands';

const LOAD_CHUNK = 10_000;
const INITIAL_LOAD_THRESHOLD = 50_000;

interface SelectionState {
  anchor: { rowId: number; colIdx: number } | null;
  cells: Set<string>;
  allRows: boolean;
}

interface TableState {
  columns: ColumnInfo[];
  rows: RowData[];
  sort: SortConfig;
  search: SearchConfig;
  filters: Record<string, Set<string | null>>;
  columnWidths: Record<string, number>;
  selection: SelectionState;
  modified: boolean;
  filePath: string | null;
  totalRows: number;
  pageSize: number;
  currentPage: number;
  sqlResult: QueryResult | null;
  savedTable: { columns: ColumnInfo[]; rows: RowData[]; totalRows: number } | null;
  activeTab: 'data' | 'schema' | 'query' | 'metadata';
  loadingMore: boolean;
  searchRows: RowData[] | null;
  searchTruncated: boolean;
  focusRowId: number | null;
  /** Whether the current result set is backed by an editable table (D2) */
  editable: boolean;
}

function emptySelection(): SelectionState {
  return { anchor: null, cells: new Set(), allRows: false };
}

function createTableStore() {
  const { subscribe, set, update } = writable<TableState>({
    columns: [],
    rows: [],
    sort: { column: '', direction: null },
    search: { query: '', column: null },
    filters: {},
    columnWidths: {},
    selection: emptySelection(),
    modified: false,
    filePath: null,
    totalRows: 0,
    pageSize: 500,
    currentPage: 0,
    sqlResult: null,
    savedTable: null,
    activeTab: 'data',
    loadingMore: false,
    searchRows: null,
    searchTruncated: false,
    focusRowId: null,
    editable: true,
  });

  function toggleSort(column: string) {
    update(state => {
      let newDirection: SortDirection = 'asc';
      if (state.sort.column === column) {
        if (state.sort.direction === 'asc') newDirection = 'desc';
        else if (state.sort.direction === 'desc') newDirection = null;
      }
      return { ...state, sort: { column: newDirection ? column : '', direction: newDirection } };
    });
  }

  function setSearch(query: string, column: string | null = null) {
    update(state => {
      const cleared = query === '';
      return {
        ...state,
        search: { query, column },
        ...(cleared ? { searchRows: null, searchTruncated: false } : {}),
      };
    });
  }

  function setSqlResult(result: QueryResult | null) {
    update(state => ({ ...state, sqlResult: result }));
  }

  function applyQueryResult(result: QueryResult) {
    update(state => {
      const saved = state.sqlResult === null
        ? { columns: state.columns, rows: state.rows, totalRows: state.totalRows }
        : state.savedTable;
      return {
        ...state,
        savedTable: saved,
        columns: result.columns,
        rows: result.rows,
        totalRows: result.rows.length,
        sqlResult: result,
        filters: {},
        selection: emptySelection(),
        editable: false,
        search: { query: '', column: null },
        searchRows: null,
        searchTruncated: false,
      };
    });
  }

  function restoreTable() {
    update(state => {
      if (!state.savedTable) return { ...state, sqlResult: null, filters: {}, selection: emptySelection() };
      return {
        ...state,
        columns: state.savedTable.columns,
        rows: state.savedTable.rows,
        totalRows: state.savedTable.totalRows,
        savedTable: null,
        sqlResult: null,
        filters: {},
        selection: emptySelection(),
        editable: true,
      };
    });
  }

  function setActiveTab(tab: 'data' | 'schema' | 'query' | 'metadata') {
    update(state => ({ ...state, activeTab: tab }));
  }

  function setRows(rows: RowData[]) {
    update(state => ({ ...state, rows }));
  }

  function setTotalRows(total: number) {
    update(state => ({ ...state, totalRows: total }));
  }

  /** Fresh state for a newly opened file/folder */
  function open(columns: ColumnInfo[], rows: RowData[], totalRows: number, filePath: string | null) {
    set({
      columns,
      rows,
      sort: { column: '', direction: null },
      search: { query: '', column: null },
      filters: {},
      columnWidths: {},
      selection: emptySelection(),
      modified: false,
      filePath,
      totalRows,
      pageSize: 500,
      currentPage: 0,
      sqlResult: null,
      savedTable: null,
      activeTab: 'data',
      loadingMore: false,
      searchRows: null,
      searchTruncated: false,
      focusRowId: null,
      editable: true,
    });
  }

  function updateRow(rowId: number, newRow: RowData) {
    update(state => {
      const rows = state.rows.map(r => r.row_id === rowId ? newRow : r);
      const searchRows = state.searchRows
        ? state.searchRows.map(r => r.row_id === rowId ? newRow : r)
        : null;
      return { ...state, rows, searchRows };
    });
  }

  function removeRows(rowIds: number[]) {
    update(state => {
      const removeSet = new Set(rowIds);
      const rows = state.rows.filter(r => !removeSet.has(r.row_id));
      const searchRows = state.searchRows
        ? state.searchRows.filter(r => !removeSet.has(r.row_id))
        : null;
      const cells = new Set(
        [...state.selection.cells].filter(k => !removeSet.has(Number(k.slice(0, k.indexOf(':')))))
      );
      return {
        ...state,
        rows,
        searchRows,
        selection: { anchor: null, cells, allRows: state.selection.allRows },
        totalRows: Math.max(0, state.totalRows - removeSet.size),
      };
    });
  }

  // --- Selection (cell-based, Excel-like) ---

  function selectCell(rowId: number, colIdx: number) {
    update(state => ({
      ...state,
      selection: { anchor: { rowId, colIdx }, cells: new Set([`${rowId}:${colIdx}`]), allRows: false },
    }));
  }

  function toggleCell(rowId: number, colIdx: number) {
    update(state => {
      const key = `${rowId}:${colIdx}`;
      const cells = new Set(state.selection.cells);
      if (cells.has(key)) cells.delete(key);
      else cells.add(key);
      return { ...state, selection: { anchor: { rowId, colIdx }, cells, allRows: false } };
    });
  }

  /** Select a whole row (click on row-id cell) */
  function selectRow(rowId: number, _shift = false, _ctrl = false) {
    update(state => {
      const cells = new Set<string>();
      for (let c = 0; c < state.columns.length; c++) cells.add(`${rowId}:${c}`);
      return { ...state, selection: { anchor: { rowId, colIdx: 0 }, cells, allRows: false } };
    });
  }

  /** Rectangle selection between anchor and end, over the given (displayed) row order */
  function setRange(anchorRowId: number, anchorColIdx: number, endRowId: number, endColIdx: number, rowOrder: number[], colCount: number) {
    update(state => {
      const aIdx = rowOrder.indexOf(anchorRowId);
      const eIdx = rowOrder.indexOf(endRowId);
      if (aIdx < 0 || eIdx < 0) return state;
      const rMin = Math.min(aIdx, eIdx);
      const rMax = Math.max(aIdx, eIdx);
      const cMin = Math.min(anchorColIdx, endColIdx);
      const cMax = Math.max(anchorColIdx, endColIdx);
      const cells = new Set<string>();
      for (let i = rMin; i <= rMax; i++) {
        const rid = rowOrder[i];
        for (let c = cMin; c <= cMax; c++) cells.add(`${rid}:${c}`);
      }
      return { ...state, selection: { anchor: { rowId: anchorRowId, colIdx: anchorColIdx }, cells, allRows: false } };
    });
  }

  function selectAll() {
    update(state => {
      const base = visibleRows(state);
      const total = base.length * state.columns.length;
      if (total <= 100_000) {
        const cells = new Set<string>();
        for (const r of base) {
          for (let c = 0; c < state.columns.length; c++) cells.add(`${r.row_id}:${c}`);
        }
        return { ...state, selection: { anchor: null, cells, allRows: false } };
      }
      return { ...state, selection: { anchor: null, cells: new Set(), allRows: true } };
    });
  }

  function clearSelection() {
    update(state => ({ ...state, selection: emptySelection() }));
  }

  /** Scroll to and select a row (used after inserting a new row) */
  function focusRow(rowId: number) {
    update(state => ({ ...state, focusRowId: rowId }));
  }

  // --- Column widths ---

  function setColumnWidth(name: string, width: number) {
    update(state => ({ ...state, columnWidths: { ...state.columnWidths, [name]: width } }));
  }

  // --- Filters ---

  function toggleFilter(colName: string, value: string | null) {
    update(state => {
      const current = state.filters[colName] ?? new Set<string | null>();
      const next = new Set(current);
      if (next.has(value)) next.delete(value);
      else next.add(value);
      return { ...state, filters: { ...state.filters, [colName]: next } };
    });
  }

  function setFilter(colName: string, values: Set<string | null>) {
    update(state => ({ ...state, filters: { ...state.filters, [colName]: values } }));
  }

  function clearFilter(colName: string) {
    update(state => {
      if (!(colName in state.filters)) return state;
      const filters = { ...state.filters };
      delete filters[colName];
      return { ...state, filters };
    });
  }

  function clearFilters() {
    update(state => ({ ...state, filters: {} }));
  }

  function clearSort() {
    update(state => ({ ...state, sort: { column: '', direction: null } }));
  }

  // --- Progressive loading for large files ---

  async function loadMore() {
    const state = getCurrent();
    if (state.loadingMore) return;
    if (state.searchRows !== null) return;
    if (state.sqlResult !== null || state.rows.length >= state.totalRows) return;
    update(s => ({ ...s, loadingMore: true }));
    try {
      const last = state.rows[state.rows.length - 1];
      const page = await getPage(last ? last.row_id : null, LOAD_CHUNK);
      update(s => {
        if (page.length === 0) return { ...s, loadingMore: false };
        return { ...s, rows: [...s.rows, ...page], loadingMore: false };
      });
    } catch (err) {
      console.error('Failed to load more rows:', err);
      update(s => ({ ...s, loadingMore: false }));
    }
  }

  function getCurrent(): TableState {
    let value: TableState;
    subscribe(v => value = v)();
    return value!;
  }

  return {
    subscribe,
    set: (val: TableState) => set(val),
    update,
    open,
    toggleSort,
    setSearch,
    setSqlResult,
    applyQueryResult,
    restoreTable,
    setActiveTab,
    setRows,
    setTotalRows,
    updateRow,
    removeRows,
    selectCell,
    toggleCell,
    selectRow,
    setRange,
    selectAll,
    clearSelection,
    focusRow,
    setColumnWidth,
    toggleFilter,
    setFilter,
    clearFilter,
    clearFilters,
    clearSort,
    loadMore,
  };
}

export const tableStore = createTableStore();

export const initialLoadThreshold = INITIAL_LOAD_THRESHOLD;

/** Row ids that have at least one selected cell (or all rows when select-all is active) */
export const selectedRowIds = derived(tableStore, ($t) => {
  let memo: { sel: SelectionState; base: RowData[] | null; out: Set<number> } = {
    sel: emptySelection(),
    base: null,
    out: new Set(),
  };
  const base = $t.selection.allRows ? visibleRows($t) : null;
  if (memo.sel === $t.selection && memo.base === base) return memo.out;
  const s = new Set<number>();
  if ($t.selection.allRows) {
    for (const r of base!) s.add(r.row_id);
  } else {
    for (const key of $t.selection.cells) {
      const idx = key.indexOf(':');
      s.add(Number(key.slice(0, idx)));
    }
  }
  memo = { sel: $t.selection, base, out: s };
  return s;
});

/** True when the engine's full result set is loaded (no more pages to fetch) */
export const allLoaded = derived(
  tableStore,
  ($t) => $t.sqlResult !== null || $t.searchRows !== null || $t.rows.length >= $t.totalRows
);

/** Unique cell values for a column (for filter popovers) */
export function distinctValues(rows: RowData[], colIdx: number): (string | null)[] {
  const seen = new Set<string | null>();
  const values: (string | null)[] = [];
  for (const r of rows) {
    const v = r.values[colIdx];
    const key = v === null || v === undefined ? null : String(v);
    if (!seen.has(key)) {
      seen.add(key);
      values.push(key);
    }
  }
  return values;
}

export const displayedRows = derived(tableStore, ($table) => {
  let memo: {
    columns: ColumnInfo[] | null;
    rows: RowData[] | null;
    searchRows: RowData[] | null;
    filters: Record<string, Set<string | null>> | null;
    search: SearchConfig | null;
    sort: SortConfig | null;
    out: RowData[];
  } = { columns: null, rows: null, searchRows: null, filters: null, search: null, sort: null, out: [] };
  if (memo.columns === $table.columns && memo.rows === $table.rows && memo.searchRows === $table.searchRows && memo.filters === $table.filters && memo.search === $table.search && memo.sort === $table.sort) {
    return memo.out;
  }
  const out = visibleRows($table);
  memo = {
    columns: $table.columns,
    rows: $table.rows,
    searchRows: $table.searchRows,
    filters: $table.filters,
    search: $table.search,
    sort: $table.sort,
    out,
  };
  return out;
});

/** Apply search / filters / sort to the current row source (pure). */
export function visibleRows($table: TableState): RowData[] {
  const { columns, rows, searchRows, filters, search, sort } = $table;
  const colIdxOf = new Map(columns.map((c, i) => [c.name, i] as const));

  let out = searchRows ?? rows;

  // Column filters (AND across columns). Presence of a key means the filter
  // is active; an empty set intentionally matches nothing.
  const filterCols = columns.filter(c => filters[c.name] !== undefined);
  if (filterCols.length > 0) {
    const filterDefs = filterCols.map(c => ({ idx: colIdxOf.get(c.name) ?? -1, set: filters[c.name]! }));
    out = out.filter(r => {
      for (const f of filterDefs) {
        const v = r.values[f.idx];
        const key = v === null || v === undefined ? null : String(v);
        if (!f.set.has(key)) return false;
      }
      return true;
    });
  }

  // Global / per-column search
  if (search.query) {
    const q = search.query.toLowerCase();
    if (search.column) {
      const colIdx = colIdxOf.get(search.column) ?? -1;
      if (colIdx >= 0) {
        out = out.filter(r => {
          const val = r.values[colIdx];
          return val !== null && String(val).toLowerCase().includes(q);
        });
      }
    } else {
      out = out.filter(r =>
        r.values.some(v => v !== null && String(v).toLowerCase().includes(q))
      );
    }
  }

  if (sort.direction && sort.column) {
    const colIdx = colIdxOf.get(sort.column) ?? -1;
    if (colIdx >= 0) {
      const dir = sort.direction;
      out = [...out].sort((a, b) => {
        const va = a.values[colIdx];
        const vb = b.values[colIdx];
        if (va === null && vb === null) return 0;
        if (va === null) return 1;
        if (vb === null) return -1;
        if (typeof va === 'number' && typeof vb === 'number') {
          return dir === 'asc' ? va - vb : vb - va;
        }
        const sa = String(va).toLowerCase();
        const sb = String(vb).toLowerCase();
        if (sa < sb) return dir === 'asc' ? -1 : 1;
        if (sa > sb) return dir === 'asc' ? 1 : -1;
        return 0;
      });
    }
  }

  return out;
}