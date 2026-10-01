<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import PageHeader from '$lib/components/page-header.svelte';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import type { AppState } from '$lib/types';
  import { loadConfigNative } from '$lib/config';

  let appState: AppState | null = $state(null);
  let clashMap: Record<string, string[]> = $state({});
  let error: string = $state('');
  let busy: string = $state('');
  let gameDir: string = $state('');
  let prefix: string = $state('');
  let archPath: string = $state('');

  async function boot() {
    try {
      const cfg = await loadConfigNative();
      gameDir = cfg.gameDir; prefix = cfg.prefix;
      if (!gameDir) {
        const found = await invoke<string | null>('detect_game').catch(() => null);
        if (found) {
          gameDir = found;
          const pfx = await invoke<string | null>('default_prefix', { gameDir }).catch(() => null);
          if (pfx) prefix = pfx;
        }
      }
      if (gameDir) await open();
    } catch (e) { error = String(e); }
  }

  async function open() {
    error = '';
    try {
      await invoke('open_manager', { gameDir, prefix });
      await refresh();
    } catch (e) { error = String(e); }
  }

  async function refresh() {
    appState = await invoke<AppState>('list_mods');
    clashMap = await invoke<Record<string, string[]>>('clashes').catch(() => ({}));
  }

  async function toggle(id: string, on: boolean) {
    await invoke('set_enabled', { ids: [id], on: !on });
    await refresh();
  }

  async function deploy() {
    busy = 'Deploying…'; error = '';
    try {
      const running = await invoke<boolean>('game_running');
      if (running) { error = 'Close the game before deploying'; busy = ''; return; }
      const files = await invoke<string[]>('deploy');
      busy = `Deployed ${files.length} files`;
      await refresh();
      try {
        const { sendNotification } = await import('@tauri-apps/plugin-notification');
        sendNotification({ title: 'W3LMN', body: `Deployed ${files.length} files` });
      } catch {}
    } catch (e) { error = String(e); busy = ''; }
  }

  async function remove(id: string) {
    if (!confirm('Remove mod and its staged files?')) return;
    await invoke('remove_mods', { ids: [id] });
    await refresh();
  }

  function prioOf(id: string): number {
    if (!appState) return 0;
    const i = appState.priority.indexOf(id);
    return i < 0 ? 0 : i + 1;
  }

  async function setPrio(id: string, ev: Event) {
    const n = Number((ev.target as HTMLInputElement).value);
    if (!n) return;
    await invoke('set_priority', { id, number: n });
    await refresh();
  }

  async function addSection() {
    const name = prompt('Section name', 'New section');
    if (!name || !appState) return;
    await invoke('add_separator', { index: appState.mods.length, name });
    await refresh();
  }

  async function editRow(id: string, cur: string, ver: string, nx: string) {
    const name = prompt('Mod name', cur);
    if (name === null) return;
    const version = prompt('Version', ver) ?? ver;
    const nexus = prompt('Nexus id', nx) ?? nx;
    await invoke('edit_mod', { id, name, version, nexusId: nexus, section: '' });
    await refresh();
  }

  function clashCount(id: string): number {
    return Object.values(clashMap).filter((ids) => ids.includes(id) && ids.length > 1).length;
  }

  async function install() {
    if (!archPath) {
      // Native file picker instead of pasting paths (dialog plugin).
      try {
        const { open } = await import('@tauri-apps/plugin-dialog');
        const sel = await open({ multiple: false, filters: [{ name: 'Mod archive', extensions: ['zip', '7z', 'tar', 'gz', 'tgz'] }] });
        if (typeof sel === 'string') archPath = sel;
      } catch {}
    }
    if (!archPath) return;
    busy = 'Installing…'; error = '';
    try {
      const base = archPath.split('/').pop() ?? archPath;
      const [n, v, nx] = await invoke<[string, string, string]>('parse_archive_name', { filename: base });
      const name = prompt('Mod name', n) ?? n;
      await invoke('install_archive', { path: archPath, name, version: v, nexusId: nx });
      archPath = '';
      await refresh();
      busy = 'Installed';
    } catch (e) { error = String(e); busy = ''; }
  }

  onMount(boot);
</script>

<div class="flex flex-1 flex-col min-w-0 bg-background overflow-hidden">
  <PageHeader title="Mods" subtitle={gameDir || 'Set game folder in Settings'}>
    {#snippet right()}
      <Button size="sm" variant="ghost" onclick={addSection} disabled={!appState}>+ Section</Button>
      <Button size="sm" onclick={deploy} disabled={!appState}>Deploy</Button>
    {/snippet}
  </PageHeader>

  <div class="flex-1 overflow-auto p-6">
    <div class="mx-auto max-w-[900px] space-y-4">
      {#if error}<Card><CardContent class="text-sm text-red-500 py-3">{error}</CardContent></Card>{/if}
      {#if busy}<div class="text-xs text-muted-foreground">{busy}</div>{/if}
      {#if !appState}
        <Card><CardHeader><CardTitle class="text-sm">No game open</CardTitle></CardHeader>
        <CardContent class="text-sm text-muted-foreground">Set the game + prefix in Settings, or auto-detect Steam. New home: <span class="font-mono">&lt;game&gt;/_W3LMN/</span> (clean break from <span class="font-mono">_ModManager</span>).</CardContent></Card>
      {:else}
        <Card>
          <CardHeader><CardTitle class="text-sm">{appState.mods.filter((m) => !m.sep).length} mods · {Object.keys(clashMap).length} clash paths</CardTitle></CardHeader>
          <CardContent class="space-y-1">
            <div class="flex gap-2 pb-2">
              <input bind:value={archPath} placeholder="/path/to/mod.zip → Install (empty = Browse)" class="flex-1 rounded border bg-background px-2 py-1 text-sm font-mono" />
              <Button size="sm" onclick={install}>{archPath ? 'Install' : 'Browse…'}</Button>
            </div>
            {#each appState.mods as m}
              {#if m.sep}
                <div class="pt-3 text-xs font-bold uppercase tracking-wide text-muted-foreground">— {m.name}</div>
              {:else}
                <div class="flex items-center gap-2 rounded-md border px-2 py-1.5 text-sm">
                  <input type="checkbox" checked={m.enabled} onchange={() => toggle(m.id, m.enabled)} aria-label="enabled" />
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium">{m.name} {#if clashCount(m.id)}<span class="ml-1 rounded bg-amber-500/20 px-1 text-[10px] text-amber-600">{clashCount(m.id)} clashes</span>{/if}</div>
                    <div class="text-[11px] text-muted-foreground truncate">{m.version} {#if m.nexus}· nexus:{m.nexus}{/if} · {m.targets.length} files</div>
                  </div>
                  <label class="text-[11px] text-muted-foreground">prio
                    <input type="number" min="1" value={prioOf(m.id)} onchange={(e) => setPrio(m.id, e)} class="w-14 rounded border bg-background px-1 py-0.5 text-xs" />
                  </label>
                  <Button size="sm" variant="ghost" onclick={() => editRow(m.id, m.name, m.version, m.nexus)}>Edit</Button>
                  <Button size="sm" variant="ghost" onclick={() => remove(m.id)}>Remove</Button>
                </div>
              {/if}
            {/each}
          </CardContent>
        </Card>
      {/if}
    </div>
  </div>
</div>
