import { open, save, confirm as confirmNative } from '@tauri-apps/plugin-dialog';

type DialogOverride = { files: (string | null)[]; confirms: boolean[]; log: string[] };

// DEV-only test seam: E2E drives the app through CDP and queues dialog answers
// via page.evaluate, so tests never touch the OS file dialog. In release builds
// `import.meta.env.DEV` is false and these branches are compiled out.
function overrideState(): DialogOverride | null {
  if (!import.meta.env.DEV) return null;
  const w = globalThis as unknown as { __pqDialogOverride?: DialogOverride };
  if (!w.__pqDialogOverride) w.__pqDialogOverride = { files: [], confirms: [], log: [] };
  return w.__pqDialogOverride;
}

export async function pickOpen(options: Parameters<typeof open>[0] = {}): Promise<string | null> {
  const o = overrideState();
  if (o && o.files.length > 0) {
    const next = o.files.shift() ?? null;
    o.log.push(`open:${next ?? '<cancel>'}`);
    return next;
  }
  const res = await open(options);
  return Array.isArray(res) ? (res[0] ?? null) : res;
}

export async function pickSave(options: Parameters<typeof save>[0] = {}): Promise<string | null> {
  const o = overrideState();
  if (o && o.files.length > 0) {
    const next = o.files.shift() ?? null;
    o.log.push(`save:${next ?? '<cancel>'}`);
    return next;
  }
  return await save(options);
}

export async function confirmDialog(message: string): Promise<boolean> {
  const o = overrideState();
  if (o && o.confirms.length > 0) {
    const next = o.confirms.shift() ?? false;
    o.log.push(`confirm:${next}:${message}`);
    return next;
  }
  return await confirmNative(message);
}
