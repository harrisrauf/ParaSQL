// Reproduce the exact deep-tests editor sequence to find the +Row/Delete timeouts.
import {
  connect, getPage, openWorkspaceDirect, openEditor, waitForRows, statusInfo,
  editCell, cellText, waitForCellText, pressUndo, pressRedo, disconnect, msgsOf, sleep,
} from './lib.mjs';
import { WS_DEMO } from './lib.mjs';

const state = () =>
  getPage().evaluate(async () => {
    const { tableStore } = await import('/src/lib/stores/table.ts');
    let s;
    const unsub = tableStore.subscribe((v) => (s = v));
    unsub();
    return { rows: s.rows.length, totalRows: s.totalRows, editable: s.editable, modified: s.modified };
  });

try {
  await connect({ reload: true });
  await openWorkspaceDirect(WS_DEMO);
  await getPage().waitForSelector('.table-entry');
  await openEditor('sample_users', { expectTotal: 1001 });
  const rowId = (await state(), (await getPage().evaluate(() => Number(document.querySelector('.grid-row')?.getAttribute('data-row')))));

  console.log('start:', (await statusInfo()).raw, JSON.stringify(await state()));

  // username edit + undo + redo
  await editCell(rowId, 'username', 'e2e_saved_user');
  await waitForCellText(rowId, 'username', 'e2e_saved_user');
  await pressUndo();
  await waitForCellText(rowId, 'username', 'user_0001');
  await pressRedo();
  await waitForCellText(rowId, 'username', 'e2e_saved_user');
  console.log('after username:', (await statusInfo()).raw, JSON.stringify(await state()));

  // age edit + undo + redo
  const ageBefore = await cellText(rowId, 'age');
  const ageAfter = String(Number(ageBefore) + 1);
  await editCell(rowId, 'age', ageAfter);
  await waitForCellText(rowId, 'age', ageAfter);
  await pressUndo();
  await waitForCellText(rowId, 'age', ageBefore);
  await pressRedo();
  await waitForCellText(rowId, 'age', ageAfter);
  console.log('after age:', (await statusInfo()).raw, JSON.stringify(await state()));

  // boolean select + undo + redo
  const b0 = await cellText(rowId, 'is_active');
  await getPage().locator(`.grid-row[data-row="${rowId}"] .cell[data-col="is_active"]`).dblclick();
  await getPage().waitForSelector('.cell-editor');
  await getPage().locator('.cell-editor').selectOption(b0 === '✓' ? 'false' : 'true');
  await getPage().waitForSelector('.cell-editor', { state: 'detached' });
  await sleep(250);
  const b1 = await cellText(rowId, 'is_active');
  console.log('boolean', b0, '->', b1);
  await pressUndo();
  await waitForCellText(rowId, 'is_active', b0);
  await pressRedo();
  await waitForCellText(rowId, 'is_active', b1);
  console.log('after boolean:', (await statusInfo()).raw, JSON.stringify(await state()));

  // + Row
  await getPage().locator('.tb-btn', { hasText: /^\+ Row$/ }).click();
  try {
    await waitForRows(1002, 1002, { timeout: 10000 });
    console.log('+Row OK:', (await statusInfo()).raw);
  } catch (e) {
    console.log('+Row FAILED:', msgsOf(e), (await statusInfo()).raw, JSON.stringify(await state()));
    const btn = getPage().locator('.tb-btn', { hasText: /^\+ Row$/ });
    console.log('+Row disabled:', await btn.isDisabled());
    console.log('console errors:', JSON.stringify(await import('./lib.mjs').then((m) => m.assertNoConsoleErrors({ excludes: [/\[vite\]/] }))));
  }
} catch (e) {
  console.log('probe-deepseq error:', msgsOf(e));
}
await disconnect();
