<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import PageHeader from '$lib/components/page-header.svelte';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { loadConfigNative } from '$lib/config';
  import type { Snippets } from '$lib/types';

  let archive = $state('');
  let nxm = $state('');
  let names = $state<string[]>([]);
  let moves = $state<[string, string][]>([]);
  let snips = $state<Record<string, Snippets>>({});
  let error = $state('');
  let info = $state('');

  async function list() {
    error = '';
    try { names = await invoke<string[]>('list_archive', { path: archive }); }
    catch (e) { error = String(e); }
  }
  async function preview() {
    error = ''; moves = []; snips = {};
    try {
      const plan = await invoke<{ moves: [string, string][]; docs: string[] }>('preview_archive', { path: archive });
      moves = plan.moves.slice(0, 200);
      info = `${plan.moves.length} targets, ${plan.docs.length} docs`;
    } catch (e) { error = String(e); }
  }
  async function parsed() {
    try {
      const base = archive.split('/').pop() ?? archive;
      const [n, v, id] = await invoke<[string, string, string]>('parse_archive_name', { filename: base });
      info = `name=${n} version=${v} nexus=${id}`;
    } catch (e) { error = String(e); }
  }
  async function snippets() {
    error = '';
    try {
      const root = archive || '/tmp';
      snips = await invoke<Record<string, Snippets>>('scan_snippets', { root });
      info = `${Object.keys(snips).length} snippet files`;
    } catch (e) { error = String(e); }
  }
  async function dlNxm() {
    error = ''; info = '';
    try {
      const cfg = await loadConfigNative();
      if (!cfg.nexusKey) { error = 'Set Nexus API key in Settings first'; return; }
      const dest = await invoke<string>('download_nxm', { url: nxm, apiKey: cfg.nexusKey, destDir: '/tmp' });
      info = `downloaded → ${dest}`;
      archive = dest;
    } catch (e) { error = String(e); }
  }
</script>

<div class="flex flex-1 flex-col min-w-0 bg-background overflow-hidden">
  <PageHeader title="Downloads" subtitle="Archives + nxm:// + snippets (pure-Rust; RAR: repack)" />
  <div class="flex-1 overflow-auto p-6">
    <div class="mx-auto max-w-[900px] space-y-4">
      {#if error}<Card><CardContent class="text-sm text-red-500 py-3">{error}</CardContent></Card>{/if}
      {#if info}<div class="text-xs text-muted-foreground">{info}</div>{/if}
      <Card><CardHeader><CardTitle class="text-sm">nxm:// link</CardTitle></CardHeader>
        <CardContent class="flex gap-2">
          <input bind:value={nxm} placeholder="nxm://witcher3/mods/123/files/456?key=..&expires=..&user_id=.." class="flex-1 rounded border bg-background px-2 py-1 text-sm font-mono" />
          <Button size="sm" onclick={dlNxm}>Download</Button>
        </CardContent></Card>
      <Card><CardHeader><CardTitle class="text-sm">Archive</CardTitle></CardHeader>
        <CardContent class="flex gap-2">
          <input bind:value={archive} placeholder="/path/to/mod.zip" class="flex-1 rounded border bg-background px-2 py-1 text-sm font-mono" />
          <Button size="sm" onclick={list}>List</Button>
          <Button size="sm" onclick={preview}>Preview</Button>
          <Button size="sm" variant="ghost" onclick={parsed}>Parse</Button>
          <Button size="sm" variant="ghost" onclick={snippets}>Snippets</Button>
        </CardContent></Card>
      {#if names.length}
        <Card><CardHeader><CardTitle class="text-sm">{names.length} entries</CardTitle></CardHeader>
        <CardContent><pre class="font-mono text-xs whitespace-pre-wrap max-h-64 overflow-auto">{names.slice(0, 200).join('\n')}</pre></CardContent></Card>
      {/if}
      {#if moves.length}
        <Card><CardHeader><CardTitle class="text-sm">Install plan (first {moves.length})</CardTitle></CardHeader>
        <CardContent><pre class="font-mono text-xs whitespace-pre-wrap max-h-64 overflow-auto">{moves.map(([, d]) => `${d}`).join('\n')}</pre></CardContent></Card>
      {/if}
      {#if Object.keys(snips).length}
        <Card><CardHeader><CardTitle class="text-sm">Settings snippets</CardTitle></CardHeader>
        <CardContent class="space-y-2 text-xs">
          {#each Object.entries(snips) as [f, s]}
            <div class="rounded border p-2">
              <div class="font-mono truncate">{f}</div>
              {#each Object.entries(s.user) as [sec, vals]}<div class="mt-1"><span class="font-bold">[{sec}]</span>{#each vals as v}<div class="font-mono ml-3">{v}</div>{/each}</div>{/each}
              {#each s.input_xml as v}<div class="font-mono mt-1">{v}</div>{/each}
              {#if s.filelist.length}<div class="text-muted-foreground">filelist: {s.filelist.join(', ')}</div>{/if}
            </div>
          {/each}
        </CardContent></Card>
      {/if}
    </div>
  </div>
</div>
