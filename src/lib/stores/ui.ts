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