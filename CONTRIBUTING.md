# Contributing to ParaSQL

Thanks for wanting to make ParaSQL better. This guide gets you from clone to
green checks as fast as possible.

## Ways to contribute

- **Bugs** — open an issue with the bug template (include your OS and the exact
  steps).
- **Features** — open a feature request first so we can agree on scope before
  you write code.
- **Docs** — README, `docs/`, examples, screenshots, translations.
- **Code** — look for `good first issue` labels, or the roadmap in the README.
- **Spread the word** — a ⭐ helps more analysts and engineers find the project.

## Development setup

### Prerequisites

| Tool | Version | Notes |
| ---- | ------- | ----- |
| Node.js | 20+ | frontend tooling |
| Rust | stable (1.77+) | `rustup` recommended |
| Tauri CLI | via npm (`@tauri-apps/cli`) | installed by `npm install` |
| WebView2 | Windows 10/11 | preinstalled on modern Windows; [download](https://developer.microsoft.com/microsoft-edge/webview2/) otherwise |

On Linux you also need the usual Tauri system packages
(`webkit2gtk-4.1`, `libappindicator3`, `librsvg2-dev`, `patchelf`, …). See the
[Tauri prerequisites](https://tauri.app/start/prerequisites/).

### Run it

```bash
git clone https://github.com/harrisrauf/ParaSQL.git
cd ParaSQL
npm install
npm run tauri dev
```

First Rust build compiles a bundled DuckDB, so expect a few minutes. Later
builds are incremental.

### Checks

```bash
npm run check                 # svelte-check (TypeScript + Svelte)
npm run build                 # production frontend build
cd src-tauri && cargo test --lib   # Rust unit tests
```

All three run in CI on every push and PR.

## Project layout

```
src/                     Svelte 5 frontend (runes)
  lib/actions.ts         user flows (open/save/export/edit/workspace)
  lib/commands.ts        typed Tauri IPC wrappers
  lib/dialogs.ts         DEV-only dialog seam (scripted dialogs for tests)
  lib/stores/            rune-based stores (table, workspace, settings, ui)
  lib/components/        DataGrid, SqlEditor, MenuBar, WorkspacePanel, …
src-tauri/src/
  commands.rs            #[tauri::command] layer (async, spawn_blocking)
  engine/duckdb.rs       DuckDB engine: open, query, edit, undo, export
  engine/catalog.rs      .parasql workspace model (portable paths)
  engine/engine_test.rs  Rust unit tests
Sample_data/             demo workspace (sales fixtures + parasql-demo.parasql)
```

### How the layers talk

```
Svelte UI  →  commands.ts  →  Tauri IPC (capability-scoped)
           →  commands.rs  →  spawn_blocking  →  DuckDB engine
           →  Parquet / Arrow
```

Rules of thumb:

- **All SQL identifiers go through `quote_ident`.** Never interpolate raw
  identifiers into SQL text.
- **File writes are atomic** (`atomic_replace`: temp file + rename). Don't
  write directly to a user path.
- **Editor operations push inverse statements or snapshot tables** for undo;
  keep them reversible.
- **User-facing errors are plain strings** returned as `Err(String)` and shown
  as toasts — no panics, no silent failures.

## Adding a command

1. Implement the logic in `src-tauri/src/engine/duckdb.rs` (with a unit test in
   `engine_test.rs`).
2. Add a thin `#[tauri::command]` wrapper in `commands.rs` and register it in
   `lib.rs`.
3. Add a typed wrapper in `src/lib/commands.ts` (camelCase argument names —
   Tauri v2 convention).
4. Wire it into an action in `src/lib/actions.ts` and the UI.
5. Run the checks above.

## Code style

- **Rust**: `cargo fmt`, small functions, descriptive error strings, tests for
  every behavioral fix.
- **TypeScript/Svelte**: Svelte 5 runes (`$state`, `$derived`, `$props`),
  strict types, no new runtime dependencies without discussion.
- **Comments**: explain *why*, not *what*. Keep them rare.
- **Commits**: imperative, scoped, e.g.
  `Fix undo totals after row insert` or `Add Excel overflow guard`.

## Pull request checklist

- [ ] `npm run check` passes
- [ ] `npm run build` passes
- [ ] `cargo test --lib` passes
- [ ] New behavior is covered by a Rust unit test
- [ ] No telemetry, no network calls, no new capabilities without discussion
- [ ] README/docs updated if user-visible behavior changed

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
