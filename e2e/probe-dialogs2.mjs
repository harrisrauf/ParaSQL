// Probe: native dialog helper — Open-file picker and Add-Folder picker.
import { connect, openWorkspaceDirect, spawnAutoDialog, callFlow, tableNames, WS_DEMO, SALES_DIR } from './lib.mjs';
import path from 'node:path';

const TITANIC = path.join(path.dirname(WS_DEMO), 'titanic.parquet');

await connect({ reload: true });
await openWorkspaceDirect(WS_DEMO);
await new Promise((r) => setTimeout(r, 500));

// 1) Add Folder (folder picker)
{
  const h = spawnAutoDialog(SALES_DIR, { timeoutSec: 40 });
  h.proc.stdout.on('data', (d) => process.stdout.write('[folder helper] ' + d));
  h.proc.stderr.on('data', (d) => process.stdout.write('[folder helper:err] ' + d));
  let res;
  try {
    res = await Promise.race([
      callFlow('addFolderFlow'),
      new Promise((_, rej) => setTimeout(() => rej(new Error('flow timeout')), 40000)),
    ]);
    console.log('addFolderFlow result:', JSON.stringify(res));
  } catch (e) {
    console.log('addFolderFlow ERROR:', e.message);
  }
  const out = await Promise.race([h.done, new Promise((r) => setTimeout(() => r({ code: 'timeout', ok: false }), 45000))]);
  console.log('folder helper exit:', out.code, 'ok:', out.ok);
  if (out.code === 'timeout') h.kill();
  console.log('tables now:', (await tableNames()).length);
}

// 2) Open file (lite picker) — via lite mode is not needed; use addTableFlow on demo workspace
{
  const h = spawnAutoDialog(TITANIC, { timeoutSec: 40 });
  h.proc.stdout.on('data', (d) => process.stdout.write('[table helper] ' + d));
  h.proc.stderr.on('data', (d) => process.stdout.write('[table helper:err] ' + d));
  try {
    const res = await Promise.race([
      callFlow('addTableFlow'),
      new Promise((_, rej) => setTimeout(() => rej(new Error('flow timeout')), 40000)),
    ]);
    console.log('addTableFlow result:', JSON.stringify(res));
  } catch (e) {
    console.log('addTableFlow ERROR:', e.message);
  }
  const out = await Promise.race([h.done, new Promise((r) => setTimeout(() => r({ code: 'timeout', ok: false }), 45000))]);
  console.log('table helper exit:', out.code, 'ok:', out.ok);
  if (out.code === 'timeout') h.kill();
  console.log('tables now:', (await tableNames()).length);
}

const { disconnect, escapeNativeDialogs } = await import('./lib.mjs');
console.log(await escapeNativeDialogs());
await disconnect();
process.exit(0);
