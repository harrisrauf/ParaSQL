// Probe menu dropdown item matching for Workspace > Save Workspace.
import { connect, openWorkspaceDirect, getPage, openMenu, WS_DEMO, escapeNativeDialogs, disconnect, sleep } from './lib.mjs';

await connect({ reload: true });
await escapeNativeDialogs();
await openWorkspaceDirect(WS_DEMO);
await sleep(300);

const page = getPage();
await page.locator('.menu-title', { hasText: 'Workspace' }).click();
await sleep(300);
const all = await page.locator('.menu-dropdown .menu-item').allTextContents();
console.log('dropdown items:', JSON.stringify(all));
console.log('regex count:', await page.locator('.menu-dropdown .menu-item', { hasText: /^Save Workspace$/ }).count());
console.log('string count:', await page.locator('.menu-dropdown .menu-item', { hasText: 'Save Workspace' }).count());
console.log('exact first visible:', await page.locator('.menu-dropdown .menu-item', { hasText: 'Save Workspace' }).first().isVisible().catch((e) => 'ERR ' + e.message));
await page.keyboard.press('Escape');

try {
  const item = await openMenu('Workspace', /^Save Workspace$/);
  console.log('openMenu OK:', await item.textContent());
} catch (e) {
  console.log('openMenu FAILED:', e.message);
}
await escapeNativeDialogs();
await disconnect();
process.exit(0);
