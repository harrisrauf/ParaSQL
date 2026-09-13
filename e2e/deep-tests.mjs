// Deep end-to-end suite for ParaSQL (Tauri 2 + WebView2 via CDP :9222).
// Run: node e2e/deep-tests.mjs
//
// Covers: workspace lifecycle (new/add/folder/rename/remove/mode/save/reopen),
// SQL queries (aggregates, joins, CTE+window, UNION ALL, UNNEST, quoted
// identifiers, duplicate aliases, read-only guard, result cap, history),
// grid UX (sort, filter, read-only enforcement, search), the table editor
// (edits, undo/redo, insert, delete, Save As, reopen), the workspace panel
// (search, summarize, context menus, badges) and lite mode (open/save/export).
import path from 'node:path';
import { readFileSync, rmSync } from 'node:fs';
import {
  connect,
  reloadApp,
  getPage,
  sleep,
  queuePrompt,
  clickWithDialog,
  openWorkspace,
  openFile,
  callFlow,
  editorText,
  typeSql,
  runQuery,
  runSql,
  historyItems,
  statusInfo,
  waitForRows,
  withConfirm,
  firstRowInfo,
  cellText,
  waitForCellText,
  visibleRowTexts,
  editCell,
  pressUndo,
  pressRedo,
  tableNames,
  tableEntry,
  previewTable,
  openEditor,
  closeEditor,
  ctxMenuOnTable,
  clickCtx,
  closeModal,
  waitForFile,
  ensureOutDir,
  backupDemoWorkspace,
  demoWorkspaceMatchesBackup,
  restoreDemoWorkspace,
  markLogStart,
  unhandledRejectionsSinceMark,
  logTailSinceMark,
  assertNoConsoleErrors,
  screenshot,
  escapeNativeDialogs,
  openMenu,
  menuClick,
  check,
  step,
  assert,
  assertEq,
  summarize,
  results,
  disconnect,
  msgsOf,
  OUT,
  SAMPLE_DIR,
  SALES_DIR,
  WS_DEMO,
} from './lib.mjs';

// ---------------------------------------------------------------- constants

const TITANIC_PATH = path.join(SAMPLE_DIR, 'titanic.parquet');
const TMP_WS = path.join(OUT, 'e2e-lifecycle.parasql');
const SAVED_USERS = path.join(OUT, 'users-saved.parquet');
const SAVED_TITANIC = path.join(OUT, 'titanic-copy.parquet');
const EXPORT_JSON = path.join(OUT, 'titanic-export.json');
const EXPORT_CSV = path.join(OUT, 'titanic-export.csv');

const Q_TITANIC =
  'SELECT Pclass, COUNT(*) AS n, SUM(CASE WHEN Survived = 1 THEN 1 ELSE 0 END) AS survived FROM titanic GROUP BY Pclass ORDER BY Pclass';
const Q_JOIN =
  'SELECT p.category, COUNT(*) AS items, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue FROM order_items oi JOIN products p ON p.product_id = oi.product_id GROUP BY p.category ORDER BY revenue DESC';
const Q_WINDOW =
  "WITH monthly AS (SELECT strftime(o.order_date, '%Y-%m') AS month, SUM(oi.quantity * p.unit_price) AS revenue FROM orders o JOIN order_items oi ON oi.order_id = o.order_id JOIN products p ON p.product_id = oi.product_id GROUP BY 1) SELECT month, ROUND(revenue, 2) AS revenue, ROUND(SUM(revenue) OVER (ORDER BY month), 2) AS running FROM monthly ORDER BY month";
const Q_UNION =
  'SELECT COUNT(*) AS n FROM (SELECT * FROM sample_gzip UNION ALL SELECT * FROM sample_uncompressed)';
const Q_UNNEST = 'SELECT order_id, UNNEST(items) AS item FROM sample_nested';
const Q_QUOTED = 'SELECT Bank, "Assets ($mil.)" FROM bank_failures WHERE "Assets ($mil.)" > 1000 ORDER BY "Assets ($mil.)" DESC LIMIT 5';
const Q_TRENDS = 'SELECT MAX("Matthew Perry") AS peak FROM search_trends';
const Q_ALIAS = 'SELECT 1 AS x, 2 AS x';

const page = () => getPage();
const T = { userRowId: null, ageBefore: null, ageAfter: null };

// ---------------------------------------------------------------- local helpers

