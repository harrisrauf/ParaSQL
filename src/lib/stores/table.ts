import { writable, derived } from 'svelte/store';
import type { ColumnInfo, RowData, SortConfig, SearchConfig, SortDirection, QueryResult } from '../types';

interface TableState {
  columns: ColumnInfo[];
  rows: RowData[];
  selectedRowIds: Set<number>;
  sort: SortConfig;
  search: SearchConfig;
  modified: boolean;
  filePath: string | null;
  totalRows: number;
  pageSize: number;
  currentPage: number;
  sqlResult: QueryResult | null;
  savedTable: { columns: ColumnInfo[]; rows: RowData[]; totalRows: number } | null;
  activeTab: 'data' | 'schema' | 'query' | 'metadata';
}

function createTableStore() {
  const { subscribe, set, update } = writable<TableState>({
    columns: [],
    rows: [],
    selectedRowIds: new Set(),
    sort: { column: '', direction: null },
    search: { query: '', column: null },
    modified: false,
    filePath: null,
    totalRows: 0,
    pageSize: 500,
    currentPage: 0,
    sqlResult: null,
    savedTable: null,
    activeTab: 'data',
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
    update(state => ({ ...state, search: { query, column } }));
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
      };
    });
  }

  function restoreTable() {
    update(state => {
      if (!state.savedTable) return { ...state, sqlResult: null };
      return {
        ...state,
        columns: state.savedTable.columns,
        rows: state.savedTable.rows,
        totalRows: state.savedTable.totalRows,
        savedTable: null,
        sqlResult: null,
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

  function selectRow(rowId: number, shift = false, ctrl = false) {
    update(state => {
      const newSet = new Set(state.selectedRowIds);
      if (ctrl) {
        if (newSet.has(rowId)) newSet.delete(rowId);
        else newSet.add(rowId);
      } else {
        newSet.clear();
        newSet.add(rowId);
      }
      return { ...state, selectedRowIds: newSet };
    });
  }

  function clearSelection() {
    update(state => ({ ...state, selectedRowIds: new Set() }));
  }

  function updateRow(rowId: number, newRow: RowData) {
    update(state => {
      const rows = state.rows.map(r => r.row_id === rowId ? newRow : r);
      return { ...state, rows };
    });
  }

  function removeRows(rowIds: number[]) {
    update(state => {
      const removeSet = new Set(rowIds);
      const rows = state.rows.filter(r => !removeSet.has(r.row_id));
      return { ...state, rows };
    });
  }

  return {
    subscribe,
    set: (val: TableState) => set(val),
    update,
    toggleSort,
    setSearch,
    setSqlResult,
    applyQueryResult,
    restoreTable,
    setActiveTab,
    setRows,
    setTotalRows,
    selectRow,
    clearSelection,
    updateRow,
    removeRows,
  };
}

export const tableStore = createTableStore();

export const displayedRows = derived(tableStore, ($table) => {
  let rows = [...$table.rows];

  if ($table.search.query) {
    const q = $table.search.query.toLowerCase();
    if ($table.search.column) {
      const colIdx = $table.columns.findIndex(c => c.name === $table.search.column);
      if (colIdx >= 0) {
        rows = rows.filter(r => {
          const val = r.values[colIdx];
          return val !== null && String(val).toLowerCase().includes(q);
        });
      }
    } else {
      rows = rows.filter(r =>
        r.values.some(v => v !== null && String(v).toLowerCase().includes(q))
      );
    }
  }

  if ($table.sort.direction && $table.sort.column) {
    const colIdx = $table.columns.findIndex(c => c.name === $table.sort.column);
    if (colIdx >= 0) {
      rows.sort((a, b) => {
        const va = a.values[colIdx];
        const vb = b.values[colIdx];
        if (va === null && vb === null) return 0;
        if (va === null) return 1;
        if (vb === null) return -1;
        if (typeof va === 'number' && typeof vb === 'number') {
          return $table.sort.direction === 'asc' ? va - vb : vb - va;
        }
        const sa = String(va).toLowerCase();
        const sb = String(vb).toLowerCase();
        if (sa < sb) return $table.sort.direction === 'asc' ? -1 : 1;
        if (sa > sb) return $table.sort.direction === 'asc' ? 1 : -1;
        return 0;
      });
    }
  }

  return rows;
});
