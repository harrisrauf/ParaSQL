// Probe: folder picker via auto-dialog helper only.
import { connect, openWorkspaceDirect, spawnAutoDialog, callFlow, tableNames, WS_DEMO, SALES_DIR, escapeNativeDialogs, disconnect, sleep } from './lib.mjs';

await connect({ reload: true });
await openWorkspaceDirect(WS_DEMO);
await sleep(500);
await escapeNativeDialogs();
console.log('tables before:', (await tableNames()).length);

const h = spawnAutoDialog(SALES_DIR, { timeoutSec: 40 });
h.proc.stdout.on('data', (d) => process.stdout.write('[helper] ' + d));
h.proc.stderr.on('data', (d) => process.stdout.write('[helper:err] ' + d));
try {
  const res = await Promise.race([
    callFlow('addFolderFlow'),
    new Promise((_, rej) => setTimeout(() => rej(new Error('flow timeout')), 40000)),
  ]);
  console.log('addFolderFlow result:', JSON.stringify(res));
} catch (e) {
  console.log('addFolderFlow ERROR:', e.message);
}
const out = await Promise.race([h.done, new Promise((r) => setTimeout(() => r({ code: 'timeout', ok: false }), 45000))]);
console.log('helper exit:', out.code, 'ok:', out.ok);
if (out.code === 'timeout') h.kill();
console.log('tables after:', (await tableNames()).length);
console.log('leftovers:', await escapeNativeDialogs());
await disconnect();
process.exit(0);
