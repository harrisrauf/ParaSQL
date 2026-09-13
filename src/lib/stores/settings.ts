import { writable } from 'svelte/store';

export type RowDensity = 'compact' | 'comfortable';
export type SchemaDialect = 'duckdb' | 'postgres' | 'mysql' | 'sqlite';
export type ThemeName = 'simple' | 'parasql';

interface Settings {
  darkMode: boolean;
  theme: ThemeName;
  pageSize: number;
  dateFormat: string;
  floatFormat: 'standard' | 'scientific';
  rowDensity: RowDensity;
  schemaDialect: SchemaDialect;
  recentFiles: string[];
}

const RECENT_LIMIT = 10;

function getDefaultSettings(): Settings {
  return {
    darkMode: false,
    theme: 'parasql',
    pageSize: 500,
    dateFormat: 'YYYY-MM-DD',
    floatFormat: 'standard',
    rowDensity: 'comfortable',
    schemaDialect: 'duckdb',
    recentFiles: [],
  };
}

function loadSettings(): Settings {
  if (typeof window !== 'undefined') {
    try {
      const saved =
        localStorage.getItem('parasql-settings') ??
        localStorage.getItem('parquet-viewer-settings');
      if (saved) {
        const raw = JSON.parse(saved) as Record<string, unknown>;
        const merged = { ...getDefaultSettings(), ...raw } as Settings;
        // Legacy name: the original theme was renamed to "simple"
        if ((raw.theme as string) === 'default') merged.theme = 'simple';
        return merged;
      }
    } catch (e) {
      console.error('Failed to load settings:', e);
    }
  }
  return getDefaultSettings();
}

function createSettingsStore() {
  const initial = loadSettings();
  const { subscribe, set, update } = writable<Settings>(initial);

  function save(settings: Settings) {
    if (typeof window !== 'undefined') {
      localStorage.setItem('parasql-settings', JSON.stringify(settings));
      // Apply dark mode to document
      if (settings.darkMode) {
        document.documentElement.classList.add('dark');
      } else {
        document.documentElement.classList.remove('dark');
      }
      // Apply brand theme to document
      document.documentElement.classList.toggle('theme-parasql', settings.theme === 'parasql');
    }
  }

  // Apply initial settings
  if (typeof window !== 'undefined') {
    save(initial);
  }

  return {
    subscribe,
    set: (val: Settings) => {
      save(val);
      set(val);
    },
    update: (fn: (val: Settings) => Settings) => {
      update((val) => {
        const newVal = fn(val);
        save(newVal);
        return newVal;
      });
    },
    toggleDarkMode: () => {
      update((val) => {
        const newVal = { ...val, darkMode: !val.darkMode };
        save(newVal);
        return newVal;
      });
    },
    setRowDensity: (density: RowDensity) => {
      update((val) => {
        const newVal = { ...val, rowDensity: density };
        save(newVal);
        return newVal;
      });
    },
    setTheme: (theme: ThemeName) => {
      update((val) => {
        const newVal = { ...val, theme };
        save(newVal);
        return newVal;
      });
    },
    setSchemaDialect: (dialect: SchemaDialect) => {
      update((val) => {
        const newVal = { ...val, schemaDialect: dialect };
        save(newVal);
        return newVal;
      });
    },
    pushRecentFile: (path: string) => {
      update((val) => {
        const newVal = {
          ...val,
          recentFiles: [path, ...val.recentFiles.filter(p => p !== path)].slice(0, RECENT_LIMIT),
        };
        save(newVal);
        return newVal;
      });
    },
    removeRecentFile: (path: string) => {
      update((val) => {
        const newVal = { ...val, recentFiles: val.recentFiles.filter(p => p !== path) };
        save(newVal);
        return newVal;
      });
    },
  };
}

export const settings = createSettingsStore();