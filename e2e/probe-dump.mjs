import { chromium } from 'playwright-core';
import { spawn } from 'node:child_process';

const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
let page = null;
for (const ctx of browser.contexts()) {
  for (const p of ctx.pages()) {
    if (p.url().includes('localhost:1420')) page = p;
  }
}
if (!page) {
  console.log('no page');
  process.exit(2);
}
page.setDefaultTimeout(20000);

const dumper = spawn(
  'powershell.exe',
  ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'e2e/dump-dialog.ps1', '-TimeoutSec', '40'],
  { cwd: process.cwd() }
);
let dumpOut = '';
dumper.stdout.on('data', (d) => (dumpOut += d.toString()));
dumper.stderr.on('data', (d) => (dumpOut += d.toString()));

const flow = page.evaluate(async () => {
  const a = await import('/src/lib/actions.ts');
  return await a.openWorkspaceFlow();
});

const result = await Promise.race([
  flow.then((r) => 'resolved:' + String(r)),
  new Promise((r) => setTimeout(() => r('flow-timeout'), 45000)),
]);
console.log('flow:', result);

await new Promise((r) => setTimeout(r, 1000));
console.log('dumper:', dumpOut.trim());
process.exit(0);
