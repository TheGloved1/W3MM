<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import PageHeader from '$lib/components/page-header.svelte';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';

  let base = $state('// paste vanilla .ws here');
  let va = $state('// mod A version');
  let vb = $state('// mod B version');
  let out = $state<string[]>([]);
  let conflicts = $state<{ base_lo: number; base_hi: number; variants: string[][] }[]>([]);
  let error = $state('');

  function lines(s: string) { return s.replace(/\r\n/g, '\n').split('\n'); }

  async function run(res: number[] = []) {
    error = '';
    try {
      const r = await invoke<{ merged: string[]; conflicts: typeof conflicts; needs_resolution: boolean }>('merge_scripts', {
        baseB64: lines(base), versionsB64: [lines(va), lines(vb)], resolutions: res,
      });
      out = r.merged; conflicts = r.conflicts;
    } catch (e) { error = String(e); }
  }
</script>

<div class="flex flex-1 flex-col min-w-0 bg-background overflow-hidden">
  <PageHeader title="Merges" subtitle="Script merger (XML merger: same engine via merge_xml)">
    {#snippet right()}<Button size="sm" onclick={() => run()}>Merge</Button>{/snippet}
  </PageHeader>
  <div class="flex-1 overflow-auto p-6">
    <div class="mx-auto max-w-[900px] space-y-4">
      {#if error}<Card><CardContent class="text-sm text-red-500 py-3">{error}</CardContent></Card>{/if}
      <div class="grid grid-cols-1 md:grid-cols-3 gap-2">
        <Card><CardHeader><CardTitle class="text-xs">Vanilla</CardTitle></CardHeader><CardContent><textarea bind:value={base} rows="12" class="w-full font-mono text-xs rounded border bg-background p-2"></textarea></CardContent></Card>
        <Card><CardHeader><CardTitle class="text-xs">Mod A</CardTitle></CardHeader><CardContent><textarea bind:value={va} rows="12" class="w-full font-mono text-xs rounded border bg-background p-2"></textarea></CardContent></Card>
        <Card><CardHeader><CardTitle class="text-xs">Mod B</CardTitle></CardHeader><CardContent><textarea bind:value={vb} rows="12" class="w-full font-mono text-xs rounded border bg-background p-2"></textarea></CardContent></Card>
      </div>
      {#if conflicts.length}
        <Card><CardHeader><CardTitle class="text-sm">{conflicts.length} conflict(s) — pick a winner per region</CardTitle></CardHeader>
        <CardContent class="space-y-3">
          {#each conflicts as c, i}
            <div class="rounded border p-2 text-xs">
              <div class="text-muted-foreground mb-1">lines {c.base_lo}–{c.base_hi}</div>
              {#each c.variants as v, j}
                <Button size="sm" variant="ghost" onclick={() => { const res = conflicts.map((_, k) => k === i ? j : 0); run(res); }}>Take variant {j + 1}</Button>
                <pre class="font-mono whitespace-pre-wrap bg-muted/50 rounded p-1 mt-1">{v.join('\n')}</pre>
              {/each}
            </div>
          {/each}
        </CardContent></Card>
      {/if}
      {#if out.length}
        <Card><CardHeader><CardTitle class="text-sm">Merged</CardTitle></CardHeader>
        <CardContent><pre class="font-mono text-xs whitespace-pre-wrap">{out.join('\n')}</pre></CardContent></Card>
      {/if}
    </div>
  </div>
</div>
