import { LazyStore } from '@tauri-apps/plugin-store';
import type { AppConfig } from './types';

// App settings persisted via the Tauri store plugin (settings.json).
// Add fields here as your app grows — keep DEFAULTS, load, and save in sync.
const store = new LazyStore('settings.json');

const DEFAULTS: AppConfig = {
  theme: 'default',
  font: 'inter',
};

export async function loadConfigNative(): Promise<AppConfig> {
  const theme = await store.get<string>('theme');
  const font = await store.get<string>('font');

  return {
    theme: theme ?? DEFAULTS.theme,
    font: font ?? DEFAULTS.font,
  };
}

export async function saveConfigNative(cfg: AppConfig): Promise<void> {
  await store.set('theme', cfg.theme);
  await store.set('font', cfg.font);
  await store.save();
}

// Also expose raw store for advanced use.
export { store as settingsStore };
