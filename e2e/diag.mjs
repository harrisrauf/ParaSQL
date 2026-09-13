import { chromium } from 'playwright-core';

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

const probe = await page.evaluate(() => {
  const internals = window.__TAURI_INTERNALS__;
  const globalDesc = Object.getOwnPropertyDescriptor(window, '__TAURI_INTERNALS__');
  const props = {};
  for (const k of Object.getOwnPropertyNames(internals)) {
    const d = Object.getOwnPropertyDescriptor(internals, k);
    props[k] = { w: d.writable, c: d.configurable, e: d.enumerable, t: typeof d.value };
  }
  return {
    global: globalDesc
      ? { writable: globalDesc.writable, configurable: globalDesc.configurable, hasValue: !!globalDesc.value }
      : 'none',
    props,
    invokeSource: String(internals.invoke).slice(0, 700),
  };
});
console.log(JSON.stringify(probe, null, 2));
process.exit(0);
