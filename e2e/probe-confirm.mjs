// Diagnostic probe: how do confirm/prompt behave in this Tauri WebView2?
import { connect, reloadApp, getPage, disconnect, msgsOf } from './lib.mjs';

try {
  await connect({ reload: true });
  await reloadApp();
  const page = getPage();
  const info = await page.evaluate(() => {
    const out = {
      confirmSource: String(window.confirm).slice(0, 200),
      confirmIsNative: /\[native code\]/.test(String(window.confirm)),
      promptSource: String(window.prompt).slice(0, 200),
      alertSource: String(window.alert).slice(0, 200),
      tauready: typeof window.__TAURI_INTERNALS__,
    };
    return out;
  });
  console.log(JSON.stringify(info, null, 2));

  const call = await page.evaluate(async () => {
    try {
      const v = window.confirm('probe-confirm');
      return { kind: typeof v, awaited: v instanceof Promise ? await v : v };
    } catch (e) {
      return { threw: String(e) };
    }
  });
  console.log('confirm() result:', JSON.stringify(call));

  const pcall = await page.evaluate(async () => {
    try {
      const v = window.prompt('probe-prompt', 'abc');
      return { kind: typeof v, awaited: v instanceof Promise ? await v : v };
    } catch (e) {
      return { threw: String(e) };
    }
  });
  console.log('prompt() result:', JSON.stringify(pcall));
} catch (e) {
  console.log('probe error:', msgsOf(e));
}
await disconnect();
