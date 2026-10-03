<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { loadConfigNative } from '$lib/config';
  import CodeViewer from '$lib/components/code-viewer.svelte';
  import SecondaryButton from '$lib/components/secondary-button.svelte';

  type Shared = { file: string; with: string[] };
  type Inputs = { rel: string; kind: string; base: string; base_encoding: string; versions: { label: string; mod_id: string; text: string }[] };
  type Conflict = { base_lo: number; base_hi: number; variants: string[][] };

  let files: { rel: string; kind: string; with: string[] }[] = $state([]);
  let sel: number = $state(0);
  let mi: Inputs | null = $state(null);
  let out: string[] = $state([]);
  let conflicts: Conflict[] = $state([]);
  let issues: string[] = $state([]);
  let status: string = $state('');
  let codeFont: string = $state('JetBrains Mono');
  let codeSize: number = $state(11);
  let loading: boolean = $state(false);

  const cur = $derived(files[sel] as { rel: string; kind: string; with: string[] } | undefined);
  function baseOf(m: Inputs | null): string[] {
    if (!m) return [];
    return m.base.split('\n');
  }

  const baseLines: string[] = $derived(baseOf(mi));
  const conflictSet = $derived.by(() => {
    const s = new Set<number>();
    for (const c of conflicts) for (let i = c.base_lo; i < c.base_hi; i++) s.add(i);
    return s;
  });

  onMount(async () => {
    try {
      const cfg = await loadConfigNative();
      codeFont = cfg.codeFont || codeFont;
      codeSize = cfg.codeSize || codeSize;
    } catch {}
    await reloadList();
  });

  async function reloadList() {
    try {
      const info = await invoke<Record<string, { scripts: Shared[]; xmls: Shared[]; lost: number }>>('analysis_summary');
      const seen = new Map<string, { rel: string; kind: string; with: string[] }>();
      for (const [, v] of Object.entries(info)) {
        for (const s of v.scripts) if (!seen.has(s.file.toLowerCase())) seen.set(s.file.toLowerCase(), { rel: s.file, kind: 'script', with: s.with });
        for (const s of v.xmls) if (!seen.has(s.file.toLowerCase())) seen.set(s.file.toLowerCase(), { rel: s.file, kind: 'xml', with: s.with });
      }
      files = [...seen.values()].sort((a, b) => a.rel.localeCompare(b.rel));
      if (sel >= files.length) sel = 0;
      if (files.length) await loadFile();
      else { mi = null; out = []; conflicts = []; status = 'Nothing overlaps — no decisions waiting.'; }
    } catch (e) { status = String(e); }
  }

  async function loadFile(res: number[] = []) {
    const f = files[sel];
    if (!f) return;
    loading = true; status = '';
    try {
      mi = await invoke<Inputs>('merge_inputs', { rel: f.rel });
      await run(res);
    } catch (e) { status = String(e); }
    loading = false;
  }

  function lines(s: string) { return s.split('\n'); }

  async function run(res: number[] = []) {
    if (!mi) return;
    status = '';
    try {
      if (mi.kind === 'xml') {
        const r = await invoke<{ merged: string; conflicts: { base_lines: string[]; variants: { lines: string[][] }[] }[]; needs_resolution: boolean }>('merge_xml', {
          base: mi.base, versions: mi.versions.map((v) => v.text), resolutions: res,
        });
        out = r.merged.split('\n');
        conflicts = r.conflicts.map((c) => ({ base_lo: 0, base_hi: 0, variants: c.variants.map((v: any) => (Array.isArray(v) ? v : v.lines ?? []).flat()) }));
        issues = [];
      } else {
        const r = await invoke<{ merged: string[]; conflicts: Conflict[]; needs_resolution: boolean }>('merge_scripts', {
          baseB64: lines(mi.base), versionsB64: mi.versions.map((v) => lines(v.text)), resolutions: res,
        });
        out = r.merged; conflicts = r.conflicts;
        issues = await invoke<string[]>('merge_check', { lines: out });
      }
      status = conflicts.length ? `${conflicts.length} decision${conflicts.length === 1 ? '' : 's'} waiting` : 'Merges cleanly — no decisions needed.';
    } catch (e) { status = String(e); }
  }

  async function save() {
    if (!mi) return;
    try {
      await invoke('save_merge', { rel: mi.rel, text: out.join('\n'), answers: [] });
      const { emit } = await import('@tauri-apps/api/event');
      await emit('mods-changed', {});
      status = `Kept merge for ${mi.rel} — deploy applies it.`;
      if (sel + 1 < files.length) { sel++; await loadFile(); }
      else await reloadList();
    } catch (e) { status = String(e); }
  }

  async function keepAll() {
    // accept the suggested (first-variant) answer for every open decision
    const res = conflicts.map(() => 0);
    await run(res);
    if (!conflicts.length) await save();
  }

  function pick(i: number, j: number) {
    const res = conflicts.map((_, k) => (k === i ? j : 0));
    run(res);
  }

  function codeStyle() { return `font-family:'${codeFont}',monospace;font-size:${codeSize}pt;`; }
