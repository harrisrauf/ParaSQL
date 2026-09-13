// Focused probe: why do + Row / Delete row / confirm behave oddly in the editor?
import {
  connect, getPage, openWorkspaceDirect, openEditor, waitForRows, statusInfo,
  disconnect, msgsOf, sleep, tableEntry,
} from './lib.mjs';
import { WS_DEMO } from './lib.mjs';

const storeState = () =>
  getPage().evaluate(async () => {
    const { tableStore } = await import('/src/lib/stores/table.ts');
    let s;
    const unsub = tableStore.subscribe((v) => (s = v));
    unsub();
    return {
      rows: s.rows.length,
      totalRows: s.totalRows,
      editable: s.editable,
      modified: s.modified,
      columns: s.columns.length,
      sqlResult: s.sqlResult !== null,
    };
  });

try {
  await connect({ reload: true });
  await openWorkspaceDirect(WS_DEMO);
  await getPage().waitForSelector('.table-entry');
  await openEditor('sample_users', { expectTotal: 1001 });

  console.log('before +Row:', JSON.stringify(await storeState()));

  await getPage().locator('.tb-btn', { hasText: /^\+ Row$/ }).click();
  await sleep(2500);
  console.log('after +Row status:', (await statusInfo()).raw);
  console.log('after +Row store:', JSON.stringify(await storeState()));
  const btn = getPage().locator('.tb-btn', { hasText: /^\+ Row$/ });
  console.log('+Row disabled:', await btn.isDisabled());

  // Direct call check
  const direct = await getPage().evaluate(async () => {
    const a = await import('/src/lib/actions.ts');
    try {
      await a.insertRowFlow();
      return 'ok';
    } catch (e) {
      return 'threw: ' + String(e);
    }
  });
  await sleep(2000);
  console.log('after direct insertRowFlow:', direct, (await statusInfo()).raw);

  // Delete row path
  const rowId = (await getPage().evaluate(() => Number(document.querySelector('.grid-row')?.getAttribute('data-row'))));
  await getPage().locator(`.grid-row[data-row="${rowId}"] .row-id-cell`).click();
  await sleep(200);
  const sel = await getPage().evaluate(async () => {
    const { selectedRowIds } = await import('/src/lib/stores/table.ts');
    let v;
    const unsub = selectedRowIds.subscribe((x) => (v = [...x]));
    unsub();
    return v;
  });
  console.log('selected row ids:', JSON.stringify(sel));
  await getPage().keyboard.press('Delete');
  await sleep(2500);
  console.log('after Delete status:', (await statusInfo()).raw);
  console.log('after Delete store:', JSON.stringify(await storeState()));

  const confirmIsh = await getPage().evaluate(async () => {
    const c = window.confirm;
    return { src: String(c).slice(0, 160), native: /\[native code\]/.test(String(c)) };
  });
  console.log('window.confirm:', JSON.stringify(confirmIsh));

  console.log('console errors:', JSON.stringify(await import('./lib.mjs').then((m) => m.assertNoConsoleErrors({ excludes: [/\[vite\]/] }))));
} catch (e) {
  console.log('probe-editor error:', msgsOf(e));
}
await disconnect();
