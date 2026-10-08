<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { loadConfigNative, saveConfigNative } from '$lib/config';
  import type { AppConfig } from '$lib/types';
  import FormInput from '$lib/components/form-input.svelte';
  import FormSelect from '$lib/components/form-select.svelte';
  import Button from '$lib/components/button.svelte';
  import FormActions from '$lib/components/form-actions.svelte';
  import LabeledField from '$lib/components/labeled-field.svelte';
  import MergerSection from '$lib/components/merger-section.svelte';
  import HeaderBar from '$lib/components/header-bar.svelte';  import WarningBanner from '$lib/components/warning-banner.svelte';

  let config: AppConfig | null = $state(null);
  let warn: string = $state('');
  let gameOk: string = $state('');
  let showKey: boolean = $state(false);
  let mergerOpen: boolean = $state(false);
  let mergerState: string = $state('');
  let mergerRep: MergerRep | null = $state(null);
  /** Native Windows runs need no Proton/Wine prefix: settings live in
   *  %USERPROFILE%\Documents, so the prefix field stays hidden there. */
  let isWindows: boolean = $state(false);
  let initialStaging: string = $state('');
  /** What's shown in the staging field: the override, or the backend's
   *  default staging dir when no override is set. */
  let stagingShown: string = $state('');
  let defaultStaging: string = $state('');

  let savedAt: string = $state('');

  type MergerRep = { config: string; wrong: [string, string, string][]; unfixable: [string, string][] };

  const codeFonts = ['JetBrains Mono', 'Fira Code', 'Hack', 'DejaVu Sans Mono', 'monospace'];

  onMount(async () => {
    console.debug("[yawmm] settings window mounted");
    try {
      isWindows = (await import('@tauri-apps/plugin-os').then((m) => m.platform()).catch(() => '')) === 'windows';
      config = await loadConfigNative();
      initialStaging = config.stagingDir ?? '';
      try {
        const rep = await invoke<{ default_staging: string }>('storage_report');
        defaultStaging = rep.default_staging ?? '';
      } catch {}
      stagingShown = config.stagingDir || defaultStaging;
      console.debug("[yawmm] settings config loaded", config);
      if (!config.gameDir) detect(true);
      else checkGame();
      checkMerger();
    } catch (e) {
      console.error("[yawmm] settings init failed", e);
      warn = String(e);
    }
  });

  async function pickDir(current: string, title: string, into: 'gameDir' | 'prefix' | 'stagingDir') {
    if (!config) return;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({ directory: true, multiple: false, defaultPath: current || undefined, title });
      if (typeof sel === 'string') {
        config[into] = sel;
        if (into === 'gameDir') checkGame();
        if (into === 'gameDir' || into === 'prefix') checkMerger();
      }
    } catch {}
  }

  async function pickMerger() {
    if (!config) return;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({ multiple: false, filters: [{ name: 'Script Merger', extensions: ['exe'] }] });
      if (typeof sel === 'string') { config.mergerPath = sel; checkMerger(); }
    } catch {}
  }

  async function detect(quiet = false) {
    if (!config) return;
    const found = await invoke<string | null>('detect_game').catch(() => null);
    if (found) {
      config.gameDir = found;
      const pfx = await invoke<string | null>('default_prefix', { gameDir: found }).catch(() => null);
      if (pfx) config.prefix = pfx;
      checkGame();
      checkMerger();
    } else if (!quiet) {
      warn = 'No Steam install found — pick the folders yourself.';
    }
  }

  async function checkGame() {
    if (!config?.gameDir) { gameOk = ''; return; }
    const ok = await invoke<boolean>('is_game_dir', { path: config.gameDir });
    gameOk = ok ? '' : 'not a game folder (need content/ + bin/)';
    if (!ok) warn = 'That game folder doesn’t look right (need content/ + bin/).';
    else if (warn.startsWith('That game folder')) warn = '';
  }

  async function checkMerger() {
    mergerRep = null;
    if (!config?.mergerPath) { mergerState = ''; return; }
    try {
      const rep = await invoke<MergerRep>('merger_check', { exePath: config.mergerPath });
      mergerRep = rep;
      const n = rep.wrong.length + rep.unfixable.length;
      mergerState = n ? `${n} path${n === 1 ? '' : 's'} need${n === 1 ? 's' : ''} attention` : 'paths look right';
    } catch (e) { mergerState = String(e); }
  }

  async function applyMergerFixes() {
    if (!mergerRep || !config) return;
    try {
      const fixes: Record<string, string> = {};
      for (const [k, , want] of mergerRep.wrong) fixes[k] = want;
      await invoke('merger_apply', { configPath: mergerRep.config, fixes });
      await checkMerger();
    } catch (e) { warn = String(e); }
  }

  async function getKey() {
    try {
      await invoke('open_path', { target: 'https://www.nexusmods.com/users/myaccount?tab=api' });
    } catch {}
  }

  async function save() {
    if (!config) return;
    const ok = await invoke<boolean>('is_game_dir', { path: config.gameDir }).catch(() => false);
    if (!ok) { warn = 'Pick the Witcher 3 folder first (it holds content/ and bin/).'; return; }
    if ((config.stagingDir ?? '') !== initialStaging) {
      // New staging home: re-show the split-drive notice until dismissed.
      config.stagingNoticeDismissed = false;
    }
    await saveConfigNative(config);
    initialStaging = config.stagingDir ?? '';
    try {
      await invoke('open_manager', { gameDir: config.gameDir, prefix: config.prefix, stagingDir: config.stagingDir ?? '' });
      const { emit } = await import('@tauri-apps/api/event');
      await emit('mods-changed', {});
    } catch {}
    warn = '';
    savedAt = new Date().toLocaleTimeString();
  }

  async function cancel() {
    // Sidebar page: revert to the last saved config instead of closing.
    try {
      config = await loadConfigNative();
      initialStaging = config.stagingDir ?? '';
      stagingShown = config.stagingDir || defaultStaging;
      warn = '';
      savedAt = '';
    } catch {}
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <HeaderBar title="Settings" subtitle="Game folder, prefix, Nexus key, merger paths">
    {#snippet right()}
      {#if savedAt}<span class="shrink-0 text-xs text-muted-foreground">Saved {savedAt}</span>{/if}
    {/snippet}
  </HeaderBar>
  <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-[22px] py-[14px]">

  {#if config}
    <LabeledField label="Game folder">
      <FormInput bind:value={config.gameDir} mono class="min-w-0 flex-1" oninput={checkGame} />
      <Button variant="secondary" size="md" onclick={() => pickDir((config as AppConfig).gameDir, 'Select The Witcher 3 folder', 'gameDir')}>Browse…</Button>
    </LabeledField>
    {#if gameOk}<div class="pl-[122px] text-[12px] text-[#e3735f]">{gameOk}</div>{/if}
    {#if !isWindows}
    <LabeledField label="Prefix">
      <FormInput bind:value={config.prefix} mono placeholder="e.g. …/steamapps/compatdata/292030/pfx" class="min-w-0 flex-1" />
      <Button variant="secondary" size="md" onclick={() => pickDir((config as AppConfig).prefix, 'Select the Wine/Proton prefix', 'prefix')}>Browse…</Button>
    </LabeledField>
    {/if}
    <LabeledField label="Staging">
      <FormInput bind:value={stagingShown} mono placeholder="same drive as the game is fastest" class="min-w-0 flex-1" oninput={() => { if (config) config.stagingDir = stagingShown; }} />
      <Button variant="secondary" size="md" onclick={async () => { await pickDir(stagingShown, 'Select the staging folder', 'stagingDir'); if (config) stagingShown = config.stagingDir || defaultStaging; }}>Browse…</Button>
      {#if config.stagingDir}<Button variant="secondary" size="md" onclick={() => { if (config) { config.stagingDir = ''; stagingShown = defaultStaging; } }}>Reset</Button>{/if}
    </LabeledField>
    <LabeledField label="Nexus API key">
      <FormInput bind:value={config.nexusKey} type={showKey ? 'text' : 'password'} mono placeholder="optional — for update checks and Nexus downloads" class="min-w-0 flex-1" />
      <Button variant="secondary" size="md" onclick={() => (showKey = !showKey)}>{showKey ? 'Hide' : 'Show'}</Button>
      <Button variant="secondary" size="md" onclick={getKey}>Get key…</Button>
    </LabeledField>
    <LabeledField label="Code font">
      <FormSelect bind:value={config.codeFont} options={codeFonts.map(f=>({value:f,label:f}))} class="min-w-0 flex-1" />
      <FormInput bind:value={config.codeSize} type="number" width="86px" class="" />
    </LabeledField>
    <div class="flex gap-3">
      <span class="w-[110px] shrink-0"></span>
      <div class="flex-1 rounded-[7px] border border-border bg-well px-3 py-2" style="font-family:{config.codeFont};font-size:{config.codeSize}pt"><span class="text-[#b48ead]">if</span><span>( IsItemSingletonItem( l_items[0] ) )</span> <span class="text-[#7fbf8a]">// 00 1lI</span></div>
    </div>

    <MergerSection
      bind:mergerPath={config.mergerPath}
      mergerState={mergerState}
      mergerRep={mergerRep}
      open={mergerOpen}
      onToggle={() => mergerOpen = !mergerOpen}
      onPickMerger={pickMerger}
      onApplyFixes={applyMergerFixes}
    />
  {/if}

  <WarningBanner message={warn} />
  <div class="flex-1"></div>
  <div class="flex items-center gap-2">
    <Button variant="secondary" size="md" onclick={() => detect()}>Detect Steam install</Button>
    <span class="flex-1"></span>
    <FormActions cancelLabel="Revert" onCancel={cancel} primaryLabel="Save" onPrimary={save} primaryClass="px-[18px] py-2 text-sm" spacer={false} />
  </div>
  </div>
</div>
