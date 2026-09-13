// Probe: dump UIA tree of the Add-Folder picker while it is open.
import { connect, openWorkspaceDirect, callFlow, tableNames, WS_DEMO, SALES_DIR, escapeNativeDialogs, sleep } from './lib.mjs';
import { spawnSync } from 'node:child_process';

await connect({ reload: true });
await openWorkspaceDirect(WS_DEMO);
await sleep(500);
await escapeNativeDialogs();

// Fire addFolderFlow without a helper so the native dialog stays open while we dump it.
callFlow('addFolderFlow').catch(() => {});
await sleep(2500);
const r = spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'e2e/dump-uia.ps1'], { encoding: 'utf8', cwd: process.cwd() });
console.log(r.stdout || '');
console.log('[dump stderr]', (r.stderr || '').slice(0, 400));
console.log('escape:', await escapeNativeDialogs());
await sleep(800);
console.log('tables now:', (await tableNames()).length);
const { disconnect } = await import('./lib.mjs');
await disconnect();
process.exit(0);
