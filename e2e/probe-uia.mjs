// Debug: run auto-dialog helper with live stdout while triggering editor Save As directly.
import { spawn } from 'node:child_process';
import path from 'node:path';
import {
  connect, openWorkspaceDirect, openEditor, getPage, callFlow, escapeNativeDialogs,
  getPage as page, disconnect, WS_DEMO, OUT, ROOT, sleep,
} from './lib.mjs';
import { rmSync } from 'node:fs';

const target = path.join(OUT, 'probe-uia.parquet');
rmSync(target, { force: true });

try {
  await connect({ reload: false });
  await escapeNativeDialogs();
  await openWorkspaceDirect(WS_DEMO);
  await openEditor('sample_users', { expectTotal: 1001 });

  const proc = spawn(
    'powershell.exe',
    ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(ROOT, 'e2e', 'auto-dialog.ps1'), '-FilePath', target, '-TimeoutSec', '60'],
    { cwd: ROOT, windowsHide: true }
  );
  proc.stdout.on('data', (d) => console.log('[helper]', d.toString().trim()));
  proc.stderr.on('data', (d) => console.log('[helper-err]', d.toString().trim()));
  const done = new Promise((r) => proc.on('close', (code) => r(code)));

  await sleep(2500);
  console.log('triggering saveAsFlow...');
  const flow = callFlow('saveAsFlow').catch((e) => 'FLOW ERROR: ' + e.message);
  const code = await Promise.race([done, sleep(70000).then(() => 'probe-timeout')]);
  console.log('helper exit:', code);
  console.log('flow:', await flow);
  console.log('file exists:', (await import('node:fs')).existsSync(target));
  await sleep(500);
  console.log('escaped leftovers:', await escapeNativeDialogs());
} catch (e) {
  console.log('PROBE ERROR:', e && e.message);
}
await disconnect();
