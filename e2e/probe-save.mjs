// Focused probe: native save dialog helper behavior (New Workspace save dialog).
import { connect, getPage, withDialog, callFlow, ensureOutDir, OUT, disconnect, msgsOf, sleep, spawnAutoDialog } from './lib.mjs';
import { existsSync, statSync } from 'node:fs';
import path from 'node:path';

const target = path.join(OUT, 'probe-new.parasql');

try {
  ensureOutDir();
  await connect({ reload: true });
  console.log('connected; spawning helper + newWorkspaceFlow');
  const helper = spawnAutoDialog(target);
  const flow = callFlow('newWorkspaceFlow').then(
    (r) => ({ ok: true, r }),
    (e) => ({ ok: false, e: String(e).slice(0, 300) })
  );
  const helperRes = await Promise.race([
    helper.done,
    sleep(40000).then(() => ({ code: 'timeout', ok: false, output: '(still running after 40s)' })),
  ]);
  console.log('helper exit:', helperRes.code, 'ok:', helperRes.ok);
  console.log('helper output:', helperRes.output);
  const flowRes = await Promise.race([flow, sleep(10000).then(() => ({ ok: 'pending' }))]);
  console.log('flow result:', JSON.stringify(flowRes));
  console.log('file exists:', existsSync(target), existsSync(target) ? statSync(target).size : '');
  const editor = getPage();
  console.log('welcome still?', await editor.locator('.welcome').count(), 'panel empty?', await editor.locator('.panel .empty').count());
} catch (e) {
  console.log('probe-save error:', msgsOf(e));
}
await disconnect();
