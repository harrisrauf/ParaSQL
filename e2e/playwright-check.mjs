// E2E smoke check for ParaSQL against the real Tauri app via WebView2 CDP (port 9222).
// Launch: npm run tauri dev -- --config src-tauri/tauri.e2e.conf.json
// Run:    node e2e/playwright-check.mjs
//
// The run is green when every check except the clearly-labelled KNOWN BUG passes.
import {
  connect,
  openWorkspace,
  WS_DEMO,
  check,
  step,
  results,
  typeSql,
  runQuery,
  editorText,
  openEditor,
  pressUndo,
  pressRedo,
  summarize,
  screenshot,
  assertNoConsoleErrors,
  disconnect,
  getPage,
  assert,
  assertEq,
} from './lib.mjs';

const globalTimer = setTimeout(() => {
  console.log('GLOBAL TIMEOUT — aborting');
  process.exit(3);
}, 180000);

try {
  await connect({ reload: true });

  await step('app shell rendered', async () => {
    await getPage().waitForSelector('.app');
  });

  await step('open demo workspace (tables in sidebar)', async () => {
    await openWorkspace(WS_DEMO);
    await getPage().waitForSelector('.table-entry');
    const n = await getPage().locator('.table-entry').count();
    assert(n >= 15, `only ${n} tables`);
    console.log(`      (sidebar has ${n} tables)`);
  });

  await step('titanic preview + read-only badge', async () => {
    await getPage().locator('.table-entry', { hasText: 'titanic' }).first().click();
    await getPage().waitForSelector('.grid .cell-value');
    const braund = await getPage().locator('.cell-value', { hasText: 'Braund' }).count();
    assert(braund > 0, 'Braund row not visible');
    assert(await getPage().locator('.readonly-badge').isVisible(), 'no read-only badge');
  });

  await step('menu dropdown survives pointer travel', async () => {
    const page = getPage();
    const title = page.locator('.menu-title', { hasText: 'Workspace' });
    const box = await title.boundingBox();
    assert(box, 'menu title not found');
    await title.click();
    await page.waitForSelector('.menu-dropdown .menu-item');
    // Jiggle over the title itself — this used to close the dropdown instantly.
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.waitForTimeout(120);
    const items = page.locator('.menu-dropdown .menu-item');
    assert((await items.count()) > 0, 'dropdown closed while hovering the menu bar');
    const itemBox = await items.first().boundingBox();
    assert(itemBox, 'no dropdown items');
    await page.mouse.move(itemBox.x + itemBox.width / 2, itemBox.y + itemBox.height / 2);
    await page.waitForTimeout(80);
    assert(await items.first().isVisible(), 'dropdown closed while travelling to an item');
    await page.keyboard.press('Escape');
  });

  await step('editor default SQL references a real workspace table', async () => {
    await getPage().locator('[role=tab]', { hasText: 'Query' }).click();
    const text = await editorText();
    const m = text.match(/^SELECT \* FROM "([^"]+)" LIMIT 100;$/);
    assert(m, `unexpected default query: "${text}"`);
    const names = await getPage().evaluate(() =>
      [...document.querySelectorAll('.table-entry')].map(
        (e) => (e.querySelector('.name')?.childNodes[0]?.textContent ?? '').trim()
      )
    );
    assert(names.includes(m[1]), `default query table "${m[1]}" not in sidebar (${names.join(', ')})`);
    assert(m[1] !== 'working', 'default query references internal `working` alias');
    const snap = await runQuery();
    assert(!snap.error, `default query errored: ${snap.error}`);
    assert(/rows? returned/.test(snap.info ?? ''), `default query gave no result info: ${snap.info}`);
  });

  await step('titanic aggregate via Query tab editor', async () => {
    await typeSql(
      'SELECT Pclass, COUNT(*) AS n, SUM(CASE WHEN Survived = 1 THEN 1 ELSE 0 END) AS survived FROM titanic GROUP BY Pclass ORDER BY Pclass'
    );
    const snap = await runQuery();
    assert(!snap.error, `query error: ${snap.error}`);
    await getPage().waitForFunction(() =>
      [...document.querySelectorAll('.cell-value')].some((el) => el.textContent === '216')
    );
    const cells = await getPage().locator('.cell-value').allTextContents();
    assert(cells.includes('491'), 'missing 491: ' + JSON.stringify(cells.slice(0, 9)));
  });

  await step('cross-file join (order_items x products)', async () => {
    await typeSql(
      'SELECT p.category, COUNT(*) AS items, ROUND(SUM(oi.quantity * p.unit_price), 2) AS revenue FROM order_items oi JOIN products p ON p.product_id = oi.product_id GROUP BY p.category ORDER BY revenue DESC'
    );
    const snap = await runQuery();
    assert(!snap.error, `query error: ${snap.error}`);
    await getPage().waitForFunction(() =>
      [...document.querySelectorAll('.cell-value')].some((el) => el.textContent === 'Toys')
    );
    const cells = await getPage().locator('.cell-value').allTextContents();
    assert(cells.includes('12337500'), 'unexpected revenue: ' + JSON.stringify(cells.slice(0, 12)));
  });

  let firstName = null;
  await step('open sample_users for editing (double-click)', async () => {
    await openEditor('sample_users', { expectTotal: 1001 });
    const info = await getPage().evaluate(() => {
      const row = document.querySelector('.grid-row');
      const cell = [...(row?.querySelectorAll('.cell') ?? [])].find(
        (c) => c.getAttribute('data-col') === 'username'
      );
      return { rowId: Number(row?.getAttribute('data-row')), value: cell?.querySelector('.cell-value')?.textContent };
    });
    firstName = { id: info.rowId, value: info.value };
    assertEq(firstName.value, 'user_0001', 'first username');
  });

  await step('edit cell + undo + redo', async () => {
    const { id } = firstName;
    const page = getPage();
    const cellSel = `.grid-row[data-row="${id}"] .cell[data-col="username"] .cell-value`;
    await page.locator(`.grid-row[data-row="${id}"] .cell[data-col="username"]`).dblclick();
    await page.waitForSelector('.cell-editor');
    await page.locator('.cell-editor').fill('e2e_edited');
    await page.keyboard.press('Enter');
    await page.waitForFunction(
      (s) => document.querySelector(s)?.textContent === 'e2e_edited',
      cellSel
    );
    await pressUndo();
    await page.waitForFunction((s) => document.querySelector(s)?.textContent === 'user_0001', cellSel);
    await pressRedo();
    await page.waitForFunction((s) => document.querySelector(s)?.textContent === 'e2e_edited', cellSel);
    await pressUndo();
    await page.waitForFunction((s) => document.querySelector(s)?.textContent === 'user_0001', cellSel);
  });

  await step('charts tab placeholder', async () => {
    await getPage().locator('[role=tab]', { hasText: 'Charts' }).click();
    await getPage().waitForSelector('.placeholder-title');
    assertEq(await getPage().locator('.placeholder-title').textContent(), 'Charts', 'placeholder title');
  });

  await step('close editor cleanly', async () => {
    await getPage().locator('.editor-entry .close-btn').click();
    await getPage().waitForSelector('.editor-entry', { state: 'detached' });
  });

  {
    const relevant = assertNoConsoleErrors();
    check(
      'no console/page errors during run (excluding expected Query failed + vite)',
      relevant.length === 0,
      relevant.join(' | ').slice(0, 300)
    );
  }

  await screenshot('parasql-e2e.png');
} catch (e) {
  check('fatal error', false, String(e?.message ?? e).slice(0, 300));
}

clearTimeout(globalTimer);

const summary = summarize({ title: 'PLAYWRIGHT-CHECK SUMMARY' });
const knownBugFails = results().filter((r) => !r.ok && r.name.startsWith('KNOWN BUG'));
const otherFails = results().filter((r) => !r.ok && !r.name.startsWith('KNOWN BUG'));
console.log(
  `\n${summary.pass}/${summary.total} checks passed; known-bug failures: ${knownBugFails.length}; other failures: ${otherFails.length}`
);

await disconnect();
process.exitCode = otherFails.length === 0 ? 0 : 1;
