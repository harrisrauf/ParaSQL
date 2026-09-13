// Shared E2E helpers for ParaSQL (Tauri 2 + WebView2, CDP on :9222).
// Used by e2e/playwright-check.mjs and e2e/deep-tests.mjs.
import { chromium } from 'playwright-core';
import { spawn } from 'node:child_process';
import { existsSync, readFileSync, statSync, mkdirSync, copyFileSync } from 'node:fs';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

export { sleep };

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, '..');
export const SAMPLE_DIR = path.join(ROOT, 'Sample_data');
export const SALES_DIR = path.join(SAMPLE_DIR, 'sales');
export const WS_DEMO = path.join(SAMPLE_DIR, 'parasql-demo.parasql');
export const OUT = 'C:\\Users\\OMEN\\AppData\\Local\\Temp\\opencode\\e2e-out';
export const BACKUP_WS = path.join(OUT, 'parasql-demo.backup.parasql');
export const LOG_PATH = 'C:\\Users\\OMEN\\AppData\\Local\\Temp\\opencode\\tauri-e2e.log';
export const CDP_URL = 'http://127.0.0.1:9222';

const S = {
  page: null,
  browser: null,
  results: [],
  timings: [],
  consoleErrors: [],
  pageErrors: [],
  dialogs: [],
  promptQueue: [],
  helpers: new Set(),
  t0: Date.now(),
  logOffset: 0,
  dialogsHooked: false,
  ticksHooked: false,
};

// ---------------------------------------------------------------- utilities

export function getPage() {
  if (!S.page) throw new Error('not connected — call connect() first');
  return S.page;
}

export function results() {
  return S.results;
}

export function timings() {
  return S.timings;
}

