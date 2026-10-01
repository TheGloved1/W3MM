<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import type { AppState, MadeFor, QueueItem } from '$lib/types';
  import { loadConfigNative } from '$lib/config';

  let appState: AppState | null = $state(null);
  let clashMap: Record<string, string[]> = $state({});
  let annotMap: Record<string, string[]> = $state({});
  let madeMap: Record<string, MadeFor> = $state({});
  let infoMap: Record<string, { scripts: { file: string; with: string[] }[]; xmls: { file: string; with: string[] }[]; lost: number }> = $state({});
  let filesMap: Record<string, string[]> = $state({});
  let openFiles: string | null = $state(null);
  let unmanaged: string[] = $state([]);
  let hits: { id: string; name: string; local: string; remote: string }[] = $state([]);

  let error: string = $state('');
  let status: string = $state('');
  let busy: string = $state('');
  let gameDir: string = $state('');
  let prefix: string = $state('');
  let filter: string = $state('');
  let menuOpen: boolean = $state(false);
  let collapsed: Record<string, boolean> = $state({});

  // downloads panel
  let dlOpen: boolean = $state(false);
  let queue: QueueItem[] = $state([]);
  let quotaText: string = $state('');
  let nxm: string = $state('');
  let archPath: string = $state('');
  let archNames: string[] = $state([]);
  let archMoves: [string, string][] = $state([]);

  const filtering = $derived(filter.trim().length > 0);
  const q = $derived(filter.trim().toLowerCase());

  function modById(id: string) { return appState?.mods.find((m) => m.id === id); }
  function modName(id: string) { return modById(id)?.name ?? id; }

  function clashCount(id: string): number {
    return Object.values(clashMap).filter((ids) => ids.includes(id) && ids.length > 1).length;
  }
  function annotCount(name: string): number {
    return Object.values(annotMap).filter((mods) => mods.includes(name)).length;
  }
  function sharedScripts(id: string) { return infoMap[id]?.scripts ?? []; }
  function sharedXmls(id: string) { return infoMap[id]?.xmls ?? []; }
  function lostCount(id: string) { return infoMap[id]?.lost ?? 0; }
  function otherClashes(id: string): string[] {
    const mine = new Set<string>();
    for (const [path, ids] of Object.entries(clashMap)) {
      if (!ids.includes(id) || ids.length < 2) continue;
      const low = path.toLowerCase();
      if (low.endsWith('.ws') || low.endsWith('.wss')) continue;
      if (low.endsWith('.xml') && low.includes('bin/')) continue;
      mine.add(path);
    }
    return [...mine];
  }

  function prioOf(id: string): number {
    if (!appState) return 0;
    const i = appState.priority.indexOf(id);
    return i < 0 ? 0 : i + 1;
  }

  function fmtDate(ts: number): string {
    if (!ts) return '';
    try { return new Date(ts * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' }); }
    catch { return ''; }
  }

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
      else status = 'Set the game folder in Settings…';
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
    annotMap = await invoke<Record<string, string[]>>('annotation_clashes').catch(() => ({}));
    infoMap = await invoke<typeof infoMap>('analysis_summary').catch(() => ({}));
    unmanaged = await invoke<string[]>('unmanaged_mods').catch(() => []);
    madeMap = {};
    if (appState) {
      for (const m of appState.mods.filter((x) => !x.sep)) {
        invoke<MadeFor>('made_for', { id: m.id }).then((r) => { madeMap[m.id] = r; }).catch(() => {});
      }
      for (const m of appState.mods.filter((x) => x.sep && x.collapsed)) collapsed[m.id] = true;
    }
    const [qt] = await invoke<[string, string, number]>('quota').catch(() => ['', '', 0] as [string, string, number]);
    quotaText = qt;
  }

  function flash(msg: string) { status = msg; }

  async function toggle(id: string, on: boolean) {
    error = '';
    await invoke('set_enabled', { ids: [id], on: !on });
    await refresh();
    await deploy(true);
  }

  async function deploy(quiet = false) {
    if (!quiet) { busy = 'Deploying…'; error = ''; }
    try {
      const running = await invoke<boolean>('game_running');
      if (running) { error = 'Close the game before deploying'; busy = ''; return; }
      const files = await invoke<string[]>('deploy');
      busy = '';
      flash(`Deployed ${files.length} file${files.length === 1 ? '' : 's'}`);
      await refresh();
      try {
        const { sendNotification } = await import('@tauri-apps/plugin-notification');
        sendNotification({ title: 'W3 Mod Manager', body: `Deployed ${files.length} files` });
      } catch {}
    } catch (e) { error = String(e); busy = ''; }
  }

  async function setPrio(id: string, ev: Event) {
    const n = Number((ev.target as HTMLInputElement).value);
    if (!n) return;
    await invoke('set_priority', { id, number: n });
    await refresh();
    await deploy(true);
  }

  async function removeMod(id: string) {
    const m = modById(id);
    if (!m || !confirm(`Uninstall “${m.name}” and its staged files?`)) return;
    await invoke('remove_mods', { ids: [id] });
    await refresh();
    await deploy(true);
  }

  async function addSection() {
    menuOpen = false;
    const name = prompt('Section name', 'New section');
    if (!name || !appState) return;
    await invoke('add_separator', { index: appState.mods.length, name });
    await refresh();
  }

  async function editRow(id: string) {
    const m = modById(id);
    if (!m) return;
    const name = prompt('Mod name', m.name);
    if (name === null) return;
    const version = prompt('Version', m.version) ?? m.version;
    const nexus = prompt('Nexus id', m.nexus) ?? m.nexus;
    await invoke('edit_mod', { id, name, version, nexusId: nexus, section: '' });
    await refresh();
  }

  async function toggleFiles(id: string) {
    if (openFiles === id) { openFiles = null; return; }
    openFiles = id;
    if (!filesMap[id]) filesMap[id] = await invoke<string[]>('staged_files', { id }).catch(() => []);
  }

  async function toggleCollapse(id: string) {
    collapsed[id] = !collapsed[id];
  }

  function sepOf(id: string): string | null {
    let cur: string | null = null;
    if (!appState) return null;
    for (const r of appState.mods) {
      if (r.sep) cur = r.id;
      else if (r.id === id) return cur;
    }
    return cur;
  }

  function isHiddenByCollapse(id: string): boolean {
    if (filtering) return false;
    const s = sepOf(id);
    return s !== null && !!collapsed[s];
  }

  function countMembers(sepId: string): number {
    if (!appState) return 0;
    let counting = false;
    let n = 0;
    for (const r of appState.mods) {
      if (r.sep) {
        if (counting) break;
        if (r.id === sepId) counting = true;
      } else if (counting) {
        if (!filtering || r.name.toLowerCase().includes(q)) n++;
      }
    }
    return n;
  }

  type Chip = { text: string; tip: string; cls: string };

  function chipsFor(m: { id: string; name: string }): Chip[] {
    const chips: Chip[] = [];
    const bad = 'bg-[#e3735f]/15 text-[#e3735f]';
    const warn = 'bg-[#c9a45c]/15 text-[#c9a45c]';
    const nw = 'bg-[#b5d95a]/15 text-[#b5d95a]';
    const files = 'bg-[#86b0cf]/15 text-[#86b0cf]';
    const sc = sharedScripts(m.id);
    if (sc.length) chips.push({ text: `⚑ ${sc.length}`, tip: sc.map((s) => `${s.file} — with ${s.with.join(', ')}`).join('\n'), cls: bad });
    const xm = sharedXmls(m.id);
    if (xm.length) chips.push({ text: `☰ ${xm.length}`, tip: xm.map((s) => `${s.file} — with ${s.with.join(', ')}`).join('\n'), cls: bad });
    const oc = otherClashes(m.id);
    if (oc.length) chips.push({ text: `≠ ${oc.length}`, tip: oc.join('\n'), cls: warn });
    const an = annotCount(m.name);
    if (an) chips.push({ text: `@ ${an}`, tip: 'Same RedKit symbol added by two mods', cls: bad });
    const lost = lostCount(m.id);
    if (lost) chips.push({ text: `⧉ ${lost}`, tip: `${lost} file${lost === 1 ? '' : 's'} overridden by higher mods`, cls: files });
    const made = madeMap[m.id];
    if (made?.short) chips.push({ text: made.short, tip: made.label || made.short, cls: made.status === 'classic' ? bad : warn });
    const h = hits.find((hh) => hh.id === m.id);
    if (h) chips.push({ text: '↑', tip: `Update on Nexus: ${h.local} → ${h.remote}`, cls: nw });
    return chips;
  }

  async function pickArchives() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({ multiple: true, filters: [{ name: 'Mod archive', extensions: ['zip', '7z', 'rar', 'tar', 'gz', 'tgz'] }] });
      const paths: string[] = Array.isArray(sel) ? sel as string[] : sel ? [sel as string] : [];
      for (const p of paths) {
        await invoke('open_tool_window', { kind: 'install', query: `path=${encodeURIComponent(p)}`, path: p });
      }
    } catch {}
  }

  async function installPath(p: string) {
    busy = 'Installing…'; error = '';
    try {
      const base = p.split('/').pop() ?? p;
      const [n, v, nx] = await invoke<[string, string, string]>('parse_archive_name', { filename: base });
      const name = prompt('Mod name', n) ?? n;
      await invoke('install_archive', { path: p, name, version: v, nexusId: nx });
      archPath = '';
      await refresh();
      await deploy(true);
      flash(`Installed “${name}”`);
    } catch (e) { error = String(e); }
    busy = '';
  }

  async function importThem() {
    if (!unmanaged.length) return;
    busy = 'Importing…';
    try {
      await invoke('import_unmanaged', { rels: unmanaged });
      await refresh();
      await deploy(true);
      flash(`Imported ${unmanaged.length} mod${unmanaged.length === 1 ? '' : 's'}`);
    } catch (e) { error = String(e); }
    busy = '';
  }

  async function checkUpdates() {
    menuOpen = false;
    busy = 'Checking Nexus…'; error = '';
    try {
      const cfg = await loadConfigNative();
      hits = await invoke<typeof hits>('check_updates', { apiKey: cfg.nexusKey });
      flash(hits.length ? `${hits.length} update${hits.length === 1 ? '' : 's'} available` : 'All tracked mods are current');
      const [qt] = await invoke<[string, string, number]>('quota').catch(() => ['', '', 0] as [string, string, number]);
      quotaText = qt;
    } catch (e) { error = String(e); }
    busy = '';
  }

  async function play() {
    if (gameDir.toLowerCase().includes('steamapps')) {
      try {
        const { open } = await import('@tauri-apps/plugin-shell');
        await open('steam://rungameid/292030');
      } catch (e) { error = String(e); }
    } else {
      flash('Not a Steam install — start it from your launcher.');
    }
  }

  async function openPath(kind: string) {
    menuOpen = false;
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      if (kind === 'game') await open(gameDir);
      else if (kind === 'settings') {
        const dir = await invoke<string>('settings_dir_path');
        await open(dir);
      } else {
        const dir = await invoke<string>('settings_dir_path');
        await open(`${dir}/${kind}`);
      }
    } catch (e) { error = String(e); }
  }

  function openResolver() { menuOpen = false; invoke('open_tool_window', { kind: 'resolver', query: '', path: '' }); }
  function openSettings() { menuOpen = false; invoke('open_tool_window', { kind: 'setup', query: '', path: '' }); }

  // ---- downloads panel ----
  async function notify(title: string, body: string) {
    try {
      const { sendNotification } = await import('@tauri-apps/plugin-notification');
      sendNotification({ title, body });
    } catch {}
  }

  async function dlNxm(preset?: string) {
    const url = preset ?? nxm;
    if (!url) return;
    nxm = url;
    error = '';
    try {
      const cfg = await loadConfigNative();
      if (!cfg.nexusKey) { error = 'Set Nexus API key in Settings first'; return; }
      flash('Resolving Nexus link…');
      const dest = await invoke<string>('download_nxm', { url, apiKey: cfg.nexusKey, destDir: '/tmp' });
      flash(`Downloaded → ${dest.split('/').pop()}`);
      archPath = dest;
      await notify('W3 Mod Manager', dest.split('/').pop() ?? 'download done');
    } catch (e) { error = String(e); }
  }

  async function pumpNext() {
    const next = queue.find((qq) => qq.status === 'queued');
    if (!next) return;
    try {
      await invoke('queue_pump', { id: next.id, destDir: '/tmp', apiKey: (await loadConfigNative()).nexusKey });
      queue = await invoke<QueueItem[]>('queue_list');
    } catch (e) { error = String(e); queue = await invoke<QueueItem[]>('queue_list'); }
  }

  async function archList() {
    error = '';
    try {
      if (!archPath) {
        const { open } = await import('@tauri-apps/plugin-dialog');
        const sel = await open({ multiple: false, filters: [{ name: 'Mod archive', extensions: ['zip', '7z', 'rar', 'tar', 'gz', 'tgz'] }] });
        if (typeof sel === 'string') archPath = sel;
      }
      if (archPath) archNames = await invoke<string[]>('list_archive', { path: archPath });
    } catch (e) { error = String(e); }
  }

  async function archPreview() {
    error = ''; archMoves = [];
    try {
      if (!archPath) return;
      const plan = await invoke<{ moves: [string, string][]; docs: string[] }>('preview_archive', { path: archPath });
      archMoves = plan.moves.slice(0, 200);
      flash(`${plan.moves.length} targets, ${plan.docs.length} docs`);
    } catch (e) { error = String(e); }
  }

  onMount(() => {
    boot();
    let unlisten: (() => void) | undefined;
    let unlistenP: (() => void) | undefined;
    let unlistenD: (() => void) | undefined;
    let unlistenM: (() => void) | undefined;
    (async () => {
      try {
        queue = await invoke<QueueItem[]>('queue_list').catch(() => []);
        const { getCurrent } = await import('@tauri-apps/plugin-deep-link');
        const cur = await getCurrent().catch(() => []);
        if (cur?.length) { dlOpen = true; await dlNxm(cur[0]); }
        unlisten = await listen<string>('nxm-url', async (e) => { dlOpen = true; await dlNxm(e.payload); });
        unlistenP = await listen<{ id: string; done: number; total: number }>('download-progress', (e) => {
          queue = queue.map((qq) => qq.id === e.payload.id ? { ...qq, done: e.payload.done, total: e.payload.total, status: 'active' } : qq);
        });
        unlistenD = await listen<{ id: string; path: string }>('download-done', async (e) => {
          queue = await invoke<QueueItem[]>('queue_list');
          archPath = e.payload.path;
          flash(`Downloaded → ${e.payload.path.split('/').pop()}`);
        });
        unlistenM = await listen('mods-changed', async () => {
          await refresh();
          await deploy(true);
        });
      } catch {}
    })();
    function onDocClick() { menuOpen = false; }
    document.addEventListener('click', onDocClick);
    return () => { unlisten?.(); unlistenP?.(); unlistenD?.(); unlistenM?.(); document.removeEventListener('click', onDocClick); };
  });
