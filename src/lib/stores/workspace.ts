import { writable } from 'svelte/store';
import type { Workspace, TableMeta } from '../types';

export interface WorkspaceState {
  /** Absolute path of the .parasql file, or null when none is open */
  path: string | null;
  doc: Workspace | null;
  activeView: 'query' | 'data' | 'charts';
  /** True while a table is materialized into the Lite editing flow */
  editorOpen: boolean;
  /** Cached file metadata, keyed by absolute path */
  metas: Record<string, TableMeta>;
  loading: boolean;
  error: string | null;
}

const initialState = (): WorkspaceState => ({
  path: null,
  doc: null,
  activeView: 'data',
  editorOpen: false,
  metas: {},
  loading: false,
  error: null,
});

function createWorkspaceStore() {
  const { subscribe, set, update } = writable<WorkspaceState>(initialState());

  return {
    subscribe,
    set,
    update,
    reset: () => set(initialState()),
  };
}

export const workspaceStore = createWorkspaceStore();
