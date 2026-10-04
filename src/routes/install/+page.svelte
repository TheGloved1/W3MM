<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Check, ChevronDown, ChevronRight, X } from 'lucide-svelte';
  import FileTree from '$lib/components/file-tree.svelte';
  import FormInput from '$lib/components/form-input.svelte';
  import FormSelect from '$lib/components/form-select.svelte';
  import ExpandableSection from '$lib/components/expandable-section.svelte';
  import ModForm from '$lib/components/mod-form.svelte';
  import ArchiveRootsList from '$lib/components/archive-roots-list.svelte';
  import FormActions from '$lib/components/form-actions.svelte';
  import FilesSection from '$lib/components/files-section.svelte';
  import PageHeader from '$lib/components/page-header.svelte';
  import WarningBanner from '$lib/components/warning-banner.svelte';
  import CollisionNotice from '$lib/components/collision-notice.svelte';

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
  <PageHeader title="Install mod" subtitle={stem} />

  {#if collisions.length}
    <CollisionNotice count={collisions.length} bind:replace />
  {/if}


  <ModForm bind:name bind:version bind:nexus bind:section sections={sections} showSectionActions={false} />

  <ArchiveRootsList roots={roots} kinds={kinds} rootLabel={rootLabel} />

  <FilesSection open={addedOpen} label="Added to the game folder" paths={addedFiles} maxHeight="16rem" onToggle={(o)=> addedOpen = o} />

  <WarningBanner message={warn} />
  <FormActions cancelLabel="Cancel" onCancel={cancel} primaryLabel={busy ? 'Installing…' : 'Install'} onPrimary={install} primaryDisabled={busy || !roots.length} primaryClass="px-[18px] py-2 text-sm">
    {#snippet cancelIcon()}<X class="inline size-4" />{/snippet}
  </FormActions>
</div>
