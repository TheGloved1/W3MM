<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  let archPath: string = $state('');
  let stem: string = $state('');
  let moves: [string, string][] = $state([]);
  let docs: string[] = $state([]);
  let name: string = $state('');
  let version: string = $state('');
  let nexus: string = $state('');
  let section: string = $state('');
  let sections: string[] = $state([]);
  let collisions: string[] = $state([]);
  let replace: boolean = $state(true);
  let snippets: string[] = $state([]);
  let keybinds: number = $state(0);
  let gvNote: string = $state('');
  let warn: string = $state('');
  let busy: boolean = $state(false);

  onMount(async () => {
    archPath = page.url.searchParams.get('path') ?? '';
    if (!archPath) { warn = 'No archive given.'; return; }
    stem = archPath.split('/').pop() ?? archPath;
    try {
      const [n, v, nx] = await invoke<[string, string, string]>('parse_archive_name', { filename: stem });
      name = n; version = v; nexus = nx;
      const plan = await invoke<{ moves: [string, string][]; docs: string[] }>('preview_archive', { path: archPath });
      moves = plan.moves; docs = plan.docs;
      const targets = plan.moves.map(([, d]) => d);
      const [managed] = await invoke<[string[], string[]]>('find_collisions', { targets });
      collisions = managed;
      try {
        const st = await invoke<AppStateLike>('list_mods');
        sections = st.mods.filter((m) => m.sep).map((m) => m.name);
      } catch {}
      try {
        const sn = await invoke<Record<string, { user: Record<string, string[]>; input_xml: string[] }>>('scan_snippets', { root: '/tmp' });
        void sn;
      } catch {}
      // keybinds + snippets found inside the archive preview
      keybinds = 0;
      const found: string[] = [];
      for (const [, d] of moves.slice(0, 400)) {
        const low = d.toLowerCase();
        if (low.endsWith('input.settings') || low.includes('keybind') || low.includes('hotkey')) { keybinds++; found.push(d); }
      }
      // made-for note from staged game versions is computed after install; show counts meanwhile
      const scripts = moves.filter(([, d]) => d.toLowerCase().endsWith('.ws')).length;
      const xmls = moves.filter(([, d]) => d.toLowerCase().endsWith('.xml')).length;
      gvNote = `${moves.length} files (${scripts} scripts, ${xmls} xml${docs.length ? `, ${docs.length} docs` : ''})`;
      void found;
    } catch (e) { warn = String(e); }
  });

  type AppStateLike = { mods: { sep: boolean; name: string }[] };

  async function install() {
    if (!name.trim()) { warn = 'Give the mod a name first.'; return; }
    busy = true; warn = '';
    try {
      if (replace && collisions.length) {
        const st = await invoke<{ mods: { id: string; name: string }[] }>('list_mods');
        const ids = st.mods.filter((m) => collisions.includes(m.id)).map((m) => m.id);
        if (ids.length) await invoke('remove_mods', { ids });
      }
      await invoke<string>('install_archive', { path: archPath, name: name.trim(), version, nexusId: nexus });
      const { emit } = await import('@tauri-apps/api/event');
      await emit('mods-changed', {});
      await getCurrentWindow().close();
    } catch (e) { warn = String(e); }
    busy = false;
  }

  async function cancel() {
    await getCurrentWindow().close();
  }
</script>

<div class="mx-auto flex h-full max-w-[780px] flex-col gap-3 overflow-y-auto px-[22px] py-5">
  <div class="text-[14pt] font-semibold">Install mod</div>
  <div class="truncate font-mono text-[12px] text-muted-foreground">{stem}</div>

  {#if collisions.length}
    <div class="rounded-[7px] border border-[#c9a45c]/40 bg-[#c9a45c]/10 px-3 py-2 text-[13px]">
      <div class="font-semibold text-[#dbb977]">Replaces {collisions.length} installed mod{collisions.length === 1 ? '' : 's'} (same files).</div>
      <label class="mt-1 flex cursor-pointer items-center gap-2">
        <input type="checkbox" bind:checked={replace} class="h-4 w-4 accent-[#c9a45c]" />
        <span>Uninstall {collisions.length === 1 ? 'it' : 'them'} and install this instead</span>
      </label>
    </div>
  {/if}
  {#if gvNote}<div class="text-[13px] text-muted-foreground">{gvNote}</div>{/if}

  <div class="grid grid-cols-[110px_minmax(0,1fr)] items-center gap-x-3 gap-y-2">
    <span class="text-sm">Name</span>
    <input bind:value={name} class="rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none focus:border-primary" />
    <span class="text-sm">Version</span>
    <input bind:value={version} class="rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none focus:border-primary" />
    <span class="text-sm">Nexus id</span>
    <input bind:value={nexus} class="rounded-[7px] border border-input bg-card px-2.5 py-1.5 font-mono text-[13px] outline-none focus:border-primary" />
    <span class="text-sm">Section</span>
    <select bind:value={section} class="rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none focus:border-primary">
      <option value="">(none)</option>
      {#each sections as s}<option value={s}>{s}</option>{/each}
    </select>
  </div>

  {#if keybinds}
    <div class="rounded-[7px] border border-border bg-card px-3 py-2 text-[13px]">Contains keybind files ({keybinds}) — review them after installing (input.settings).</div>
  {/if}

  <div class="min-h-0 flex-1 overflow-auto rounded-[7px] border border-border bg-card">
    {#each moves.slice(0, 300) as [, d]}
      <div class="border-b border-border/50 px-3 py-[3px] font-mono text-[11px] text-muted-foreground">{d}</div>
    {/each}
    {#if moves.length > 300}<div class="px-3 py-1 text-[11px] text-muted-foreground">… {moves.length - 300} more</div>{/if}
  </div>

  {#if warn}<p class="font-semibold text-[#dbb977]">{warn}</p>{/if}
  <div class="flex items-center gap-2">
    <span class="flex-1"></span>
    <button onclick={cancel} class="rounded-[7px] border border-border bg-popover px-4 py-2 text-sm hover:bg-accent">Cancel</button>
    <button onclick={install} disabled={busy || !moves.length} class="rounded-[7px] bg-primary px-[18px] py-2 text-sm font-semibold text-primary-foreground hover:brightness-110 disabled:opacity-50">{busy ? 'Installing…' : 'Install'}</button>
  </div>
</div>
