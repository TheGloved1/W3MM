<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Check, ChevronDown, ChevronRight, X } from 'lucide-svelte';
  import FileTree from '$lib/components/file-tree.svelte';
  import FormInput from '$lib/components/form-input.svelte';
  import FormSelect from '$lib/components/form-select.svelte';
  import PrimaryButton from '$lib/components/primary-button.svelte';
  import ExpandableSection from '$lib/components/expandable-section.svelte';
  import ModForm from '$lib/components/mod-form.svelte';

  type Root = { prefix: string; kind: string; folder: string; files: number };

  let archPath: string = $state('');
  let stem: string = $state('');
  let name: string = $state('');
  let version: string = $state('');
  let nexus: string = $state('');
  let section: string = $state('');
  let sections: { id: string; name: string }[] = $state([]);
  let collisions: string[] = $state([]);
  let replace: boolean = $state(true);
  let roots: Root[] = $state([]);
  let addedOpen: boolean = $state(false);
  let addedFiles: string[] = $state([]);
  let warn: string = $state('');
  let busy: boolean = $state(false);

  const kinds = ['Mod', 'DLC', 'Bin', 'Content'];

  onMount(async () => {
    archPath = page.url.searchParams.get('path') ?? '';
    if (!archPath) { warn = 'No archive given.'; return; }
    stem = archPath.split('/').pop() ?? archPath;
    try {
      const [n, v, nx] = await invoke<[string, string, string]>('parse_archive_name', { filename: stem });
      // Defaults from the download that fetched this file win over filename
      // parsing (original install_archives defaults: page name, meta version,
      // numeric mod id). Manual installs fall back to parsing.
      name = page.url.searchParams.get('name') || n;
      version = page.url.searchParams.get('version') || v;
      nexus = page.url.searchParams.get('nexus') || nx;
      const prev = await invoke<{ roots: Root[]; moves: [string, string][] }>('install_preview', { path: archPath });
      roots = prev.roots;
      if (!roots.length) roots = [{ prefix: '', kind: 'Mod', folder: '', files: 0 }];
      addedFiles = prev.moves.map(([, d]) => d);
      const [managed] = await invoke<[string[], string[]]>('find_collisions', { targets: addedFiles });
      collisions = managed;
      const st = await invoke<{ mods: { sep: boolean; id: string; name: string }[] }>('list_mods');
      sections = st.mods.filter((m) => m.sep).map((m) => ({ id: m.id, name: m.name }));
    } catch (e) { warn = String(e); }
  });

  function rootLabel(r: Root): string {
    if (!r.folder) return '(loose files)';
    const base = r.kind === 'Mod' ? 'mods' : r.kind === 'DLC' ? 'dlc' : r.kind === 'Bin' ? 'bin' : 'content';
    return `${base}/${r.folder}/`;
  }

  async function install() {
    if (!name.trim()) { warn = 'Give the mod a name first.'; return; }
    for (const r of roots) {
      if (!r.folder.trim()) { warn = 'Every archive row needs a folder name.'; return; }
    }
    busy = true; warn = '';
    try {
      if (replace && collisions.length) {
        const st = await invoke<{ mods: { id: string }[] }>('list_mods');
        const ids = st.mods.filter((m) => collisions.includes(m.id)).map((m) => m.id);
        if (ids.length) await invoke('remove_mods', { ids });
      }
      await invoke<string>('install_roots', {
        path: archPath, name: name.trim(), version, nexusId: nexus, section,
        roots: roots.map((r) => ({ prefix: r.prefix, kind: r.kind, folder: r.folder.trim(), files: r.files })),
      });
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
  <div class="truncate text-[13px] text-muted-foreground">{stem}</div>

  {#if collisions.length}
    <div class="rounded-[7px] border border-[#c9a45c]/40 bg-[#c9a45c]/10 px-3 py-2 text-[13px]">
      <div class="font-semibold text-[#dbb977]">Replaces {collisions.length} installed mod{collisions.length === 1 ? '' : 's'} (same files).</div>
      <label class="mt-1 flex cursor-pointer items-center gap-2">
        <input type="checkbox" bind:checked={replace} class="h-4 w-4 accent-[#c9a45c]" />
        <span>Uninstall {collisions.length === 1 ? 'it' : 'them'} and install this instead</span>
      </label>
    </div>
  {/if}


  <ModForm bind:name bind:version bind:nexus bind:section sections={sections} showSectionActions={false} />

  <div class="rounded-[7px] border border-border bg-card px-3 py-2">
    <div class="flex items-center gap-1 py-1 text-sm font-semibold">
      <ChevronDown class="size-4 text-muted-foreground" /> Archive contents
    </div>
    <div class="grid grid-cols-[minmax(0,1fr)_130px_minmax(0,1fr)] gap-2 px-1 pb-1 text-[12px] text-muted-foreground">
      <span class="pl-7">From archive</span><span>Type</span><span>Folder name</span>
    </div>
    {#each roots as r}
      <div class="grid grid-cols-[minmax(0,1fr)_130px_minmax(0,1fr)] items-center gap-2 border-t border-border/50 px-1 py-1.5">
        <span class="flex min-w-0 items-center gap-2">
          <span class="flex h-[18px] w-[18px] items-center justify-center rounded-[4px] border-[1.5px] border-[#c9a45c] bg-[#c9a45c] text-[#1c2127]"><Check class="size-3" /></span>
          <span class="truncate font-mono text-[12px]">{rootLabel(r)}</span>
        </span>
        <select bind:value={r.kind} class="rounded-[7px] border border-input bg-background px-2 py-1 text-[13px] outline-none focus:border-primary">
          {#each kinds as k}<option value={k}>{k}</option>{/each}
        </select>
        <input bind:value={r.folder} class="rounded-[7px] border border-input bg-background px-2 py-1 font-mono text-[12px] outline-none focus:border-primary" />
      </div>
    {/each}
  </div>

  <ExpandableSection open={addedOpen} label="Added to the game folder" count={addedFiles.length} onToggle={() => addedOpen = !addedOpen}>
    <FileTree paths={addedFiles} maxHeight="16rem" class="rounded-t-none border-t-0" />
  </ExpandableSection>

  {#if warn}<p class="font-semibold text-[#dbb977]">{warn}</p>{/if}
  <div class="flex items-center gap-2">
    <span class="flex-1"></span>
    <button onclick={cancel} class="rounded-[7px] border border-border bg-popover px-4 py-2 text-sm hover:bg-accent"><X class="inline size-4" /> Cancel</button>
    <PrimaryButton onclick={install} disabled={busy || !roots.length} class="px-[18px] py-2 text-sm">{busy ? 'Installing…' : 'Install'}</PrimaryButton>
  </div>
</div>
