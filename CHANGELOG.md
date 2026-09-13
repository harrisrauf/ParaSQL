# Changelog

All notable changes to ParaSQL are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned

- Chart builder and pivot tables (ECharts)
- Dashboards and notebooks
- AI copilot (bring your own model)
- Cross-platform release builds

## [0.2.0] - 2026-09-13

The first public release of ParaSQL — a full Parquet workbench, not just a viewer.

### Added

- **SQL workbench** powered by a bundled DuckDB engine: cross-file joins, CTEs,
  window functions, `UNNEST`, quoted identifiers, query history, and
  table-aware autocomplete.
- **Spreadsheet-grade data grid**: virtualized rows (500k+), sort, per-column
  filters, full-table search, inline editing with type awareness, row
  insert/delete, and keyboard navigation.
- **Workspace mode**: add individual Parquet files or scan folders; every file
  becomes a queryable table. Workspaces are stored as portable `.parasql` files
  with relative paths and can be re-opened on any machine.
- **Editable tables**: open a workspace table for editing (with a size guard,
  default 2 GB) or edit a single Parquet file directly; export the result back
  to Parquet with SNAPPY, ZSTD, GZIP, LZ4, BROTLI, or no compression.
- **Import / export**: JSON, CSV, Excel (XLSX), and Parquet export for both
  tables and arbitrary query results; table summaries via DuckDB `SUMMARIZE`.
- **Undo / redo** with exact round-trips, including BLOBs, microsecond
  timestamps, arrays, structs, and maps.
- **E2E test harness** driven by Playwright over the WebView2 CDP endpoint,
  with a DEV-only dialog seam (107 deep checks + 12 smoke checks).
- Application branding: ParaSQL identity, icons, and a portable demo workspace
  included in `Sample_data/`.

### Security & robustness

- Atomic file writes for every export and workspace save (temp file + rename).
- Read-only enforcement for query results across the UI and all shortcuts.
- Strict Content-Security-Policy; no network access, no telemetry.
- Transactional editor table swaps and safe SQL guardrails
  (SELECT/WITH only, result caps, poison-tolerant locking).

### Changed

- Upgraded the bundled DuckDB engine to 1.5.5, fixing a parser-error
  connection-poisoning bug that could take the whole app down.

### Fixed

- Destructive confirmations now use the Tauri dialog plugin (native
  `window.confirm` is no longer overridden incorrectly).
- Undo totals, editor save-point tracking, folder import with heterogeneous
  schemas, and query previews after errors.
