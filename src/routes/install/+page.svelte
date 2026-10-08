<script lang="ts">
  import { onMount, untrack } from 'svelte';
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
  import Button from '$lib/components/button.svelte';

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
  /** Unmapped destinations from install_preview; remapped for display. */
  let rawFiles: string[] = $state([]);
  let warn: string = $state('');
  let busy: boolean = $state(false);
  /** The recorded archive is gone (moved/renamed download, legacy import):
   *  offer to locate the file instead of dead-ending on the error. */
  let missing: string = $state('');

  const kinds = ['Mod', 'DLC', 'Bin', 'Content'];

  onMount(() => {
    archPath = page.url.searchParams.get('path') ?? '';
    if (!archPath) { warn = 'No archive given.'; return; }
    stem = archPath.split('/').pop() ?? archPath;
    load();
  });

  /** Preview the archive; missing files detour to the locate flow. */
  async function load() {
    warn = '';
    missing = '';
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
      rawFiles = prev.moves.map(([, d]) => d);
      addedFiles = await remapFiles();
      const [managed] = await invoke<[string[], string[]]>('find_collisions', { targets: addedFiles });
      collisions = managed;
      const st = await invoke<{ mods: { sep: boolean; id: string; name: string }[] }>('list_mods');
      sections = st.mods.filter((m) => m.sep).map((m) => ({ id: m.id, name: m.name }));
    } catch (e) {
      const msg = String(e);
      // install_preview runs extraction: a stale recorded path (moved or
      // deleted download, legacy `_ModManager` import) lands here. The
      // backend phrases it as "Archive not found"; older builds leak the raw
      // OS error, so match both.
      if (/archive not found|no such file|could not find|not_found/i.test(msg)) {
        missing = archPath;
        warn = `Archive not found — it may have been moved, renamed, or deleted. Locate the file to continue; installing from the new location also repairs the mod's recorded path.`;
      } else {
        warn = msg;
      }
    }
  }

  /** Pick a replacement archive and re-run the preview from it. */
  async function locate() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({
        multiple: false,
        filters: [{ name: 'Mod archive', extensions: ['zip', '7z', 'rar', 'tar', 'gz', 'tgz'] }],
      });
      if (!sel || Array.isArray(sel)) return;
      archPath = sel as string;
      stem = archPath.split('/').pop() ?? archPath;
      roots = [];
      rawFiles = [];
      addedFiles = [];
      collisions = [];
      await load();
    } catch (e) {
      warn = String(e);
    }
  }

  function rootLabel(r: Root): string {
    if (!r.folder) return '(loose files)';
    const base = r.kind === 'Mod' ? 'mods' : r.kind === 'DLC' ? 'dlc' : r.kind === 'Bin' ? 'bin' : 'content';
    return `${base}/${r.folder}/`;
  }

  /** Re-run the backend mapping so the file tree follows the table's edits. */
  async function remapFiles(): Promise<string[]> {
    if (!rawFiles.length) return [];
    const choices = roots.map((r) => ({ prefix: r.prefix, kind: r.kind, folder: r.folder.trim(), files: r.files }));
    console.debug(`[yawmm] remap_roots: ${rawFiles.length} rels, rows=${JSON.stringify(choices)}, first=${rawFiles[0]}`);
    return invoke<string[]>('remap_roots', {
      rels: rawFiles,
      roots: choices,
      modFolder: name,
    })
      .then((out) => {
        console.debug(`[yawmm] remap_roots -> ${out.length} rels, first=${out[0] ?? ''}`);
        return out;
      })
      .catch((e) => {
        // Keep the tree usable, but make the failure visible: a stale backend
        // (missing command) silently looked like "edits do nothing".
        console.error(`[yawmm] remap_roots failed: ${String(e)}`);
        warn = `Could not apply archive table: ${String(e)}`;
        return rawFiles;
      });
  }

  let remapTimer: ReturnType<typeof setTimeout> | undefined;
  function scheduleRemap() {
    clearTimeout(remapTimer);
    remapTimer = setTimeout(async () => {
      addedFiles = await remapFiles();
      addedOpen = true;
    }, 120);
  }

  // Drive invalidation from state, not from control events, so every edit path
  // (select, typed folder, future controls) refreshes the preview.
  $effect(() => {
    const sig = JSON.stringify(roots.map((r) => [r.prefix, r.kind, r.folder]));
    if (!sig || !rawFiles.length) return;
    untrack(scheduleRemap);
  });

  async function install() {
    if (!name.trim()) { warn = 'Give the mod a name first.'; return; }
    // Mod and DLC rows need a folder; content/bin take loose files.
    for (const r of roots) {
      if ((r.kind === 'Mod' || r.kind === 'DLC') && !r.folder.trim()) {
        warn = 'Every Mod and DLC row needs a folder name.';
        return;
      }
    }
    busy = true; warn = '';
    try {
      // Replacement installs are atomic in the backend: the replaced rows'
      // list position (load order) is kept for the new row.
      let replaceIds: string[] = [];
      if (replace && collisions.length) {
        const st = await invoke<{ mods: { id: string }[] }>('list_mods');
        replaceIds = st.mods.filter((m) => collisions.includes(m.id)).map((m) => m.id);
      }
      await invoke<string>('install_roots', {
        path: archPath, name: name.trim(), version, nexusId: nexus, section,
        roots: roots.map((r) => ({ prefix: r.prefix, kind: r.kind, folder: r.folder.trim(), files: r.files })),
        replaceIds,
      });
      const { emit } = await import('@tauri-apps/api/event');
      await emit('mods-changed', {});
      await emit('mod-installed', {});
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
  {#if missing}
    <div class="flex items-center gap-2">
      <Button variant="secondary" size="md" onclick={locate}>Locate archive…</Button>
      <span class="truncate font-mono text-[12px] text-muted-foreground">{missing}</span>
    </div>
  {/if}
  <FormActions cancelLabel="Cancel" onCancel={cancel} primaryLabel={busy ? 'Installing…' : 'Install'} onPrimary={install} primaryDisabled={busy || !roots.length} primaryClass="px-[18px] py-2 text-sm">
    {#snippet cancelIcon()}<X class="inline size-4" />{/snippet}
  </FormActions>
</div>
