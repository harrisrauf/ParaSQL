export interface ColumnInfo {
  name: string;
  dtype: string;
  nullable: boolean;
}

export interface RowData {
  row_id: number;
  values: (string | number | boolean | null)[];
}

export interface QueryResult {
  columns: ColumnInfo[];
  rows: RowData[];
}

export interface SearchResult {
  rows: RowData[];
  truncated: boolean;
}

export interface MetadataJson {
  columns: ColumnInfo[];
  total_rows: number;
  file_path: string | null;
}

export type SortDirection = 'asc' | 'desc' | null;

export interface SortConfig {
  column: string;
  direction: SortDirection;
}

export interface SearchConfig {
  query: string;
  column: string | null;
}

// --- Workspace (multi-file "database") ---

export interface WorkspaceTable {
  name: string;
  path: string;
  source: 'file' | 'folder';
  mode: 'query' | 'editable';
  compression: string | null;
  size: number | null;
  mtime: number | null;
  abs_path: string | null;
  missing: boolean;
}

export interface SavedQuery {
  name: string;
  sql: string;
}

export interface Workspace {
  version: number;
  name: string;
  tables: WorkspaceTable[];
  saved_queries: SavedQuery[];
  charts: unknown[];
  dashboards: unknown[];
  notebooks: unknown[];
  edit_size_limit_mb: number | null;
  dir: string | null;
}

export interface TableMeta {
  path: string;
  rows: number;
  row_groups: number;
  compression: string;
  size_bytes: number;
}

export interface TableSummary {
  table: string;
  columns: { name: string | null; type: string | null; null: unknown }[];
  stats: Record<string, unknown>[];
  sample: RowData[];
}
