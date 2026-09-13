import { writable } from 'svelte/store';

interface UiState {
  sidebarVisible: boolean;
}

function createUiStore() {
  const { subscribe, set, update } = writable<UiState>({ sidebarVisible: true });

  function toggleSidebar() {
    update(s => ({ ...s, sidebarVisible: !s.sidebarVisible }));
  }

  return {
    subscribe,
    set,
    update,
    toggleSidebar,
  };
}

export const uiStore = createUiStore();

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'error';
  message: string;
}

function createToastStore() {
  const { subscribe, update } = writable<Toast[]>([]);
  let nextId = 1;

  function dismiss(id: number) {
    update(list => list.filter(t => t.id !== id));
  }

  function notify(message: string, kind: Toast['kind'] = 'info') {
    const id = nextId++;
    update(list => [...list, { id, kind, message }]);
    setTimeout(() => dismiss(id), kind === 'error' ? 7000 : 4500);
  }

  return { subscribe, notify, dismiss };
}

export const toastStore = createToastStore();

function createQueryHistoryStore() {
  const { subscribe, update } = writable<string[]>([]);
  return {
    subscribe,
    push(sql: string) {
      update(h => [sql, ...h.filter(q => q !== sql)].slice(0, 20));
    },
    clear() {
      update(() => []);
    },
  };
}

export const queryHistoryStore = createQueryHistoryStore();

export function notify(message: string, kind: Toast['kind'] = 'info') {
  toastStore.notify(message, kind);
}