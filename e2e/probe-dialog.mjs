import { chromium } from 'playwright-core';
import { spawn } from 'node:child_process';

const WS = 'D:\\.projects\\paraquet Viewer\\Sample_data\\parasql-demo.parasql';

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
page.setDefaultTimeout(30000);

const helper = spawn(
  'powershell.exe',
  ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'e2e/auto-dialog.ps1', '-FilePath', WS, '-TimeoutSec', '30'],
  { cwd: process.cwd() }
);
let helperOut = '';
helper.stdout.on('data', (d) => (helperOut += d.toString()));
helper.stderr.on('data', (d) => (helperOut += d.toString()));

const flow = page.evaluate(async () => {
  const a = await import('/src/lib/actions.ts');
  return await a.openWorkspaceFlow();
});

const result = await Promise.race([
  flow.then((r) => 'resolved:' + String(r)),
  new Promise((r) => setTimeout(() => r('flow-timeout'), 40000)),
]);
console.log('flow:', result);
const tables = await page.locator('.table-entry').count().catch(() => -1);
console.log('tables:', tables);
await new Promise((r) => setTimeout(r, 1200));
console.log('helper:', helperOut.trim().split('\n').join(' | '));
process.exit(0);
