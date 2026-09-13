// Probe: drive the open folder dialog with exp-folder6.ps1.
import { connect, openWorkspaceDirect, callFlow, tableNames, WS_DEMO, SALES_DIR, escapeNativeDialogs, disconnect, sleep } from './lib.mjs';
import { spawnSync } from 'node:child_process';

await connect({ reload: true });
await openWorkspaceDirect(WS_DEMO);
await sleep(500);
await escapeNativeDialogs();

const flowPromise = callFlow('addFolderFlow').catch((e) => 'ERROR: ' + e);
await sleep(2500);
const r = spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'e2e/exp-folder6.ps1', '-FilePath', SALES_DIR], { encoding: 'utf8', cwd: process.cwd(), timeout: 45000 });
console.log(r.stdout || '');
console.log('[stderr]', (r.stderr || '').slice(0, 300));
const flow = await Promise.race([flowPromise, sleep(8000).then(() => 'flow still pending')]);
console.log('flow:', JSON.stringify(flow));
console.log('tables after:', (await tableNames()).length);
console.log('leftovers:', await escapeNativeDialogs());
await disconnect();
process.exit(0);
