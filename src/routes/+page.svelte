<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import PageHeader from '$lib/components/page-header.svelte';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import type { AppState } from '$lib/types';
  import { loadConfigNative } from '$lib/config';

  let appState: AppState | null = $state(null);
  let error: string = $state('');
  let busy: string = $state('');
  let gameDir: string = $state('');
  let prefix: string = $state('');

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
      appState = await invoke<AppState>('list_mods');
    } catch (e) { error = String(e); }
  }

  async function toggle(id: string, on: boolean) {
    await invoke('set_enabled', { ids: [id], on: !on });
    appState = await invoke<AppState>('list_mods');
  }

  async function deploy() {
    busy = 'Deploying…'; error = '';
    try {
      const files = await invoke<string[]>('deploy');
      busy = `Deployed ${files.length} files`;
      appState = await invoke<AppState>('list_mods');
    } catch (e) { error = String(e); busy = ''; }
  }

  async function remove(id: string) {
    if (!confirm('Remove mod and its staged files?')) return;
    await invoke('remove_mods', { ids: [id] });
    appState = await invoke<AppState>('list_mods');
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
    appState = await invoke<AppState>('list_mods');
  }

  onMount(boot);
</script>

<div class="flex flex-1 flex-col min-w-0 bg-background overflow-hidden">
  <PageHeader title="Mods" subtitle={gameDir || 'Set game folder in Settings'}>
    {#snippet right()}
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
          <CardHeader><CardTitle class="text-sm">{appState.mods.filter((m) => !m.sep).length} mods</CardTitle></CardHeader>
          <CardContent class="space-y-1">
            {#each appState.mods as m}
              {#if m.sep}
                <div class="pt-3 text-xs font-bold uppercase tracking-wide text-muted-foreground">— {m.name}</div>
              {:else}
                <div class="flex items-center gap-2 rounded-md border px-2 py-1.5 text-sm">
                  <input type="checkbox" checked={m.enabled} onchange={() => toggle(m.id, m.enabled)} aria-label="enabled" />
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium">{m.name}</div>
                    <div class="text-[11px] text-muted-foreground truncate">{m.version} {#if m.nexus}· nexus:{m.nexus}{/if}</div>
                  </div>
                  <label class="text-[11px] text-muted-foreground">prio
                    <input type="number" min="1" value={prioOf(m.id)} onchange={(e) => setPrio(m.id, e)} class="w-14 rounded border bg-background px-1 py-0.5 text-xs" />
                  </label>
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