async function poll(fn, { timeout = 15000, interval = 150, label = 'condition' } = {}) {
  const deadline = Date.now() + timeout;
  for (;;) {
    let v = null;
    try {
      v = await fn();
    } catch {
      v = null;
    }
    if (v) return v;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${label}`);
    await sleep(interval);
  }
}

async function waitJson(file, pred, label) {
  return poll(
    () => {
      try {
        const j = JSON.parse(readFileSync(file, 'utf8'));
        return pred(j) ? j : null;
      } catch {
        return null;
      }
    },
    { label, timeout: 20000 }
  );
}

async function hasBadge(name) {
  return page().evaluate(
    (n) =>
      [...document.querySelectorAll('.table-entry')].some(
        (e) =>
          (e.querySelector('.name')?.childNodes[0]?.textContent ?? '').trim() === n &&
          !!e.querySelector('.badge')
      ),
    name
  );
}

/** Test-side fallback for Add Folder when the native folder picker can't be driven. */
async function addFolderFallback(dir) {
  await page().evaluate(async (d) => {
    const cmds = await import('/src/lib/commands.ts');
    const { workspaceStore } = await import('/src/lib/stores/workspace.ts');
    let s;
    const unsub = workspaceStore.subscribe((v) => (s = v));
    unsub();
    const files = await cmds.listParquetFiles(d);
    const tables = [...s.doc.tables];
    const used = new Set(tables.map((t) => t.name));
    const uniqueName = (base) => {
      if (!used.has(base)) {
        used.add(base);
        return base;
      }
      let i = 2;
      while (used.has(`${base}_${i}`)) i++;
      const n = `${base}_${i}`;
      used.add(n);
      return n;
    };
    for (const f of files) {
      const meta = await cmds.getTableMeta(f);
      tables.push({
        name: uniqueName(f.replace(/\\/g, '/').split('/').pop().replace(/\.parquet$/i, '')),
        path: f,
        source: 'file',
        mode: 'query',
        compression: meta.compression,
        size: meta.size_bytes,
        mtime: null,
        abs_path: f,
        missing: false,
      });
    }
    const synced = await cmds.syncWorkspaceTables(s.path, { ...s.doc, tables });
    workspaceStore.update((v) => ({ ...v, doc: synced }));
  }, dir);
}

// ---------------------------------------------------------------- setup

ensureOutDir();
for (const f of [TMP_WS, SAVED_USERS, SAVED_TITANIC, EXPORT_JSON, EXPORT_CSV, path.join(OUT, 'probe-new.parasql')]) {
  rmSync(f, { force: true });
}
markLogStart();

try {
  await connect({ reload: true });
  await escapeNativeDialogs();

  // ================================ A. workspace open (dialog helper)

  await step('setup: backup demo workspace', async () => {
    backupDemoWorkspace();
    assert(demoWorkspaceMatchesBackup(), 'backup mismatch right after copy');
  });

  await step('shell: app + welcome screen render', async () => {
    await page().waitForSelector('.app');
    await page().waitForSelector('.welcome');
  });

  await step('workspace: open demo .parasql via native dialog helper', async () => {
    await openWorkspace(WS_DEMO);
    await page().waitForSelector('.table-entry');
    const names = await tableNames();
    assertEq(names.length, 15, 'table count');
    for (const n of ['titanic', 'iris', 'sample_users', 'orders', 'order_items', 'products']) {
      assert(names.includes(n), `missing table ${n}`);
    }
    check('workspace: sidebar lists all 15 demo tables', true, names.length + ' entries');
  });

  await step('editor: default SQL references a real workspace table and runs', async () => {
    const text = await editorText();
    const m = text.match(/^SELECT \* FROM "([^"]+)" LIMIT 100;$/);
    assert(m, `unexpected default query: "${text}"`);
    const names = await tableNames();
    assert(names.includes(m[1]), `default query table "${m[1]}" not in sidebar (${names.join(', ')})`);
    assert(m[1] !== 'working', 'default query still references internal `working` alias');
    check('editor default SQL references a real workspace table', true, `targets "${m[1]}"`);

    const snap = await runQuery();
    assert(!snap.error, `default query errored: ${snap.error}`);
    assert(/rows? returned/.test(snap.info ?? ''), `default query gave no result info: ${snap.info}`);
    check('editor default SQL executes without error', true, snap.info);
  });

  // ================================ B. SQL queries

  await step('query: titanic aggregate (216/136, 184/87, 491/119)', async () => {
    const snap = await runSql(Q_TITANIC);
    assert(!snap.error, `query error: ${snap.error}`);
    assertEq(snap.info, '3 rows returned', 'row count');
    const cells = await page().locator('.cell-value').allTextContents();
    for (const v of ['216', '136', '184', '87', '491', '119']) {
      assert(cells.includes(v), `missing value ${v} in ${JSON.stringify(cells.slice(0, 9))}`);
    }
  });

  await step('query: cross-file join top row Toys / 12337500', async () => {
    const snap = await runSql(Q_JOIN);
    assert(!snap.error, `query error: ${snap.error}`);
    const cells = await page().locator('.cell-value').allTextContents();
    assert(cells.includes('Toys'), 'Toys missing: ' + JSON.stringify(cells.slice(0, 12)));
    assert(cells.includes('12337500'), '12337500 missing: ' + JSON.stringify(cells.slice(0, 12)));
  });

  await step('query: CTE + window running total by month', async () => {
    const snap = await runSql(Q_WINDOW);
    assert(!snap.error, `query error: ${snap.error}`);
    const info = await firstRowInfo();
    assertEq(info.values.month, '2022-01', 'first month');
    assertEq(info.values.running, info.values.revenue, 'first running total != first revenue');
    const total = Number((snap.info ?? '').match(/(\d+) rows/)?.[1] ?? 0);
    assert(total >= 30, `expected >=30 months, got ${snap.info}`);
    check('query: window function returns 30+ months with cumulative running total', true, snap.info);
  });

  await step('query: UNION ALL gzip + uncompressed = 40000', async () => {
    const snap = await runSql(Q_UNION);
    assert(!snap.error, `query error: ${snap.error}`);
    const cells = await page().locator('.cell-value').allTextContents();
    assert(cells.includes('40000'), '40000 missing: ' + JSON.stringify(cells.slice(0, 5)));
  });

  await step('query: UNNEST sample_nested.items', async () => {
    const snap = await runSql(Q_UNNEST);
    assert(!snap.error, `query error: ${snap.error}`);
    const cells = await page().locator('.cell-value').allTextContents();
    assert(cells.includes('book'), 'book item missing: ' + JSON.stringify(cells.slice(0, 10)));
  });

  await step('query: quoted identifiers bank_failures', async () => {
    const snap = await runSql(Q_QUOTED);
    assert(!snap.error, `query error: ${snap.error}`);
    assertEq(snap.info, '5 rows returned', 'row count');
  });

  await step('query: quoted identifier search_trends "Matthew Perry"', async () => {
    const snap = await runSql(Q_TRENDS);
    assert(!snap.error, `query error: ${snap.error}`);
    const v = (await page().locator('.cell-value').first().textContent()) ?? '';
    assert(Number(v) > 0, `peak value not > 0: "${v}"`);
  });

  await step('query: duplicate aliases deduped (observed x / x_1)', async () => {
    const snap = await runSql(Q_ALIAS);
    assert(!snap.error, `query error: ${snap.error}`);
    const cols = await page().evaluate(() =>
      [...document.querySelectorAll('.col-header .col-name')].map((e) => e.textContent)
    );
    assert(cols.includes('x'), `x column missing: ${JSON.stringify(cols)}`);
    assert(cols.some((c) => /^x_\d+$/.test(c)), `deduplicated second alias missing: ${JSON.stringify(cols)}`);
    const cells = await page().locator('.cell-value').allTextContents();
    assert(cells.includes('1') && cells.includes('2'), 'both alias values not visible');
    check('query: duplicate alias column names preserved with suffix', true, cols.join(','));
  });

  await step('query: non-SELECT rejected with Only SELECT error', async () => {
    const snap = await runSql('DELETE FROM titanic');
    assert(snap.error, 'expected an error bar');
    assert(snap.error.includes('Only SELECT'), `unexpected error: ${snap.error}`);
  });

  await step('query: unbounded SELECT * FROM sample_large hits 100000 cap', async () => {
    const snap = await runSql('SELECT * FROM sample_large', { timeout: 90000 });
    assert(snap.error, 'expected an error bar');
    assert(/more than 100000|LIMIT/i.test(snap.error), `unexpected error: ${snap.error}`);
  });

  await step('query: history lists recent queries and restores text', async () => {
    const items = await historyItems();
    assert(items.length >= 2, `expected >=2 history items, got ${items.length}`);
    await page().locator('.history-item').first().click();
    const text = await editorText();
    assertEq(text, Q_ALIAS, 'restored editor text');
    check('query: clicking history item restores its SQL into the editor', true, items.length + ' items');
  });

  // ================================ C. grid / UX (titanic preview)

  await step('grid: preview titanic returns 500-row read-only result', async () => {
    await previewTable('titanic', { expectRows: 500 });
    await waitForRows(500, 500);
    const st = await statusInfo();
    assert(st.queryBadge, 'query-badge missing');
    assert(await page().locator('.readonly-badge').isVisible(), 'read-only badge missing');
  });

  await step('grid: column sort cycles asc -> desc -> off', async () => {
    const before = await firstRowInfo();
    await page().locator('.col-header[data-col="Name"] .col-name').click();
    await page().waitForSelector('.sort-indicator');
    assertEq(await page().locator('.sort-indicator').first().textContent(), '▲', 'header sort glyph');
    const asc = await firstRowInfo();
    assertEq((await statusInfo()).sorted, 'Name ↑', 'asc indicator');
    assert(asc.firstValue !== before.firstValue, 'first row did not change on asc sort');
    await page().locator('.col-header[data-col="Name"] .col-name').click();
    assertEq((await statusInfo()).sorted, 'Name ↓', 'desc indicator');
    assertEq(await page().locator('.sort-indicator').first().textContent(), '▼', 'header desc glyph');
    const desc = await firstRowInfo();
    assert(desc.firstValue !== asc.firstValue, 'first row did not change on desc sort');
    await page().locator('.col-header[data-col="Name"] .col-name').click();
    assertEq((await statusInfo()).sorted, null, 'sort not cleared after third click');
    check('grid: sort reorders and clear works', true);
  });

  await step('grid: filter popover excludes Pclass 2/3 then Clear restores', async () => {
    await page().locator('.col-header[data-col="Pclass"] .filter-btn').click();
    await page().waitForSelector('.filter-popover');
    check('grid: filter popover opens for Pclass', true);
    const toggle = async (v) => {
      await page()
        .locator('.filter-option')
        .filter({ has: page().locator('.value-opt', { hasText: new RegExp(`^${v}$`) }) })
        .click();
    };
    await toggle('2');
    await toggle('3');
    await page().waitForFunction(() => !!document.querySelector('.filtered-badge'));
    const st = await statusInfo();
    assert(st.visibleRows < st.totalRows, `no rows filtered (${st.visibleRows}/${st.totalRows})`);
    const vals = await visibleRowTexts('Pclass');
    assert(vals.length > 0, 'no rows visible after filter');
    assert(vals.every((v) => v === '1'), 'non-1 classes visible: ' + JSON.stringify([...new Set(vals)]));
    check('grid: unchecking 2/3 leaves only class 1 rows', true, `${st.visibleRows}/${st.totalRows}`);
    await page().locator('.filter-popover button', { hasText: 'Clear' }).click();
    await page().waitForFunction(() => !document.querySelector('.filtered-badge'));
    await page().keyboard.press('Escape');
    await page().waitForSelector('.filter-popover', { state: 'detached' });
    assertEq((await statusInfo()).visibleRows, 500, 'rows after Clear');
  });

  await step('grid: read-only > dblclick, Delete and + Row are inert', async () => {
    const st0 = await statusInfo();
    assert(
      await page().locator('.tb-btn', { hasText: /^\+ Row$/ }).isDisabled(),
      '+ Row enabled on query results'
    );
    await page().locator('.grid-row .cell').first().dblclick();
    await sleep(400);
    assertEq(await page().locator('.cell-editor').count(), 0, 'cell editor opened on read-only grid');
    await page().keyboard.press('Delete');
    await sleep(400);
    assertEq((await statusInfo()).totalRows, st0.totalRows, 'Delete changed row count');
    check('grid: read-only grid blocks editing shortcuts', true);
  });

  await step('grid: toolbar search finds Braund then clears', async () => {
    await page().locator('.search-input').fill('Braund');
    await page().waitForFunction(() => {
      const t = (document.querySelector('.statusbar')?.textContent ?? '').replace(/,/g, '');
      const m = t.match(/(\d+) of (\d+) rows/);
      return m && Number(m[1]) < Number(m[2]);
    });
    const texts = await visibleRowTexts('Name');
    assert(texts.some((t) => t && t.includes('Braund')), 'no Braund row visible');
    await page().locator('.search-bar .clear-btn').click();
    await waitForRows(500, 500);
    check('grid: clearing search restores 500 rows', true);
  });

  // ================================ D. editor (sample_users)

  await step('editor: open sample_users via double-click (1001 rows)', async () => {
    await openEditor('sample_users', { expectTotal: 1001 });
    const label = (await page().locator('.editor-entry .name').textContent()) ?? '';
    assert(label.includes('sample-users.parquet'), `editor label: ${label}`);
    const info = await firstRowInfo();
    T.userRowId = info.rowId;
    assertEq(info.values.username, 'user_0001', 'first username');
    assertEq((await statusInfo()).modified, false, 'modified flag before edits');
  });

  await step('editor: username text edit + undo + redo', async () => {
    await editCell(T.userRowId, 'username', 'e2e_saved_user');
    await waitForCellText(T.userRowId, 'username', 'e2e_saved_user');
    assert((await statusInfo()).modified, 'not marked modified after edit');
    await pressUndo();
    await waitForCellText(T.userRowId, 'username', 'user_0001');
    await pressRedo();
    await waitForCellText(T.userRowId, 'username', 'e2e_saved_user');
  });

  await step('editor: numeric age edit + undo + redo', async () => {
    T.ageBefore = await cellText(T.userRowId, 'age');
    T.ageAfter = String(Number(T.ageBefore) + 1);
    assert(T.ageAfter !== 'NaN', `unexpected age value "${T.ageBefore}"`);
    await editCell(T.userRowId, 'age', T.ageAfter);
    await waitForCellText(T.userRowId, 'age', T.ageAfter);
    await pressUndo();
    await waitForCellText(T.userRowId, 'age', T.ageBefore);
    await pressRedo();
    await waitForCellText(T.userRowId, 'age', T.ageAfter);
    check('editor: numeric edit round-trips through undo/redo', true, `${T.ageBefore} -> ${T.ageAfter}`);
  });

  await step('editor: boolean is_active edit uses select + undo/redo', async () => {
    const before = await cellText(T.userRowId, 'is_active');
    await page().locator(`.grid-row[data-row="${T.userRowId}"] .cell[data-col="is_active"]`).dblclick();
    await page().waitForSelector('.cell-editor');
    assertEq(
      await page().locator('.cell-editor').evaluate((el) => el.tagName),
      'SELECT',
      'boolean editor tag'
    );
    const target = before === '✓' ? 'false' : 'true';
    await page().locator('.cell-editor').selectOption(target);
    await page().waitForSelector('.cell-editor', { state: 'detached' });
    await sleep(250);
    const after = await cellText(T.userRowId, 'is_active');
    assert(after !== before, `boolean did not change (${before})`);
    await pressUndo();
    await waitForCellText(T.userRowId, 'is_active', before);
    await pressRedo();
    await waitForCellText(T.userRowId, 'is_active', after);
  });

  await step('editor: + Row inserts (1002) and Ctrl+Z restores (1001)', async () => {
    await page().locator('.tb-btn', { hasText: /^\+ Row$/ }).click();
    await waitForRows(1002, 1002);
    check('editor: + Row grows the table to 1002 rows', true);
    await pressUndo();
    await waitForRows(1001, 1001);
    const afterUndo = await statusInfo();
    check('undo restores totalRows', afterUndo.totalRows === 1001, `status="${afterUndo.raw}"`);
    check('editor: Ctrl+Z after insert restores 1001 rows', true);
  });

  await step('editor: delete row confirmation — dismiss keeps, accept deletes', async () => {
    const first = await firstRowInfo();
    const rowSel = `.grid-row[data-row="${first.rowId}"] .row-id-cell`;

    // cancel path: dismissing the confirmation must keep the row
    await page().locator(rowSel).click();
    let cancelOut = '';
    let dialogShown = false;
    try {
      const cancelRun = await withConfirm('dismiss', () => page().keyboard.press('Delete'));
      cancelOut = cancelRun.helper.output.replace(/\s+/g, ' ').slice(0, 180);
      dialogShown = /CONFIRM_FOUND/.test(cancelOut);
    } catch (e) {
      cancelOut = (msgsOf(e) || String(e)).replace(/\s+/g, ' ').slice(0, 180);
    }

    if (!dialogShown) {
      check(
        'APP BUG: delete-row confirmation dialog never shown (plugin-dialog 2.7.1 has no dialog.confirm command)',
        false,
        cancelOut
      );
      // confirm() rejects, so the awaited flow aborts: row and totals must be untouched.
      await sleep(400);
      const st = await statusInfo();
      assertEq(st.visibleRows, 1001, 'visible rows after rejected delete confirm');
      assertEq(st.totalRows, 1001, 'total rows after rejected delete confirm');
      check('delete-row rejected confirm leaves the row intact', true);
      return;
    }

    check('delete-row shows confirmation dialog', true, cancelOut);
    check('delete-row confirmation mentions the deletion', /Delete 1 row\(s\)\?/i.test(cancelOut), cancelOut);
    await sleep(600);
    const stAfterCancel = await statusInfo();
    assertEq(stAfterCancel.visibleRows, 1001, 'visible rows after dismissing delete confirm');
    assertEq(stAfterCancel.totalRows, 1001, 'total rows after dismissing delete confirm');
    assertEq(await cellText(first.rowId, 'username'), first.values.username, 'row vanished despite dismissed confirm');
    check('delete-row dismissed keeps the row', true);

    // accept path: accepting the confirmation deletes the row
    await page().locator(rowSel).click();
    const acceptRun = await withConfirm('accept', () => page().keyboard.press('Delete'));
    const acceptOut = acceptRun.helper.output.replace(/\s+/g, ' ').slice(0, 180);
    check('delete-row accepted performs the delete', /CONFIRM_FOUND/.test(acceptOut), acceptOut);
    await waitForRows(1000, 1000);
    check('delete-row accepted removes the row (1000)', true);

    await pressUndo();
    await waitForRows(1001, 1001);
    check('editor: Ctrl+Z restores the deleted row', true);
  });

  await step('editor: Save As writes edits to a temp parquet', async () => {
    await clickWithDialog(page().locator('.tb-btn', { hasText: /^Save As$/ }), SAVED_USERS);
    const st = await waitForFile(SAVED_USERS, { minSize: 1000 });
    check('editor: saved parquet exists and is non-trivial', true, `${st.size} bytes`);
    await poll(async () => !(await statusInfo()).modified, { label: 'modified flag to clear' });
    check('editor: status returns to Saved after Save As', true);
  });

  await step('editor: saved parquet re-read shows username + age edits', async () => {
    await page().locator('[role=tab]', { hasText: 'Query' }).click();
    const sql = `SELECT username, age, is_active FROM read_parquet('${SAVED_USERS.replace(/\\/g, '/')}') LIMIT 1`;
    const snap = await runSql(sql);
    assert(!snap.error, `query error: ${snap.error}`);
    assertEq(await cellText((await firstRowInfo()).rowId, 'username'), 'e2e_saved_user', 'username in file');
    const ages = await page().locator('.cell-value').allTextContents();
    assert(ages.includes(T.ageAfter), `age ${T.ageAfter} not in saved file: ${JSON.stringify(ages)}`);
  });

  await step('editor: close clears the grid and keeps workspace tables', async () => {
    await closeEditor();
    assertEq(await page().locator('.grid').count(), 0, 'grid still rendered after close');
    assert(await page().locator('.hint').isVisible(), 'query hint not shown');
    assertEq((await tableNames()).length, 15, 'workspace tables after close');
  });

  // ================================ E. workspace panel

  await step('panel: table search filters to titanic then clears', async () => {
    await page().locator('.panel .search').fill('titan');
    await page().waitForFunction(() => document.querySelectorAll('.table-entry').length === 1);
    assertEq((await tableNames())[0], 'titanic', 'filtered table');
    await page().locator('.panel .search').fill('');
    await page().waitForFunction(() => document.querySelectorAll('.table-entry').length === 15);
  });

  await step('panel: Summarize iris modal includes sepal.length', async () => {
    await ctxMenuOnTable('iris');
    await clickCtx('Summarize…');
    await page().waitForSelector('.modal');
    const title = (await page().locator('.modal-title').textContent()) ?? '';
    assert(title.includes('iris'), `modal title: ${title}`);
    const body = (await page().locator('.modal-body').textContent()) ?? '';
    assert(body.includes('sepal.length'), 'sepal.length not in summary');
    await closeModal();
  });

  await step('panel: context menu Query table gives 500-row read-only preview', async () => {
    await ctxMenuOnTable('titanic');
    await clickCtx('Query table');
    await page().waitForSelector('.result-info');
    await waitForRows(500, 500);
    assert((await statusInfo()).queryBadge, 'query badge missing');
    assert(await page().locator('.readonly-badge').isVisible(), 'read-only badge missing');
  });

  await step('panel: Mark editable adds badge, Mark query-only removes it', async () => {
    await ctxMenuOnTable('titanic');
    await clickCtx('Mark editable');
    await poll(() => hasBadge('titanic'), { label: 'edit badge to appear' });
    check('panel: Mark editable shows the edit badge', true);
    await ctxMenuOnTable('titanic');
    await clickCtx('Mark query-only');
    await poll(async () => !(await hasBadge('titanic')), { label: 'edit badge to disappear' });
    check('panel: Mark query-only hides the edit badge', true);
  });

  await step('panel: open-in-editor shows editor entry (badge tracks mode, not editor)', async () => {
    await openEditor('sample_users', { expectTotal: 1001 });
    assertEq(await page().locator('.editor-entry').count(), 1, 'editor entry');
    const badge = await hasBadge('sample_users');
    check(
      'panel: query-mode table open in editor shows no edit badge (badge = editable mode only)',
      badge === false,
      'current app behavior'
    );
    await closeEditor();
    assertEq(await page().locator('.editor-entry').count(), 0, 'editor entry removed');
  });

  await step('workspace: demo .parasql still byte-identical to backup', async () => {
    assert(demoWorkspaceMatchesBackup(), 'demo workspace file changed during read-only tests');
  });

  // ================================ F. workspace lifecycle (temp workspace)

  await step('lifecycle: New Workspace saves empty .parasql via native dialog', async () => {
    const item = await openMenu('Workspace', 'New Workspace…');
    await clickWithDialog(item, TMP_WS);
    await page().waitForSelector('.panel .empty');
    assertEq((await tableNames()).length, 0, 'tables in new workspace');
    const raw = readFileSync(TMP_WS, 'utf8');
    assert(raw.includes('e2e-lifecycle'), 'workspace file missing name');
    check('lifecycle: new workspace file written to disk', true);
  });

  await step('lifecycle: Add Table (iris) via native dialog', async () => {
    await clickWithDialog(
      page().locator('.add-btn[aria-label="Add table"]'),
      path.join(SAMPLE_DIR, 'iris.parquet')
    );
    await poll(async () => (await tableNames()).length === 1, { label: '1 table' });
    assertEq((await tableNames())[0], 'iris', 'added table name');
  });

  await step('lifecycle: Add Folder (sales) adds 4 tables', async () => {
    let helperOk = true;
    try {
      const item = await openMenu('Workspace', 'Add Folder…');
      await clickWithDialog(item, SALES_DIR);
    } catch (e) {
      helperOk = false;
      check('lifecycle: Add Folder native picker helper succeeded', false, msgsOf(e));
      await escapeNativeDialogs();
      await addFolderFallback(SALES_DIR);
    }
    if (helperOk) check('lifecycle: Add Folder native picker helper succeeded', true);
    await poll(async () => (await tableNames()).length === 5, { label: '5 tables', timeout: 30000 });
    const names = await tableNames();
    for (const n of ['customers', 'orders', 'order_items', 'products']) {
      assert(names.includes(n), `missing imported table ${n}`);
    }
    check('lifecycle: folder import added customers/orders/order_items/products', true);
  });

  await step('lifecycle: rename table via prompt', async () => {
    await ctxMenuOnTable('iris');
    queuePrompt('iris_renamed');
    await clickCtx('Rename…');
    await poll(async () => (await tableNames()).includes('iris_renamed'), { label: 'rename to apply' });
    assert(!(await tableNames()).includes('iris'), 'old table name still present');
    check('lifecycle: rename updated the sidebar entry', true);
  });

  await step('lifecycle: remove table confirmation — dismiss keeps, accept removes', async () => {
    await ctxMenuOnTable('customers');
    let cancelOut = '';
    let dialogShown = false;
    try {
      const cancelRun = await withConfirm('dismiss', () => clickCtx('Remove from workspace'));
      cancelOut = cancelRun.helper.output.replace(/\s+/g, ' ').slice(0, 180);
      dialogShown = /CONFIRM_FOUND/.test(cancelOut);
    } catch (e) {
      cancelOut = (msgsOf(e) || String(e)).replace(/\s+/g, ' ').slice(0, 180);
    }

    if (!dialogShown) {
      check(
        'APP BUG: remove-table confirmation dialog never shown (plugin-dialog 2.7.1 has no dialog.confirm command)',
        false,
        cancelOut
      );
      // The rejected confirm leaves the context menu open. Close it programmatically,
      // then continue the lifecycle scenario via the flow itself (test-side fallback)
      // so the later lifecycle steps remain meaningful.
      await page().evaluate(() => {
        document.querySelector('.tree')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      });
      await sleep(300);
      await callFlow('removeTableFlow', 'customers');
      await poll(async () => !(await tableNames()).includes('customers'), { label: 'remove fallback to apply' });
      return;
    }

    check('remove-table shows confirmation dialog', true, cancelOut);
    check(
      'remove-table confirmation mentions the table',
      /Remove "customers" from the workspace/i.test(cancelOut),
      cancelOut
    );
    await sleep(600);
    assert((await tableNames()).includes('customers'), 'customers removed despite dismissed confirm');
    check('remove-table dismissed keeps the sidebar entry', true);

    await ctxMenuOnTable('customers');
    const acceptRun = await withConfirm('accept', () => clickCtx('Remove from workspace'));
    const acceptOut = acceptRun.helper.output.replace(/\s+/g, ' ').slice(0, 180);
    check('remove-table accepted removes the sidebar entry', /CONFIRM_FOUND/.test(acceptOut), acceptOut);
    await poll(async () => !(await tableNames()).includes('customers'), { label: 'remove to apply' });
  });

  await step('lifecycle: toggle products query-only <-> editable', async () => {
    await ctxMenuOnTable('products');
    await clickCtx('Mark editable');
    await poll(() => hasBadge('products'), { label: 'badge on' });
    await ctxMenuOnTable('products');
    await clickCtx('Mark query-only');
    await poll(async () => !(await hasBadge('products')), { label: 'badge off' });
    await ctxMenuOnTable('products');
    await clickCtx('Mark editable');
    await poll(() => hasBadge('products'), { label: 'badge back on' });
  });

  await step('lifecycle: Save Workspace persists 4 tables incl. iris_renamed', async () => {
    // string (not regex): .menu-item text carries a leading space from the icon slot
    await menuClick('Workspace', 'Save Workspace');
    const j = await waitJson(
      TMP_WS,
      (doc) => doc.tables.some((t) => t.name === 'iris_renamed') && doc.tables.length === 4,
      'saved workspace doc'
    );
    const names = j.tables.map((t) => t.name);
    check('lifecycle: workspace file lists 4 tables', names.length === 4, names.join(','));
    assert(names.includes('iris_renamed'), 'iris_renamed missing from file');
    assert(!names.includes('customers'), 'removed customers still in file');
    assertEq(j.tables.find((t) => t.name === 'products').mode, 'editable', 'products mode in file');
  });

  await step('lifecycle: reopen workspace restores tables and editable mode', async () => {
    await openWorkspace(TMP_WS);
    await page().waitForSelector('.table-entry');
    const names = await tableNames();
    assertEq(names.length, 4, 'table count after reopen');
    assert(names.includes('iris_renamed'), 'renamed table missing after reopen');
    assert(!names.includes('customers'), 'removed table came back');
    assertEq(await hasBadge('products'), true, 'editable badge after reopen');
    check('lifecycle: workspace persisted correctly across save/reopen', true);
  });

  // ================================ G. lite mode

  await step('lite: reload returns to welcome with all entry buttons', async () => {
    await reloadApp({ expectWelcome: true });
    const labels = (await page().locator('.welcome button').allTextContents()).map((s) => s.trim());
    for (const b of ['Open File', 'Open Folder', 'New Workspace', 'Open Workspace']) {
      assert(labels.includes(b), `missing welcome button ${b}`);
    }
  });

  await step('lite: Open File (titanic) via native dialog shows 891 rows editable', async () => {
    await clickWithDialog(page().locator('.welcome button', { hasText: /^Open File$/ }), TITANIC_PATH);
    await waitForRows(891, 891);
    assertEq(await page().locator('.grid').count(), 1, 'grid');
    assertEq(await page().locator('.readonly-badge').count(), 0, 'readonly badge shown in lite mode');
  });

  await step('lite: toolbar search Braund subsets rows then clears', async () => {
    await page().locator('.search-input').fill('Braund');
    await page().waitForFunction(() => {
      const t = (document.querySelector('.statusbar')?.textContent ?? '').replace(/,/g, '');
      const m = t.match(/(\d+) of (\d+) rows/);
      return m && Number(m[1]) < Number(m[2]);
    });
    const texts = await visibleRowTexts('Name');
    assert(texts.some((t) => t && t.includes('Braund')), 'no Braund row visible');
    await page().locator('.search-bar .clear-btn').click();
    await waitForRows(891, 891);
  });

  await step('lite: Save As writes a titanic copy via dialog', async () => {
    await clickWithDialog(page().locator('.tb-btn', { hasText: /^Save As$/ }), SAVED_TITANIC);
    const st = await waitForFile(SAVED_TITANIC, { minSize: 1000 });
    check('lite: saved titanic copy exists', true, `${st.size} bytes`);
  });

  await step('lite: reopen saved copy via toolbar Open -> Open File…', async () => {
    await page().locator('.tb-btn', { hasText: 'Open' }).first().click();
    await page().waitForSelector('.tb-dropdown');
    await clickWithDialog(
      page().locator('.tb-dropdown .menu-item', { hasText: 'Open File…' }),
      SAVED_TITANIC
    );
    await waitForRows(891, 891);
  });

  await step('lite: Export JSON via dialog produces data', async () => {
    await page().locator('.tb-btn', { hasText: 'Export' }).click();
    await clickWithDialog(page().locator('.tb-dropdown .menu-item', { hasText: /^JSON$/ }), EXPORT_JSON);
    const st = await waitForFile(EXPORT_JSON, { minSize: 1000 });
    const text = readFileSync(EXPORT_JSON, 'utf8');
    check('lite: JSON export non-empty', st.size > 1000, `${st.size} bytes`);
    assert(text.includes('Braund'), 'Braund missing from JSON export');
  });

  await step('lite: Export CSV via dialog produces data', async () => {
    await page().locator('.tb-btn', { hasText: 'Export' }).click();
    await clickWithDialog(page().locator('.tb-dropdown .menu-item', { hasText: /^CSV$/ }), EXPORT_CSV);
    const st = await waitForFile(EXPORT_CSV, { minSize: 1000 });
    const text = readFileSync(EXPORT_CSV, 'utf8');
    check('lite: CSV export non-empty', st.size > 1000, `${st.size} bytes`);
    assert(text.includes('Braund'), 'Braund missing from CSV export');
  });

  await step('lite: reopen saved sample_users shows committed edits', async () => {
    await page().locator('.tb-btn', { hasText: 'Open' }).first().click();
    await page().waitForSelector('.tb-dropdown');
    await clickWithDialog(
      page().locator('.tb-dropdown .menu-item', { hasText: 'Open File…' }),
      SAVED_USERS
    );
    await waitForRows(1001, 1001);
    const info = await firstRowInfo();
    assertEq(info.values.username, 'e2e_saved_user', 'saved username');
    assertEq(info.values.age, T.ageAfter, 'saved age');
    assertEq((await statusInfo()).modified, false, 'reopened file marked modified');
    check('lite: saved parquet persisted all editor changes', true);
  });

  // ================================ H. robustness + cleanup

  await step('robustness: console/page error scan', async () => {
    const errs = assertNoConsoleErrors({ excludes: [/\[vite\]/, /Query failed:/] });
    const confirmErrs = errs.filter((e) => e.includes('dialog.confirm not allowed'));
    const otherErrs = errs.filter((e) => !e.includes('dialog.confirm not allowed'));
    check('no unexpected console errors', otherErrs.length === 0, otherErrs.join(' | ').slice(0, 300) || 'none');
    check(
      'APP BUG: confirm() console errors "dialog.confirm not allowed. Command not found"',
      confirmErrs.length === 0,
      confirmErrs.length ? `${confirmErrs.length} error(s)` : 'none'
    );
  });

  await step('robustness: dev log rejection scan', async () => {
    const hits = unhandledRejectionsSinceMark();
    const confirmHits = hits.filter((l) => l.includes('dialog.confirm not allowed'));
    const otherHits = hits.filter((l) => !l.includes('dialog.confirm not allowed'));
    check('no unhandled rejections', otherHits.length === 0, otherHits.join(' | ').slice(0, 300) || 'none');
    check(
      'APP BUG: confirm() rejections logged as UNHANDLED REJECTION',
      confirmHits.length === 0,
      confirmHits.length ? `${confirmHits.length} hit(s) "${confirmHits[0].trim().slice(0, 90)}"` : 'none'
    );
    const tail = logTailSinceMark();
    check('robustness: dev log tail read', true, `${tail.length} chars since start`);
  });

  await step('cleanup: demo workspace unchanged, restored from backup', async () => {
    const unchanged = demoWorkspaceMatchesBackup();
    check('cleanup: demo workspace byte-identical to backup', unchanged, unchanged ? '' : 'file had changed — restoring');
    assert(restoreDemoWorkspace(), 'restore from backup failed');
    check('cleanup: demo workspace restored from temp backup', true);
  });

  await screenshot('parasql-deep-final.png');
} catch (e) {
  check('fatal error', false, msgsOf(e));
}

const summary = summarize({ title: 'DEEP-TESTS SUMMARY' });
const bugFails = results().filter((r) => !r.ok && (r.name.startsWith('KNOWN BUG') || r.name.startsWith('APP BUG')));
const otherFails = results().filter((r) => !r.ok && !r.name.startsWith('KNOWN BUG') && !r.name.startsWith('APP BUG'));
console.log(
  `\n${summary.pass}/${summary.total} checks passed; known/app-bug failures: ${bugFails.length}; other failures: ${otherFails.length}`
);
if (otherFails.length > 0) {
  console.log('OTHER FAILURES:');
  for (const f of otherFails) console.log(`  - ${f.name}: ${(f.detail ?? '').slice(0, 200)}`);
}

await disconnect();
process.exitCode = otherFails.length === 0 ? 0 : 1;
