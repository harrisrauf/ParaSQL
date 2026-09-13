// Probe: does undo after + Row work, and how long does it take?
import {
  connect, openWorkspaceDirect, openEditor, closeEditor, callFlow, getPage,
  statusInfo, pressUndo, assertNoConsoleErrors, msgsOf, sleep, disconnect, WS_DEMO,
} from './lib.mjs';

try {
  await connect({ reload: true });
  await openWorkspaceDirect(WS_DEMO);
  await openEditor('sample_users', { expectTotal: 1001 });

  const canUndo0 = await getPage().evaluate(() =>
    import('/src/lib/commands.ts').then((c) => c.canUndo())
  );
  console.log('canUndo before insert:', canUndo0);

  // 1) direct API undo after toolbar insert
  await getPage().locator('.tb-btn', { hasText: /^\+ Row$/ }).click();
  await getPage().waitForFunction(() => {
    const t = (document.querySelector('.statusbar')?.textContent ?? '').replace(/,/g, '');
    return /1,?002 of 1,?002 rows/.test(t);
  }, null, { timeout: 20000 });
  console.log('after +Row:', (await statusInfo()).raw.trim());
  console.log('canUndo after insert:', await getPage().evaluate(() =>
    import('/src/lib/commands.ts').then((c) => c.canUndo())
  ));

  let t0 = Date.now();
  const r = await callFlow('undoFlow');
  console.log('direct undoFlow returned', JSON.stringify(r), 'in', Date.now() - t0, 'ms');
  console.log('after direct undo:', (await statusInfo()).raw.trim());

  // 2) keyboard undo after insert
  await getPage().locator('.tb-btn', { hasText: /^\+ Row$/ }).click();
  await sleep(1500);
  console.log('after 2nd +Row:', (await statusInfo()).raw.trim());
  t0 = Date.now();
  await pressUndo();
  for (let i = 0; i < 12; i++) {
    await sleep(1000);
    const st = await statusInfo();
    console.log(`  t+${i + 1}s status:`, st.raw.trim());
    if (st.rowsText === '1,001 of 1,001 rows') break;
  }
  console.log('keyboard undo total ms:', Date.now() - t0);
  console.log('console errors:', JSON.stringify(assertNoConsoleErrors({ excludes: [/\[vite\]/] })));

  await closeEditor();
} catch (e) {
  console.log('PROBE ERROR:', msgsOf(e));
}
await disconnect();