</script>

<div class="flex h-full min-h-0">
  <!-- main column -->
  <div class="flex min-w-0 flex-1 flex-col gap-[14px] px-[22px] pt-[18px] pb-[12px]">
    <!-- header -->
    <div class="flex items-center gap-[10px]">
      <div class="flex-1 leading-tight min-w-0">
        <div class="text-[19pt] font-semibold tracking-tight">The Witcher 3</div>
        <div class="text-[13px] text-muted-foreground truncate">
          {#if appState}{appState.mods.filter((m) => !m.sep).length} mods{#if gameDir} · {gameDir}{/if}{:else}W3 Mod Manager{/if}
        </div>
      </div>
      <input
        bind:value={filter}
        placeholder="Filter mods"
        class="w-[230px] rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground/70 focus:border-primary"
      />
      <button onclick={play} class="rounded-[7px] border border-border bg-popover px-4 py-[7px] text-sm hover:bg-accent">▶ Play</button>
      <button onclick={pickArchives} class="rounded-[7px] bg-primary px-[18px] py-2 text-sm font-semibold text-primary-foreground hover:brightness-110">Install mods</button>
      <div class="relative">
        <button onclick={(e) => { e.stopPropagation(); menuOpen = !menuOpen; }} class="rounded-[7px] border border-border bg-popover px-3 py-[7px] text-sm hover:bg-accent" aria-label="More">•••</button>
        {#if menuOpen}
          <div role="menu" tabindex="-1" class="absolute right-0 z-30 mt-1 w-56 rounded-[7px] border border-border bg-popover py-1 shadow-xl" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
            <button onclick={addSection} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">New section</button>
            <button onclick={checkUpdates} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">Check Nexus for updates</button>
            <button onclick={openResolver} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">Script decisions…</button>
            <button onclick={importThem} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent" disabled={!unmanaged.length}>Import existing mods{#if unmanaged.length} ({unmanaged.length}){/if}</button>
            <div class="my-1 border-t border-border"></div>
            <button onclick={() => openPath('game')} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">Open: Game folder</button>
            <button onclick={() => openPath('settings')} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">Open: Settings folder</button>
            <button onclick={() => openPath('mods.settings')} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">Open: mods.settings</button>
            <button onclick={() => openPath('input.settings')} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">Open: input.settings</button>
            <div class="my-1 border-t border-border"></div>
            <button onclick={openSettings} class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent">Settings…</button>
          </div>
        {/if}
      </div>
    </div>

    <!-- banners -->
    {#if unmanaged.length}
      <div class="flex items-center gap-2 rounded-[7px] border border-border bg-card px-[14px] py-2 text-sm">
        <span class="flex-1">{unmanaged.length} mod folder{unmanaged.length === 1 ? '' : 's'} in the game {unmanaged.length === 1 ? 'is' : 'are'} not managed yet.</span>
        <button onclick={importThem} class="rounded-[7px] border border-border bg-popover px-3 py-1 text-sm hover:bg-accent">Import them</button>
      </div>
    {/if}
    {#if hits.length}
      <div class="flex items-center gap-2 rounded-[7px] border border-border bg-card px-[14px] py-2 text-sm">
        <span class="flex-1 text-[#b5d95a]">{hits.length} update{hits.length === 1 ? '' : 's'} on Nexus: {hits.slice(0, 3).map((h) => `${h.name} → ${h.remote}`).join(' · ')}{hits.length > 3 ? ' …' : ''}</span>
      </div>
    {/if}

    <!-- list -->
    <div class="min-h-0 flex-1 overflow-auto rounded-[7px] border border-border bg-card">
      <div class="grid grid-cols-[90px_minmax(0,1fr)_120px_minmax(0,1.2fr)_110px] items-center gap-2 border-b border-border px-3 py-2 text-[11px] uppercase tracking-wide text-muted-foreground sticky top-0 bg-card z-10">
        <span class="text-center">Priority</span><span>Mod</span><span>Version</span><span>Status</span><span>Installed</span>
      </div>
      {#if !appState}
        <div class="flex h-64 items-center justify-center px-6 text-center text-[12pt] text-muted-foreground whitespace-pre-line">Set the game folder in Settings…</div>
      {:else if !appState.mods.length}
        <div class="flex h-64 items-center justify-center px-6 text-center text-[12pt] text-muted-foreground whitespace-pre-line">{"No mods yet\n\nClick Install mods, or drop .zip / .7z / .rar files here"}</div>
      {:else}
        {#each appState.mods as m}
          {#if m.sep}
            {#if !filtering}
              <button onclick={() => toggleCollapse(m.id)} class="grid w-full grid-cols-1 items-center gap-2 border-b border-border px-3 text-left hover:bg-accent/50" style="min-height:40px">
                <span class="text-[13px] font-semibold text-muted-foreground">{collapsed[m.id] ? '›' : '⌄'} {m.name} <span class="font-normal">({countMembers(m.id)})</span></span>
              </button>
            {/if}
          {:else if !filtering || m.name.toLowerCase().includes(q)}
            {#if !(filtering ? false : isHiddenByCollapse(m.id))}
            <div class="grid grid-cols-[90px_minmax(0,1fr)_120px_minmax(0,1.2fr)_110px] items-center gap-2 border-b border-border/60 px-3 hover:bg-accent/30" style="min-height:40px">
              <span class="flex justify-center">
                {#if clashCount(m.id) || sharedScripts(m.id).length || sharedXmls(m.id).length}
                  <input type="number" min="1" value={prioOf(m.id)} onchange={(e) => setPrio(m.id, e)} title="Priority — 1 wins"
                    class="w-[52px] rounded-full border border-primary/60 bg-primary/15 px-1 py-[3px] text-center text-[13px] font-semibold text-primary outline-none" />
                {:else}
                  <span class="text-muted-foreground/50">–</span>
                {/if}
              </span>
              <span class="flex min-w-0 items-center gap-2">
                <input type="checkbox" checked={m.enabled} onchange={() => toggle(m.id, m.enabled)} aria-label="enabled for {m.name}"
                  class="h-[18px] w-[18px] shrink-0 cursor-pointer appearance-none rounded-[4px] border-[1.5px] border-[#4a535e] bg-transparent checked:border-[#c9a45c] checked:bg-[#c9a45c] checked:bg-[url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 18 18%22><path d=%22M5.2 9.3l2.5 2.5 5.1-5.3%22 fill=%22none%22 stroke=%22%231c2127%22 stroke-width=%222.1%22 stroke-linecap=%22round%22 stroke-linejoin=%22round%22/></svg>')] checked:bg-center checked:bg-no-repeat" />
                <span class="min-w-0">
                  <span class="block truncate text-sm">{m.name}</span>
                  {#if openFiles === m.id}
                    <span class="block max-h-32 overflow-auto font-mono text-[10px] text-muted-foreground whitespace-pre-wrap">{(filesMap[m.id] ?? ['…']).join('\n')}</span>
                  {/if}
                </span>
              </span>
              <span class="truncate text-[13px] text-muted-foreground">{m.version}</span>
              <span class="flex flex-wrap gap-1 py-1">
                {#each chipsFor(m) as c}
                  <span title={c.tip} class="rounded-full px-2 py-[1px] text-[11px] font-semibold {c.cls}">{c.text}</span>
                {/each}
                <button onclick={() => toggleFiles(m.id)} title="staged files" class="text-[11px] text-muted-foreground hover:text-foreground">{openFiles === m.id ? '▴' : '▾'}</button>
              </span>
              <span class="flex items-center gap-1 text-[13px] text-muted-foreground">
                <span>{fmtDate(m.updated)}</span>
                <button onclick={() => editRow(m.id)} title="Edit" class="rounded px-1 hover:bg-accent hover:text-foreground">✎</button>
                <button onclick={() => removeMod(m.id)} title="Uninstall" class="rounded px-1 hover:bg-accent hover:text-foreground">🗑</button>
              </span>
            </div>
            {/if}
          {/if}
        {/each}
      {/if}
    </div>

    <!-- footer -->
    <div class="flex items-center gap-2">
      <span class="min-w-0 flex-1 text-[13px] text-muted-foreground break-words" title={error || status}>{error ? `⚠ ${error}` : status}</span>
      {#if quotaText}<span class="text-[12px] text-muted-foreground">{quotaText}</span>{/if}
      {#if busy}
        <span class="text-[12px] text-muted-foreground">{busy}</span>
        <span class="h-[6px] w-[180px] overflow-hidden rounded bg-muted"><span class="block h-full w-1/3 animate-pulse rounded bg-primary"></span></span>
      {/if}
      <button onclick={() => { dlOpen = !dlOpen; }} title="Downloads" class="rounded-[7px] border px-3 py-1.5 text-sm {dlOpen ? 'border-primary bg-primary/15 text-primary' : 'border-border bg-popover text-muted-foreground hover:text-foreground hover:bg-accent'}">⤓{queue.filter((qq) => qq.status === 'active' || qq.status === 'queued').length ? ` (${queue.filter((qq) => qq.status === 'active' || qq.status === 'queued').length})` : ''}</button>
    </div>
  </div>

  <!-- downloads slide-over -->
  {#if dlOpen}
    <div class="flex w-[330px] max-w-[80vw] shrink-0 flex-col gap-3 overflow-y-auto border-l border-border bg-card px-4 py-[18px]">
      <div class="flex items-center gap-1.5">
        <div class="flex-1 leading-tight">
          <div class="text-[13pt] font-semibold">Downloads</div>
          {#if queue.length}<div class="text-[12px] text-muted-foreground">{queue.length} file{queue.length === 1 ? '' : 's'}</div>{/if}
        </div>
        <button onclick={async () => { try { const { open } = await import('@tauri-apps/plugin-shell'); await open('/tmp'); } catch {} }} class="rounded px-2 py-1 text-[13px] text-muted-foreground hover:bg-accent hover:text-foreground">Open folder</button>
      </div>
      <div class="flex gap-2">
        <input bind:value={nxm} placeholder="nxm:// link" class="min-w-0 flex-1 rounded-[7px] border border-input bg-background px-2 py-1.5 font-mono text-[12px] outline-none focus:border-primary" />
        <button onclick={() => dlNxm()} class="rounded-[7px] border border-border bg-popover px-3 py-1.5 text-sm hover:bg-accent">Get</button>
      </div>
      {#each queue as qq}
        <div class="rounded-[7px] border border-border bg-background/60 p-2 text-[12px]">
          <div class="truncate font-mono">{qq.filename}</div>
          <div class="mt-1 h-[5px] overflow-hidden rounded bg-muted">
            <div class="h-full rounded bg-primary" style="width:{qq.total ? Math.round(100 * qq.done / qq.total) : 0}%"></div>
          </div>
          <div class="mt-1 flex items-center gap-2 text-muted-foreground">
            <span class="flex-1">{qq.status}{#if qq.total} · {Math.round(100 * qq.done / Math.max(1, qq.total))}%{/if}{#if qq.error} · {qq.error}{/if}</span>
            {#if qq.status === 'queued'}<button class="hover:text-foreground" onclick={pumpNext}>Start</button>{/if}
            {#if qq.status === 'active'}<button class="hover:text-foreground" onclick={async () => { await invoke('queue_pause', { id: qq.id, paused: true }); queue = await invoke<QueueItem[]>('queue_list'); }}>Pause</button>{/if}
            {#if qq.status === 'paused'}<button class="hover:text-foreground" onclick={async () => { await invoke('queue_pause', { id: qq.id, paused: false }); queue = await invoke<QueueItem[]>('queue_list'); }}>Resume</button>{/if}
            <button class="hover:text-foreground" onclick={async () => { await invoke('queue_cancel', { id: qq.id }); queue = await invoke<QueueItem[]>('queue_list'); }}>✕</button>
          </div>
        </div>
      {/each}
      {#if !queue.length}
        <p class="text-[13px] text-muted-foreground">Click “Mod Manager Download” on a Witcher 3 mod's Nexus page. It downloads here, then install it below.</p>
      {/if}
      <div class="border-t border-border pt-3">
        <div class="mb-2 text-[13px] font-semibold">Install from file</div>
        <input bind:value={archPath} placeholder="/path/to/mod.zip" class="mb-2 w-full rounded-[7px] border border-input bg-background px-2 py-1.5 font-mono text-[12px] outline-none focus:border-primary" />
        <div class="flex gap-2">
          <button onclick={archList} class="rounded-[7px] border border-border bg-popover px-2 py-1 text-[13px] hover:bg-accent">List</button>
          <button onclick={archPreview} class="rounded-[7px] border border-border bg-popover px-2 py-1 text-[13px] hover:bg-accent">Preview</button>
          <button onclick={() => archPath && installPath(archPath)} class="rounded-[7px] bg-primary px-2 py-1 text-[13px] font-semibold text-primary-foreground hover:brightness-110" disabled={!archPath}>Install</button>
        </div>
        {#if archNames.length}
          <pre class="mt-2 max-h-40 overflow-auto rounded bg-well p-2 font-mono text-[11px] whitespace-pre-wrap">{archNames.slice(0, 200).join('\n')}</pre>
        {/if}
        {#if archMoves.length}
          <pre class="mt-2 max-h-40 overflow-auto rounded bg-well p-2 font-mono text-[11px] whitespace-pre-wrap">{archMoves.map(([, d]) => d).join('\n')}</pre>
        {/if}
      </div>
    </div>
  {/if}
</div>
