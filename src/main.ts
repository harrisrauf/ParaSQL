import { mount } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import App from './App.svelte';
import './app.css';

// Diagnostics bridge — only installed in dev builds to avoid an IPC round-trip
// per console call in production.
if (import.meta.env.DEV) {
  function logWeb(msg: string) {
    try {
      invoke('debug_log', { msg });
    } catch {
      /* ignore */
    }
  }

  window.addEventListener('error', (e) => {
    logWeb(`ERROR: ${e.message} @ ${e.filename}:${e.lineno}\n${e.error?.stack ?? ''}`);
  });
  window.addEventListener('unhandledrejection', (e) => {
    const r = e.reason as { stack?: string } | string | undefined;
    logWeb(`UNHANDLED REJECTION: ${typeof r === 'string' ? r : (r?.stack ?? String(r))}`);
  });

  const origLog = console.log.bind(console);
  const origError = console.error.bind(console);
  const origWarn = console.warn.bind(console);
  console.log = (...args) => { logWeb(`LOG: ${args.map(String).join(' ')}`); origLog(...args); };
  console.error = (...args) => { logWeb(`ERR: ${args.map(String).join(' ')}`); origError(...args); };
  console.warn = (...args) => { logWeb(`WARN: ${args.map(String).join(' ')}`); origWarn(...args); };
}

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