</script>

<div class="flex h-full min-h-0 flex-col gap-2 px-[18px] py-3">
  <div class="flex items-center gap-2">
    <div class="text-[15px] font-semibold">Script decisions</div>
    <span class="flex-1"></span>
    <SecondaryButton onclick={keepAll} disabled={!conflicts.length}>Keep every suggested merge</SecondaryButton>
    <SecondaryButton onclick={() => getCurrentWindow().close()}>Close</SecondaryButton>
  </div>

  <div class="flex min-h-0 flex-1 gap-3">
    <!-- file list -->
    <div class="flex w-[240px] shrink-0 flex-col rounded-[7px] border border-border bg-card">
      <div class="flex items-baseline px-3 pt-2">
        <span class="flex-1 text-[12pt] font-semibold">Scripts</span>
        <span class="text-[12px] text-muted-foreground">{files.length}</span>
      </div>
      <div class="min-h-0 flex-1 overflow-auto p-1.5">
        {#each files as f, i}
          <button onclick={() => { sel = i; loadFile(); }} class="block w-full truncate rounded px-2 py-1.5 text-left font-mono text-[12px] {i === sel ? 'bg-primary/20 text-primary' : 'hover:bg-accent'}">{f.rel}</button>
        {/each}
        {#if !files.length}<div class="px-2 py-3 text-[12px] text-muted-foreground">No overlapping files.</div>{/if}
      </div>
    </div>

    <!-- decision view -->
    <div class="flex min-w-0 flex-1 flex-col gap-2 overflow-auto">
      {#if cur}
        <div class="flex items-center gap-2">
          <span class="truncate font-mono text-[13px]">{cur.rel}</span>
          <span class="rounded-full bg-[#c9a45c]/15 px-2 py-[1px] text-[11px] font-semibold text-[#c9a45c]">{cur.kind === 'xml' ? 'XML' : 'Conflict'}</span>
          <span class="truncate text-[12px] text-muted-foreground">with {cur.with.join(', ')}</span>
        </div>
      {/if}
      {#if status}<div class="text-[13px] text-muted-foreground">{status}</div>{/if}

      {#if mi}
        <div class="grid min-h-[220px] flex-1 grid-cols-3 gap-2">
          <div class="flex min-w-0 flex-col rounded-[7px] border border-border bg-well">
            <div class="border-b border-border px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">Vanilla</div>
            <CodeViewer lines={baseLines} codeFont={codeFont} codeSize={codeSize} highlight={(i)=>conflictSet.has(i)} />
          </div>
          {#each mi.versions.slice(0, 2) as v}
            <div class="flex min-w-0 flex-col rounded-[7px] border border-border bg-well">
              <div class="truncate border-b border-border px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">{v.label}</div>
              <CodeViewer lines={v.text.split('\n')} codeFont={codeFont} codeSize={codeSize} />
            </div>
          {/each}
        </div>

        {#if conflicts.length}
          <div class="text-[13px] font-semibold">Decisions</div>
          {#each conflicts as c, i}
            <div class="rounded-[7px] border border-[#e3735f]/40 bg-card p-2">
              <div class="mb-1 text-[12px] text-muted-foreground">lines {c.base_lo + 1}–{Math.max(c.base_hi, c.base_lo + 1)}</div>
              {#each c.variants as v, j}
                <button onclick={() => pick(i, j)} class="mb-1 block w-full rounded border border-border bg-well p-1.5 text-left hover:border-primary">
                  <div class="mb-1 text-[11px] text-muted-foreground">Take variant {j + 1}{mi.versions[j] ? ` — ${mi.versions[j].label}` : ''}</div>
                  <pre class="overflow-auto whitespace-pre-wrap" style={codeStyle()}>{v.join('\n')}</pre>
                </button>
              {/each}
            </div>
          {/each}
        {/if}

        {#if issues.length}
          <div class="rounded-[7px] border border-border bg-card p-2 text-[12px]">
            <div class="mb-1 font-semibold">Checks</div>
            {#each issues as s}<div class="font-mono text-[#dbb977]">{s}</div>{/each}
          </div>
        {/if}

        {#if out.length}
          <div class="rounded-[7px] border border-border bg-well">
            <div class="flex items-center border-b border-border px-2 py-1">
              <span class="flex-1 text-[11px] uppercase tracking-wide text-muted-foreground">Merged preview</span>
              <button onclick={save} class="rounded-[7px] bg-primary px-3 py-1 text-[13px] font-semibold text-primary-foreground hover:brightness-110">Save merge</button>
            </div>
            <CodeViewer lines={out} codeFont={codeFont} codeSize={codeSize} class="max-h-64" />
          </div>
        {/if}
      {/if}
      {#if loading}<div class="text-[13px] text-muted-foreground">Loading…</div>{/if}
    </div>
  </div>
</div>
