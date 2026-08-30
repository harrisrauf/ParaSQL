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
