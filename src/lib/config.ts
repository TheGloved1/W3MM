import { LazyStore } from '@tauri-apps/plugin-store';
import type { AppConfig } from './types';

// W3LMN settings persisted via the Tauri store plugin (settings.json).
const store = new LazyStore('settings.json');

const DEFAULTS: AppConfig = {
  theme: 'default',
  font: 'inter',
  gameDir: '',
  prefix: '',
  nexusKey: '',
};

export async function loadConfigNative(): Promise<AppConfig> {
  const get = (k: keyof AppConfig) => store.get<string>(k);
  const [theme, font, gameDir, prefix, nexusKey] = await Promise.all([
    get('theme'), get('font'), get('gameDir'), get('prefix'), get('nexusKey'),
  ]);
  return {
    theme: theme ?? DEFAULTS.theme,
    font: font ?? DEFAULTS.font,
    gameDir: gameDir ?? '',
    prefix: prefix ?? '',
    nexusKey: nexusKey ?? '',
  };
}

export async function saveConfigNative(cfg: AppConfig): Promise<void> {
  await store.set('theme', cfg.theme);
  await store.set('font', cfg.font);
  await store.set('gameDir', cfg.gameDir);
  await store.set('prefix', cfg.prefix);
  await store.set('nexusKey', cfg.nexusKey);
  await store.save();
}

export { store as settingsStore };
