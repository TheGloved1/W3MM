import { LazyStore } from '@tauri-apps/plugin-store';
import type { AppConfig } from './types';

// YAWMM settings persisted via the Tauri store plugin (settings.json).
const store = new LazyStore('settings.json');

const DEFAULTS: AppConfig = {
  theme: 'default',
  font: 'inter',
  gameDir: '',
  prefix: '',
  stagingDir: '',
  stagingNoticeDismissed: false,
  nexusKey: '',
  codeFont: 'JetBrains Mono',
  codeSize: 11,
  mergerPath: '',
};

export async function loadConfigNative(): Promise<AppConfig> {
  const get = (k: keyof AppConfig) => store.get<string>(k);
  const getN = (k: keyof AppConfig) => store.get<number>(k);
  const getB = (k: keyof AppConfig) => store.get<boolean>(k);
  const [theme, font, gameDir, prefix, stagingDir, stagingNoticeDismissed, nexusKey, codeFont, codeSize, mergerPath] = await Promise.all([
    get('theme'), get('font'), get('gameDir'), get('prefix'), get('stagingDir'), getB('stagingNoticeDismissed'), get('nexusKey'),
    get('codeFont'), getN('codeSize'), get('mergerPath'),
  ]);
  return {
    theme: theme ?? DEFAULTS.theme,
    font: font ?? DEFAULTS.font,
    gameDir: gameDir ?? '',
    prefix: prefix ?? '',
    stagingDir: stagingDir ?? '',
    stagingNoticeDismissed: stagingNoticeDismissed ?? false,
    nexusKey: nexusKey ?? '',
    codeFont: codeFont ?? DEFAULTS.codeFont,
    codeSize: codeSize ?? DEFAULTS.codeSize,
    mergerPath: mergerPath ?? '',
  };
}

export async function saveConfigNative(cfg: AppConfig): Promise<void> {
  await store.set('theme', cfg.theme);
  await store.set('font', cfg.font);
  await store.set('gameDir', cfg.gameDir);
  await store.set('prefix', cfg.prefix);
  await store.set('stagingDir', cfg.stagingDir);
  await store.set('stagingNoticeDismissed', cfg.stagingNoticeDismissed);
  await store.set('nexusKey', cfg.nexusKey);
  await store.set('codeFont', cfg.codeFont);
  await store.set('codeSize', cfg.codeSize);
  await store.set('mergerPath', cfg.mergerPath);
  await store.save();
}

export { store as settingsStore };
