<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { loadConfigNative, saveConfigNative } from '$lib/config';
  import type { AppConfig } from '$lib/types';
  import PageHeader from '$lib/components/page-header.svelte';
  import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Label } from '$lib/components/ui/label';
  import * as Select from '$lib/components/ui/select';
  import { Separator } from '$lib/components/ui/separator';

  const themes = [
    { id: 'default', label: 'Default (NMS Dark)', desc: 'Space blue — default' },
    { id: 'rose-pine', label: 'Rose Pine', desc: 'Muted pine, love pink' },
    { id: 'rose-pine-moon', label: 'Rose Pine Moon', desc: 'Darker violet' },
    { id: 'rose-pine-dawn', label: 'Rose Pine Dawn', desc: 'Warm light' },
    { id: 'catppuccin-mocha', label: 'Catppuccin Mocha', desc: 'Rich dark' },
    { id: 'catppuccin-macchiato', label: 'Catppuccin Macchiato', desc: 'Soft dark' },
    { id: 'catppuccin-frappe', label: 'Catppuccin Frappé', desc: 'Muted mid' },
    { id: 'catppuccin-latte', label: 'Catppuccin Latte', desc: 'Bright light' },
  ];

  const fonts = [
    { id: 'inter', label: 'Inter', desc: 'Clean sans — default' },
    { id: 'geist', label: 'Geist Sans', desc: 'Geometric' },
    { id: 'space', label: 'Space Grotesk', desc: 'Futuristic' },
    { id: 'manrope', label: 'Manrope', desc: 'Friendly' },
    { id: 'sora', label: 'Sora', desc: 'Soft rounded' },
    { id: 'jetbrains', label: 'JetBrains Mono', desc: 'Mono' },
  ];

  let config: AppConfig | null = $state(null);
  let status = $state("");
  let gameOk = $state<string>("");

  function apply(cfg: AppConfig) {
    document.documentElement.setAttribute('data-theme', cfg.theme);
    document.documentElement.setAttribute('data-font', cfg.font);
  }

  async function persist() {
    if (!config) return;
    apply(config);
    await saveConfigNative(config);
    status = "Saved";
    setTimeout(() => status = "", 2000);
  }

  async function onThemeChange(v: string) {
    if (!config || !v) return;
    config.theme = v;
    await persist();
  }

  async function onFontChange(v: string) {
    if (!config || !v) return;
    config.font = v;
    await persist();
  }

  onMount(async () => {
    config = await loadConfigNative();
    apply(config);
  });

  import { invoke } from '@tauri-apps/api/core';
  async function pickDir(current: string | undefined, title: string): Promise<string | null> {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({ directory: true, multiple: false, defaultPath: current || undefined, title });
      return typeof sel === 'string' ? sel : null;
    } catch { return null; }
  }
  async function browseGame() {
    if (!config) return;
    const sel = await pickDir(config.gameDir, 'Pick The Witcher 3 folder');
    if (sel) { config.gameDir = sel; await persist(); await checkGame(); }
  }
  async function browsePrefix() {
    if (!config) return;
    const sel = await pickDir(config.prefix, 'Pick Proton prefix (…/compatdata/292030/pfx)');
    if (sel) { config.prefix = sel; await persist(); }
  }
  async function detect() {
    if (!config) return;
    const found = await invoke<string | null>('detect_game').catch(() => null);
    if (found) {
      config.gameDir = found;
      const pfx = await invoke<string | null>('default_prefix', { gameDir: found }).catch(() => null);
      if (pfx) config.prefix = pfx;
      await persist();
    }
  }
  async function checkGame() {
    if (!config?.gameDir) return;
    gameOk = await invoke<boolean>('is_game_dir', { path: config.gameDir }) ? 'looks like the game folder' : 'not a game folder (need content/ + bin/)';
  }
  async function validateKey() {
    if (!config?.nexusKey) { status = 'Paste a key first'; return; }
    try {
      await invoke('nexus_status', { apiKey: config.nexusKey });
      status = 'Nexus key OK';
    } catch (e) { status = String(e); }
    setTimeout(() => status = '', 3000);
  }
  async function importLegacy() {
    if (!config?.gameDir) return;
    try {
      const prev = await invoke<{ mods: { name: string }[] }>('import_legacy_preview', { gameDir: config.gameDir });
      status = `_ModManager has ${prev.mods.length} rows (preview only — clean break, nothing imported yet)`;
    } catch (e) { status = String(e); }
    setTimeout(() => status = '', 4000);
  }
</script>

