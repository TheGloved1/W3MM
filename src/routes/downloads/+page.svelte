<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import PageHeader from '$lib/components/page-header.svelte';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';

  let archive = $state('');
  let names = $state<string[]>([]);
  let moves = $state<[string, string][]>([]);
  let error = $state('');

  async function list() {
    error = '';
    try { names = await invoke<string[]>('list_archive', { path: archive }); }
    catch (e) { error = String(e); }
  }
  async function preview() {
    error = ''; moves = [];
    try {
      const plan = await invoke<{ moves: [string, string][]; docs: string[] }>('preview_archive', { path: archive });
      moves = plan.moves.slice(0, 200);
    } catch (e) { error = String(e); }
  }
  async function parsed() {
    try {
      const base = archive.split('/').pop() ?? archive;
      const [n, v, id] = await invoke<[string, string, string]>('parse_archive_name', { filename: base });
      error = `name=${n} version=${v} nexus=${id}`;
    } catch (e) { error = String(e); }
  }
</script>

<div class="flex flex-1 flex-col min-w-0 bg-background overflow-hidden">
  <PageHeader title="Downloads" subtitle="Pure-Rust .zip/.7z/.tar install preview (RAR: repack, see README)" />
  <div class="flex-1 overflow-auto p-6">
    <div class="mx-auto max-w-[900px] space-y-4">
      {#if error}<Card><CardContent class="text-sm py-3">{error}</CardContent></Card>{/if}
      <Card><CardHeader><CardTitle class="text-sm">Archive</CardTitle></CardHeader>
        <CardContent class="flex gap-2">
          <input bind:value={archive} placeholder="/path/to/mod.zip" class="flex-1 rounded border bg-background px-2 py-1 text-sm font-mono" />
          <Button size="sm" onclick={list}>List</Button>
          <Button size="sm" onclick={preview}>Preview</Button>
          <Button size="sm" variant="ghost" onclick={parsed}>Parse name</Button>
        </CardContent></Card>
      {#if names.length}
        <Card><CardHeader><CardTitle class="text-sm">{names.length} entries</CardTitle></CardHeader>
        <CardContent><pre class="font-mono text-xs whitespace-pre-wrap max-h-64 overflow-auto">{names.slice(0, 200).join('\n')}</pre></CardContent></Card>
      {/if}
      {#if moves.length}
        <Card><CardHeader><CardTitle class="text-sm">Install plan (first {moves.length})</CardTitle></CardHeader>
        <CardContent><pre class="font-mono text-xs whitespace-pre-wrap max-h-64 overflow-auto">{moves.map(([s, d]) => `${d}`).join('\n')}</pre></CardContent></Card>
      {/if}
    </div>
  </div>
</div>
