// Probe: editor-mode Save As writes a parquet? (mirrors deep-tests editor sequence)
import { rmSync, readdirSync } from 'node:fs';
import path from 'node:path';
import {
  connect, openWorkspaceDirect, openEditor, getPage, clickWithDialog, waitForFile,
  editCell, waitForCellText, firstRowInfo, statusInfo, assertNoConsoleErrors, msgsOf,
  disconnect, WS_DEMO, OUT,
} from './lib.mjs';

const target = path.join(OUT, 'probe-editor-save.parquet');
rmSync(target, { force: true });

try {
  await connect({ reload: true });
  await openWorkspaceDirect(WS_DEMO);
  await openEditor('sample_users', { expectTotal: 1001 });
  const info = await firstRowInfo();
  await editCell(info.rowId, 'username', 'probe_saved');
  await waitForCellText(info.rowId, 'username', 'probe_saved');
  console.log('status before save:', (await statusInfo()).raw.trim());

  await clickWithDialog(getPage().locator('.tb-btn', { hasText: /^Save As$/ }), target);
  const st = await waitForFile(target, { minSize: 1000, timeout: 15000 }).catch((e) => e);
  console.log('file:', st instanceof Error ? st.message : `${st.size} bytes`);
  console.log('status after save:', (await statusInfo()).raw.trim());
  await new Promise((r) => setTimeout(r, 1000));
  console.log('OUT dir entries:', JSON.stringify(readdirSync(OUT)));
  console.log('console errors raw:', JSON.stringify(assertNoConsoleErrors({ excludes: [/\[vite\]/] })));
} catch (e) {
  console.log('PROBE ERROR:', msgsOf(e));
}
await disconnect();