<div class="flex-1 bg-background text-foreground">
  <PageHeader sticky title="Settings" subtitle="Customize your app">
    {#snippet before()}
      <Button variant="ghost" size="sm" onclick={() => goto('/')}>← Back</Button>
      <div class="h-4 w-px bg-border"></div>
    {/snippet}
    {#snippet right()}
      <div class="text-xs text-muted-foreground">{status}</div>
    {/snippet}
  </PageHeader>

  <div class="mx-auto max-w-[720px] p-6 space-y-6">
    <div>
      <h1 class="text-lg font-semibold tracking-tight">Game</h1>
      <p class="text-sm text-muted-foreground">Linux-only: Steam + Proton prefix. Home is <span class="font-mono">&lt;game&gt;/_W3LMN/</span>.</p>
    </div>

    <Card>
      <CardHeader>
        <CardTitle class="text-sm">Witcher 3 + Proton</CardTitle>
        <CardDescription>Game folder holds content/ + bin/. Prefix is .../compatdata/292030/pfx.</CardDescription>
      </CardHeader>
      <CardContent class="space-y-3">
        {#if config}
          <div class="space-y-1">
            <Label>Game folder</Label>
            <div class="flex gap-2">
              <input bind:value={config.gameDir} oninput={persist} placeholder="/home/you/.local/share/Steam/steamapps/common/The Witcher 3" class="flex-1 rounded border bg-background px-2 py-1 text-sm font-mono" />
              <Button size="sm" variant="ghost" onclick={browseGame}>Browse</Button>
              <Button size="sm" onclick={detect}>Detect</Button>
              <Button size="sm" variant="ghost" onclick={checkGame}>Check</Button>
            </div>
            {#if gameOk}<div class="text-[11px] text-muted-foreground">{gameOk}</div>{/if}
          </div>
          <div class="space-y-1">
            <Label>Proton prefix</Label>
            <div class="flex gap-2">
              <input bind:value={config.prefix} oninput={persist} placeholder=".../steamapps/compatdata/292030/pfx" class="flex-1 rounded border bg-background px-2 py-1 text-sm font-mono" />
              <Button size="sm" variant="ghost" onclick={browsePrefix}>Browse</Button>
            </div>
          </div>
          <div class="space-y-1">
            <Label>Nexus API key</Label>
            <div class="flex gap-2">
              <input bind:value={config.nexusKey} oninput={persist} type="password" placeholder="Personal API key from nexusmods.com" class="flex-1 rounded border bg-background px-2 py-1 text-sm font-mono" />
              <Button size="sm" variant="ghost" onclick={validateKey}>Validate</Button>
            </div>
          </div>
          <div class="flex gap-2 pt-1">
            <Button size="sm" variant="ghost" onclick={importLegacy}>Preview _ModManager import</Button>
          </div>
        {/if}
      </CardContent>
    </Card>
    <!-- TODO: add your own settings cards here. Appearance below demos the theme system. -->
    <div>
      <h1 class="text-lg font-semibold tracking-tight">Appearance</h1>
      <p class="text-sm text-muted-foreground">Themes and fonts apply instantly and persist via Tauri store.</p>
    </div>

    <Card>
      <CardHeader>
        <CardTitle class="text-sm">Theme</CardTitle>
        <CardDescription>Pick your vibe — Rose Pine & Catppuccin flavors included.</CardDescription>
      </CardHeader>
      <CardContent class="space-y-4">
        <div class="space-y-2">
          <Label>Theme</Label>
          {#if config}
            <Select.Root type="single" value={config.theme} onValueChange={onThemeChange}>
              <Select.Trigger class="w-full">
                <Select.Value placeholder="Select theme" />
              </Select.Trigger>
              <Select.Content>
                {#each themes as t}
                  <Select.Item value={t.id}>
                    <div class="flex flex-col items-start">
                      <span class="text-sm">{t.label}</span>
                      <span class="text-[11px] text-muted-foreground">{t.desc}</span>
                    </div>
                  </Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          {/if}
        </div>

        <div class="grid grid-cols-2 sm:grid-cols-4 gap-2">
          {#each themes as t}
            <button
              class="rounded-lg border p-3 text-left hover:bg-muted transition {config?.theme === t.id ? 'border-primary ring-1 ring-primary' : 'border-border'}"
              onclick={() => onThemeChange(t.id)}
            >
              <div class="text-xs font-medium">{t.label}</div>
              <div class="mt-2 flex gap-1">
                <span class="h-3 w-6 rounded" style="background: var(--color-nms-bg)"></span>
                <span class="h-3 w-6 rounded" style="background: var(--color-primary)"></span>
                <span class="h-3 w-6 rounded" style="background: var(--color-nms-panel)"></span>
              </div>
            </button>
          {/each}
        </div>

        <Separator />

        <div class="space-y-2">
          <Label>Font</Label>
          {#if config}
            <Select.Root type="single" value={config.font} onValueChange={onFontChange}>
              <Select.Trigger class="w-full">
                <Select.Value placeholder="Select font" />
              </Select.Trigger>
              <Select.Content>
                {#each fonts as f}
                  <Select.Item value={f.id}>
                    <div class="flex flex-col items-start">
                      <span class="text-sm" style="font-family: var(--font-sans)">{f.label}</span>
                      <span class="text-[11px] text-muted-foreground">{f.desc}</span>
                    </div>
                  </Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          {/if}
          <div class="rounded-md border bg-muted/30 p-3">
            <div class="text-sm font-medium" style="font-family: var(--font-sans)">Preview — Aa Bb Cc 123</div>
            <div class="text-xs text-muted-foreground mt-1" style="font-family: var(--font-sans)">The quick brown fox jumps over the lazy dog.</div>
          </div>
        </div>
      </CardContent>
    </Card>
  </div>
</div>
