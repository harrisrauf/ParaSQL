# ParaSQL E2E harness

Playwright drives the running Tauri app over CDP (WebView2 remote debugging on `:9222`).

## How it works

- `start-dev.cmd` launches `npm run tauri dev` with `src-tauri/tauri.e2e.conf.json`, which
  enables `--remote-debugging-port=9222` and uses a dedicated WebView2 profile.
- `lib.mjs` connects with `chromium.connectOverCDP('http://127.0.0.1:9222')`, finds the
  `localhost:1420` page and exposes helpers (SQL editor, grid, sidebar, menus, files).
- File pickers and confirms are never OS dialogs in tests: in DEV builds
  `src/lib/dialogs.ts` consumes a queue installed by the tests via `page.evaluate`
  (`window.__pqDialogOverride`). Tests still click the real UI — only the picker is stubbed.
  Release builds compile the seam out (`import.meta.env.DEV`), so production is untouched.

## Run

1. `e2e\start-dev.cmd` — leave running; first build takes a few minutes.
2. `node e2e\playwright-check.mjs` — smoke suite, ~5 s.
3. `node e2e\deep-tests.mjs` — full suite, ~30 s.

Exit code is non-zero when any non-labelled check fails. Checks whose names start with
`APP BUG:` or `KNOWN BUG:` are diagnostic probes and are excluded from the exit code.

## Files

- `lib.mjs` — shared helpers: connection, dialog seam, SQL, grid, sidebar, reporting.
- `playwright-check.mjs` — fast smoke suite (12 checks).
- `deep-tests.mjs` — full suite (107 checks): queries, grid, editor, panel, lifecycle,
  exports, Lite mode, robustness.
- `start-dev.cmd` — dev launcher used by the suites (log: `.e2e-dev.log`).
- `screenshot.mjs` — captures `docs/assets/screenshot-query.png` for the README.

## Notes

- `Sample_data/parasql-demo.parasql` is backed up and restored by the deep suite.
- The WebView2 profile in `src-tauri/tauri.e2e.conf.json` is created at
  `src-tauri/target/debug/e2e-profile` (relative path), so no machine-specific
  configuration is needed. Delete that folder if the profile gets wedged.
- npm shortcuts: `npm run dev:e2e`, `npm run e2e:smoke`, `npm run e2e:deep`.