export function escapeRe(s) {
  return String(s).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function cssAttrValue(s) {
  return String(s).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

export function withTimeout(promise, ms, label = 'operation') {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

export function msgsOf(err) {
  return String(err?.message ?? err).split('\n')[0].slice(0, 220);
}

// ---------------------------------------------------------------- connect

export async function connect({ reload = true } = {}) {
  const browser = await chromium.connectOverCDP(CDP_URL);
  let page = null;
  for (const ctx of browser.contexts()) {
    for (const p of ctx.pages()) {
      if (p.url().includes('localhost:1420')) page = p;
    }
  }
  if (!page) throw new Error('app page (localhost:1420) not found over CDP');
  page.setDefaultTimeout(15000);
  S.browser = browser;
  S.page = page;

  if (!S.dialogsHooked) {
    page.on('dialog', async (d) => {
      const entry = { type: d.type(), message: d.message(), at: Date.now() };
      try {
        if (d.type() === 'prompt') {
          const answer = S.promptQueue.shift();
          entry.answer = answer;
          await d.accept(answer ?? 'renamed_by_e2e');
        } else if (d.type() === 'beforeunload') {
          await d.accept();
        } else {
          await d.accept();
        }
      } catch (e) {
        entry.error = msgsOf(e);
      }
      S.dialogs.push(entry);
    });
    S.dialogsHooked = true;
  }

  page.on('console', (m) => {
    if (m.type() === 'error') S.consoleErrors.push(m.text());
  });
  page.on('pageerror', (e) => S.pageErrors.push(String(e?.message ?? e)));

  if (reload) await reloadApp({ expectWelcome: true });
  return { browser, page };
}

export async function reloadApp({ expectWelcome = false } = {}) {
  const page = getPage();
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForSelector('.app');
  if (expectWelcome) await page.waitForSelector('.welcome');
}

export function queuePrompt(text) {
  S.promptQueue.push(text);
}

export function dialogsSeen() {
  return S.dialogs.slice();
}

// ---------------------------------------------------------------- dialogs (in-app seam)
//
// DEV builds route every open/save/confirm through src/lib/dialogs.ts, which
// consumes a queue set here via page.evaluate while running under E2E. No OS
// dialogs are involved, so no PowerShell/keystroke automation is needed.

async function pushQueue(kind, items) {
  await getPage().evaluate(
    ({ kind, items }) => {
      const w = window;
      w.__pqDialogOverride ||= { files: [], confirms: [], log: [] };
      w.__pqDialogOverride[kind].push(...items);
    },
    { kind, items }
  );
}

/** Queue paths (or null for a cancel) for the next pickOpen/pickSave calls. */
export function queueFile(...paths) {
  return pushQueue('files', paths);
}

/** Queue boolean answers for the next confirmDialog calls. */
export function queueConfirm(...answers) {
  return pushQueue('confirms', answers);
}

export async function dialogQueueState() {
  return getPage().evaluate(() => {
    const o = window.__pqDialogOverride;
    return o ? { files: o.files.length, confirms: o.confirms.length, log: [...o.log] } : null;
  });
}

export async function killAllHelpers() {
  S.helpers.clear();
  try {
    await getPage().evaluate(() => {
      if (window.__pqDialogOverride) {
        window.__pqDialogOverride = { files: [], confirms: [], log: [] };
      }
    });
  } catch {
    /* page may be gone */
  }
}

/** Compatibility shim: with the seam there are no native dialogs left to escape. */
export async function escapeNativeDialogs() {
  return 'dialog seam active (no native dialogs)';
}

/** Queue the file the flow will pick, then run `flow` (which opens the picker). */
export async function withDialog(filePath, flow, { flowTimeout = 40000 } = {}) {
  await queueFile(filePath);
  const res = await withTimeout(Promise.resolve().then(flow), flowTimeout, 'flow');
  return { res, helper: { ok: true, output: `seam queued: ${filePath}` } };
}

/** Queue the file, click a UI element that opens the picker, then let the flow run. */
export async function clickWithDialog(locator, filePath) {
  await queueFile(filePath);
  await locator.click();
  return { ok: true, output: `seam queued: ${filePath}` };
}

/**
 * Queue the answer for the app's confirmDialog, then run `flow` (which triggers it).
 * Mirrors the old helper's return shape for callers.
 */
export async function withConfirm(action, flow, { flowTimeout = 30000 } = {}) {
  await queueConfirm(action === 'accept');
  const res = await withTimeout(Promise.resolve().then(flow), flowTimeout, 'confirm flow');
  return { res, helper: { ok: true, output: `seam confirm: ${action}` } };
}

// ---------------------------------------------------------------- flows / page svc

export async function callFlow(name, ...args) {
  return getPage().evaluate(
    async ({ name, args }) => {
      const a = await import('/src/lib/actions.ts');
      return await a[name](...args);
    },
    { name, args }
  );
}

/** Ensure the one-time run-tick click hook is installed on the page. */
async function ensureRunTicks() {
  if (S.ticksHooked) return;
  await getPage().evaluate(() => {
    if (window.__pqRunTicks === undefined) {
      window.__pqRunTicks = 0;
      document.addEventListener(
        'click',
        (e) => {
          if (e.target?.closest?.('.run-btn')) window.__pqRunTicks++;
        },
        true
      );
    }
  });
  S.ticksHooked = true;
}

export async function openWorkspaceDirect(wsPath) {
  return callFlow('openWorkspaceFlow', wsPath);
}

export async function openWorkspace(wsPath, opts = {}) {
  if (opts.direct) return openWorkspaceDirect(wsPath);
  const { res } = await withDialog(wsPath, () => callFlow('openWorkspaceFlow'));
  return res;
}

export async function openFileDirect(filePath) {
  return callFlow('openFileFlow', filePath);
}

export async function openFile(filePath, opts = {}) {
  if (opts.direct) return openFileDirect(filePath);
  const { res } = await withDialog(filePath, () => callFlow('openFileFlow'));
  return res;
}

export async function saveAs(path) {
  const { res } = await withDialog(path, () => callFlow('saveAsFlow'));
  return res;
}

// ---------------------------------------------------------------- SQL editor

export function normalizeText(s) {
  return String(s ?? '')
    .replace(/\u00a0/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export async function editorText() {
  return normalizeText(await getPage().locator('.cm-content').textContent());
}

/** If the Query tab isn't rendered (Data is the default view), switch to it. */
export async function ensureQueryTab() {
  const page = getPage();
  if ((await page.locator('.cm-content').count()) === 0) {
    const tab = page.locator('[role=tab]', { hasText: 'Query' }).first();
    if (await tab.count()) {
      await tab.click();
      await page.waitForSelector('.cm-content');
    }
  }
}

export async function typeSql(sql) {
  const page = getPage();
  await ensureQueryTab();
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+a');
  await page.keyboard.type(sql);
  const text = await editorText();
  if (text !== normalizeText(sql)) {
    throw new Error(`editor text mismatch: "${text.slice(0, 90)}"`);
  }
}

export async function snapshotQuery() {
  return getPage().evaluate(() => ({
    error: document.querySelector('.error-bar .error-text')?.textContent ?? null,
    info: document.querySelector('.result-info span')?.textContent ?? null,
  }));
}

export async function runQuery({ timeout = 45000 } = {}) {
  const page = getPage();
  await ensureQueryTab();
  await ensureRunTicks();
  const prev = await snapshotQuery();
  const base = await page.evaluate(() => window.__pqRunTicks ?? 0);
  await page.click('.run-btn');
  await sleep(60);
  await page.waitForFunction(
    ({ base, pe, pi }) => {
      if ((window.__pqRunTicks ?? 0) <= base) return false;
      const running = document.querySelector('.run-btn')?.disabled ?? false;
      if (running) return false;
      const err = document.querySelector('.error-bar .error-text')?.textContent ?? null;
      const info = document.querySelector('.result-info span')?.textContent ?? null;
      return err !== pe || info !== pi || true;
    },
    { base, pe: prev.error, pi: prev.info },
    { timeout }
  );
  return snapshotQuery();
}

export async function runSql(sql, opts) {
  await typeSql(sql);
  return runQuery(opts);
}

export async function historyItems() {
  await ensureQueryTab();
  return getPage().locator('.history-item').allTextContents();
}

// ---------------------------------------------------------------- grid / status

export async function statusInfo() {
  return getPage().evaluate(() => {
    const bar = document.querySelector('.statusbar');
    const text = bar?.textContent ?? '';
    const m = text.match(/([\d,]+)\s+of\s+([\d,]+)\s+rows/);
    const sort = text.match(/sorted:\s*(\S+)\s*([↑↓])/);
    return {
      raw: text,
      visibleRows: m ? Number(m[1].replace(/,/g, '')) : null,
      totalRows: m ? Number(m[2].replace(/,/g, '')) : null,
      cols: Number((text.match(/(\d+)\s+cols/) ?? [])[1] ?? 0),
      filtered: !!bar?.querySelector('.filtered-badge'),
      queryBadge: !!bar?.querySelector('.query-badge'),
      matchBadge: bar?.querySelector('.match-badge')?.textContent ?? null,
      sorted: sort ? `${sort[1]} ${sort[2]}` : null,
      modified: !!bar?.querySelector('.status-item.modified'),
    };
  });
}

export async function waitForRows(visible, total, { timeout = 20000 } = {}) {
  await getPage().waitForFunction(
    ({ v, t }) => {
      const text = (document.querySelector('.statusbar')?.textContent ?? '').replace(/,/g, '');
      const m = text.match(/(\d+) of (\d+) rows/);
      if (!m) return false;
      return (v == null || Number(m[1]) === v) && (t == null || Number(m[2]) === t);
    },
    { v: visible, t: total },
    { timeout }
  );
}

/** Wait for the visible row count only (when totalRows is intentionally not asserted). */
export async function waitForVisibleRows(visible, { timeout = 20000 } = {}) {
  await getPage().waitForFunction(
    (v) => {
      const m = (document.querySelector('.statusbar')?.textContent ?? '')
        .replace(/,/g, '')
        .match(/(\d+) of (\d+) rows/);
      return m && Number(m[1]) === v;
    },
    visible,
    { timeout }
  );
}

export async function firstRowInfo() {
  return getPage().evaluate(() => {
    const row = document.querySelector('.grid-row');
    if (!row) return null;
    const values = {};
    for (const cell of row.querySelectorAll('.cell')) {
      values[cell.getAttribute('data-col')] = cell.querySelector('.cell-value')?.textContent ?? null;
    }
    return {
      rowId: Number(row.getAttribute('data-row')),
      firstValue: row.querySelector('.cell-value')?.textContent ?? null,
      values,
    };
  });
}

export async function cellText(rowId, col) {
  return getPage().evaluate(
    ({ rid, c }) => {
      const row = document.querySelector(`.grid-row[data-row="${rid}"]`);
      if (!row) return null;
      for (const cell of row.querySelectorAll('.cell')) {
        if (cell.getAttribute('data-col') === c) {
          return cell.querySelector('.cell-value')?.textContent ?? null;
        }
      }
      return null;
    },
    { rid: rowId, c: col }
  );
}

export async function waitForCellText(rowId, col, expected, { timeout = 20000 } = {}) {
  await getPage().waitForFunction(
    ({ rid, c, e }) => {
      const row = document.querySelector(`.grid-row[data-row="${rid}"]`);
      if (!row) return false;
      for (const cell of row.querySelectorAll('.cell')) {
        if (cell.getAttribute('data-col') === c) {
          return (cell.querySelector('.cell-value')?.textContent ?? null) === e;
        }
      }
      return false;
    },
    { rid: rowId, c: col, e: expected },
    { timeout }
  );
}

export async function visibleRowTexts(col) {
  return getPage().evaluate((c) => {
    const out = [];
    for (const row of document.querySelectorAll('.grid-row')) {
      for (const cell of row.querySelectorAll('.cell')) {
        if (cell.getAttribute('data-col') === c) {
          out.push(cell.querySelector('.cell-value')?.textContent ?? null);
        }
      }
    }
    return out;
  }, col);
}

// Opens the CellEditor for (rowId, col), sets value, commits, waits for close.
export async function editCell(rowId, col, value) {
  const page = getPage();
  const cell = page.locator(
    `.grid-row[data-row="${rowId}"] .cell[data-col="${cssAttrValue(col)}"]`
  );
  await cell.dblclick();
  await page.waitForSelector('.cell-editor');
  const editor = page.locator('.cell-editor');
  const tag = await editor.evaluate((el) => el.tagName);
  if (tag === 'SELECT') {
    await editor.selectOption(String(value));
  } else {
    await editor.fill(String(value));
    await page.keyboard.press('Enter');
  }
  await page.waitForSelector('.cell-editor', { state: 'detached' });
}

export async function focusApp() {
  await getPage().locator('.statusbar').click({ position: { x: 2, y: 2 } }).catch(() => {});
}

export async function pressUndo() {
  await focusApp();
  await getPage().keyboard.press('Control+z');
}

export async function pressRedo() {
  await focusApp();
  await getPage().keyboard.press('Control+y');
}

// ---------------------------------------------------------------- sidebar / editor

export async function tableNames() {
  return getPage().evaluate(() =>
    [...document.querySelectorAll('.table-entry')].map((e) =>
      (e.querySelector('.name')?.childNodes[0]?.textContent ?? '').trim()
    )
  );
}

export async function tableEntry(name) {
  const page = getPage();
  const idx = await page.evaluate(
    (n) =>
      [...document.querySelectorAll('.table-entry')].findIndex(
        (e) => (e.querySelector('.name')?.childNodes[0]?.textContent ?? '').trim() === n
      ),
    name
  );
  if (idx < 0) throw new Error(`table entry not found: ${name}`);
  return page.locator('.table-entry').nth(idx);
}

export async function previewTable(name, { timeout = 25000, expectRows = null } = {}) {
  const page = getPage();
  const entry = await tableEntry(name);
  await entry.click();
  await page.waitForSelector('.grid .cell-value', { timeout });
  await page.waitForFunction(
    (want) => {
      const m = (document.querySelector('.statusbar')?.textContent ?? '')
        .replace(/,/g, '')
        .match(/(\d+) of (\d+) rows/);
      if (!m) return false;
      return want == null ? Number(m[1]) > 0 : Number(m[1]) === want;
    },
    expectRows,
    { timeout }
  );
}

export async function openEditor(name, { expectTotal = null, timeout = 40000 } = {}) {
  const page = getPage();
  const entry = await tableEntry(name);
  await entry.dblclick();
  await page.waitForSelector('.editor-entry', { timeout });
  await page.waitForSelector('.grid .cell-value', { timeout });
  if (expectTotal != null) await waitForRows(expectTotal, expectTotal, { timeout });
}

export async function closeEditor() {
  const page = getPage();
  await page.locator('.editor-entry .close-btn').click();
  await page.waitForSelector('.editor-entry', { state: 'detached' });
}

export async function ctxMenuOnTable(name) {
  const entry = await tableEntry(name);
  await entry.click({ button: 'right' });
  await getPage().waitForSelector('.ctx-menu');
}

export async function clickCtx(label) {
  await getPage().locator('.ctx-menu button', { hasText: label }).first().click();
}

export async function closeModal() {
  const page = getPage();
  if (await page.locator('.modal').count()) {
    await page.locator('.modal-close').click();
    await page.waitForSelector('.modal', { state: 'detached' });
  }
}

/** Open a menu bar title and return the locator of the requested dropdown item (retries). */
export async function openMenu(title, itemText) {
  const page = getPage();
  const titleBtn = page.locator('.menu-title', { hasText: title });
  const item = page.locator('.menu-dropdown .menu-item', { hasText: itemText }).first();
  for (let attempt = 0; attempt < 3; attempt++) {
    await titleBtn.click();
    if (await item.isVisible().catch(() => false)) return item;
    await page.keyboard.press('Escape');
    await sleep(150);
  }
  throw new Error(`menu item not visible: ${title} > ${itemText}`);
}

/** Click a menu bar title then one of its dropdown items, with reopen retries. */
export async function menuClick(title, itemText) {
  const item = await openMenu(title, itemText);
  await item.click();
}

// ---------------------------------------------------------------- files / robustness

export function waitForFile(filePath, { minSize = 1, timeout = 30000 } = {}) {
  return (async () => {
    const deadline = Date.now() + timeout;
    for (;;) {
      try {
        const st = statSync(filePath);
        if (st.size >= minSize) return st;
      } catch {
        /* not yet */
      }
      if (Date.now() > deadline) throw new Error(`file not written: ${filePath}`);
      await sleep(200);
    }
  })();
}

export function ensureOutDir() {
  if (!existsSync(OUT)) mkdirSync(OUT, { recursive: true });
  return OUT;
}

export function backupDemoWorkspace() {
  ensureOutDir();
  copyFileSync(WS_DEMO, BACKUP_WS);
  return BACKUP_WS;
}

export function demoWorkspaceMatchesBackup() {
  try {
    return readFileSync(WS_DEMO).equals(readFileSync(BACKUP_WS));
  } catch {
    return false;
  }
}

export function restoreDemoWorkspace() {
  copyFileSync(BACKUP_WS, WS_DEMO);
  return demoWorkspaceMatchesBackup();
}

export function markLogStart() {
  try {
    S.logOffset = readFileSync(LOG_PATH, 'utf8').length;
  } catch {
    S.logOffset = 0;
  }
}

export function logTailSinceMark() {
  try {
    const full = readFileSync(LOG_PATH, 'utf8');
    return full.length > S.logOffset ? full.slice(S.logOffset) : '';
  } catch {
    return '';
  }
}

export function unhandledRejectionsSinceMark() {
  return logTailSinceMark()
    .split(/\r?\n/)
    .filter((l) => /UNHANDLED REJECTION|panicked at/i.test(l));
}

const DEFAULT_EXCLUDES = [/\[vite\]/, /Query failed:/];

export function assertNoConsoleErrors({ excludes = DEFAULT_EXCLUDES } = {}) {
  const relevant = [...S.consoleErrors, ...S.pageErrors].filter(
    (e) => !excludes.some((re) => re.test(e))
  );
  return relevant;
}

export async function screenshot(name) {
  try {
    ensureOutDir();
    await getPage().screenshot({ path: path.join(OUT, name) });
    return path.join(OUT, name);
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------- reporting

export function check(name, ok, detail = '') {
  const trimmed = String(detail ?? '').slice(0, 300);
  S.results.push({ name, ok: !!ok, detail: trimmed, ms: Date.now() - S.t0 });
  const label = ok ? 'PASS' : 'FAIL';
  console.log(`${label}  ${name}${trimmed ? `  — ${trimmed}` : ''}`);
  return !!ok;
}

export async function step(name, fn) {
  const t0 = Date.now();
  try {
    const out = await fn();
    S.timings.push({ name, ms: Date.now() - t0, ok: true });
    check(name, true);
    return out;
  } catch (e) {
    S.timings.push({ name, ms: Date.now() - t0, ok: false, err: msgsOf(e) });
    check(name, false, msgsOf(e));
    return null;
  }
}

export function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

export function assertEq(actual, expected, label = 'value') {
  if (actual !== expected) throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}

export function summarize({ title = 'SUMMARY' } = {}) {
  const pass = S.results.filter((r) => r.ok).length;
  const fails = S.results.filter((r) => !r.ok);
  console.log(`\n===== ${title} =====`);
  console.log(`${pass} passed, ${fails.length} failed, ${S.results.length} total`);
  for (const f of fails) console.log(`  FAIL: ${f.name}${f.detail ? ` — ${f.detail}` : ''}`);
  console.log('\n----- timings (ms) -----');
  for (const t of S.timings) {
    console.log(`${t.ok ? 'ok ' : 'ERR'} ${String(t.ms).padStart(7)}  ${t.name}${t.err ? `  (${t.err})` : ''}`);
  }
  const totalMs = Date.now() - S.t0;
  console.log(`total wall time: ${(totalMs / 1000).toFixed(1)}s`);
  console.log(`console errors (filtered view): ${S.consoleErrors.length} raw / ${S.pageErrors.length} pageerrors`);
  return { pass, fail: fails.map((f) => f.name), total: S.results.length };
}

export async function disconnect() {
  killAllHelpers();
  try {
    await S.browser?.close();
  } catch {
    /* disconnect only */
  }
}
