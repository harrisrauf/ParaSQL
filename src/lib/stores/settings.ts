import { writable } from 'svelte/store';

interface Settings {
  darkMode: boolean;
  pageSize: number;
  dateFormat: string;
  floatFormat: 'standard' | 'scientific';
}

function loadSettings(): Settings {
  if (typeof window !== 'undefined') {
    try {
      const saved = localStorage.getItem('parquet-viewer-settings');
      if (saved) {
        return { ...getDefaultSettings(), ...JSON.parse(saved) };
      }
    } catch (e) {
      console.error('Failed to load settings:', e);
    }
  }
  return getDefaultSettings();
}

function getDefaultSettings(): Settings {
  return {
    darkMode: false,
    pageSize: 500,
    dateFormat: 'YYYY-MM-DD',
    floatFormat: 'standard',
  };
}

function createSettingsStore() {
  const initial = loadSettings();
  const { subscribe, set, update } = writable<Settings>(initial);

  function save(settings: Settings) {
    if (typeof window !== 'undefined') {
      localStorage.setItem('parquet-viewer-settings', JSON.stringify(settings));
      // Apply dark mode to document
      if (settings.darkMode) {
        document.documentElement.classList.add('dark');
      } else {
        document.documentElement.classList.remove('dark');
      }
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
  };
}

export const settings = createSettingsStore();
