<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { loadConfigNative, saveConfigNative } from '$lib/config';
  import type { AppConfig } from '$lib/types';
  import FormInput from '$lib/components/form-input.svelte';
  import FormSelect from '$lib/components/form-select.svelte';
  import Button from '$lib/components/button.svelte';
  import FormActions from '$lib/components/form-actions.svelte';
  import LabeledField from '$lib/components/labeled-field.svelte';
  import MergerSection from '$lib/components/merger-section.svelte';
  import PageHeader from '$lib/components/page-header.svelte';
  import WarningBanner from '$lib/components/warning-banner.svelte';

  let config: AppConfig | null = $state(null);
  let warn: string = $state('');
  let gameOk: string = $state('');
  let showKey: boolean = $state(false);
  let mergerOpen: boolean = $state(false);
  let mergerState: string = $state('');
  let mergerRep: MergerRep | null = $state(null);

  type MergerRep = { config: string; wrong: [string, string, string][]; unfixable: [string, string][] };

  const codeFonts = ['JetBrains Mono', 'Fira Code', 'Hack', 'DejaVu Sans Mono', 'monospace'];

  onMount(async () => {
    console.debug("[w3mm] settings window mounted");
    try {
      config = await loadConfigNative();
      console.debug("[w3mm] settings config loaded", config);
      if (!config.gameDir) detect(true);
      else checkGame();
      checkMerger();
    } catch (e) {
      console.error("[w3mm] settings init failed", e);
      warn = String(e);
    }
  });

  async function pickDir(current: string, title: string, into: 'gameDir' | 'prefix') {
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
      const { open } = await import('@tauri-apps/plugin-shell');
      await open('https://www.nexusmods.com/users/myaccount?tab=api');
    } catch {}
  }

  async function save() {
    if (!config) return;
    const ok = await invoke<boolean>('is_game_dir', { path: config.gameDir }).catch(() => false);
    if (!ok) { warn = 'Pick the Witcher 3 folder first (it holds content/ and bin/).'; return; }
    await saveConfigNative(config);
    try {
      await invoke('open_manager', { gameDir: config.gameDir, prefix: config.prefix });
      const { emit } = await import('@tauri-apps/api/event');
      await emit('mods-changed', {});
    } catch {}
    await getCurrentWindow().close().catch(() => history.back());
  }

  async function cancel() {
    await getCurrentWindow().close().catch(() => history.back());
  }
</script>

<div class="mx-auto flex h-full max-w-[680px] flex-col gap-3 overflow-y-auto px-[22px] py-5">
  <PageHeader title="Settings" description="Pick your Witcher 3 folder and its Proton/Wine prefix. The prefix holds mods.settings — without it, load order isn't applied." />

  {#if config}
    <LabeledField label="Game folder">
      <FormInput bind:value={config.gameDir} mono class="min-w-0 flex-1" oninput={checkGame} />
      <Button variant="secondary" size="md" onclick={() => pickDir((config as AppConfig).gameDir, 'Select The Witcher 3 folder', 'gameDir')}>Browse…</Button>
    </LabeledField>
    {#if gameOk}<div class="pl-[122px] text-[12px] text-[#e3735f]">{gameOk}</div>{/if}
    <LabeledField label="Prefix">
      <FormInput bind:value={config.prefix} mono placeholder="e.g. …/steamapps/compatdata/292030/pfx" class="min-w-0 flex-1" />
      <Button variant="secondary" size="md" onclick={() => pickDir((config as AppConfig).prefix, 'Select the Wine/Proton prefix', 'prefix')}>Browse…</Button>
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
    <FormActions cancelLabel="Cancel" onCancel={cancel} primaryLabel="Save" onPrimary={save} primaryClass="px-[18px] py-2 text-sm" spacer={false} />
  </div>
</div>
