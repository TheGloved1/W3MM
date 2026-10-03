<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ChevronDown, ChevronRight, X } from "lucide-svelte";
  import FileTree from "$lib/components/file-tree.svelte";
  import FormInput from "$lib/components/form-input.svelte";

  type Row = {
    id: string;
    sep: boolean;
    name: string;
    version: string;
    nexus: string;
    archive: string;
    updated: number;
  };

  let id: string = $state("");
  let name: string = $state("");
  let version: string = $state("");
  let nexus: string = $state("");
  let archive: string = $state("");
  let updated: number = $state(0);
  let section: string = $state("");
  let sections: { id: string; name: string }[] = $state([]);
  let files: string[] = $state([]);
  let filesOpen: boolean = $state(false);
  let warn: string = $state("");

  onMount(async () => {
    id = page.url.searchParams.get("id") ?? "";
    if (!id) {
      warn = "No mod given.";
      return;
    }
    await reload();
  });

  async function reload() {
    const st = await invoke<{ mods: Row[] }>("list_mods");
    const m = st.mods.find((r) => r.id === id);
    if (!m) {
      warn = "Mod not found.";
      return;
    }
    name = m.name;
    version = m.version;
    nexus = m.nexus;
    archive = m.archive;
    updated = m.updated;
    sections = st.mods
      .filter((r) => r.sep)
      .map((r) => ({ id: r.id, name: r.name }));
    section = sectionOf(st.mods, id);
    files = await invoke<string[]>("staged_files", { id }).catch(() => []);
  }

  function sectionOf(rows: Row[], mid: string): string {
    let cur = "";
    for (const r of rows) {
      if (r.sep) cur = r.id;
      else if (r.id === mid) return cur;
    }
    return cur;
  }

  function fmtWhen(ts: number): string {
    try {
      return new Date(ts * 1000).toLocaleDateString("en-GB", {
        day: "2-digit",
        month: "short",
        year: "numeric",
      });
    } catch {
      return "";
    }
  }

  async function newSection() {
    const n = prompt("Section name", "New section");
    if (!n) return;
    const row = await invoke<{ id: string; name: string }>("add_separator", {
      index: 9999,
      name: n,
    });
    sections = [...sections, { id: row.id, name: row.name }];
    section = row.id;
  }

  async function removeSection() {
    if (!section) return;
    if (!confirm("Remove this section? Its mods stay in the list.")) return;
    await invoke("remove_section_cmd", { sepId: section });
    section = "";
    await reload();
  }

  async function save() {
    if (!name.trim()) {
      warn = "Give the mod a name first.";
      return;
    }
    try {
      const st = await invoke<{ mods: Row[] }>("list_mods");
      const before = sectionOf(st.mods, id);
      await invoke("edit_mod", {
        id,
        name: name.trim(),
        version,
        nexusId: nexus,
        section: "",
      });
      if (section !== before)
        await invoke("move_to_section", { id, sepId: section });
      const { emit } = await import("@tauri-apps/api/event");
      await emit("mods-changed", {});
      await getCurrentWindow().close();
    } catch (e) {
      warn = String(e);
    }
  }

  async function cancel() {
    await getCurrentWindow().close();
  }
</script>

<div
  class="mx-auto flex h-full max-w-[820px] flex-col gap-3 overflow-y-auto px-[22px] py-5"
>
  <div class="text-[14pt] font-semibold">Edit mod</div>
  <div class="truncate text-[13px] text-muted-foreground">
    {archive} &nbsp;·&nbsp; installed {fmtWhen(updated)}
  </div>

  <div
    class="grid grid-cols-[auto_minmax(0,1fr)_auto_110px_auto_110px] items-center gap-x-3 gap-y-2"
  >
    <span class="text-sm">Name in list</span>
    <FormInput bind:value={name} />
    <span class="text-sm">Version</span>
    <FormInput bind:value={version} mono width="110px" />
    <span class="text-sm">Nexus ID</span>
    <FormInput bind:value={nexus} mono width="110px" />
    <span class="text-sm">Section</span>
    <select
      bind:value={section}
      class="rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none focus:border-primary"
    >
      <option value="">No section</option>
      {#each sections as s}<option value={s.id}>{s.name}</option>{/each}
    </select>
    <span></span>
    <button
      onclick={newSection}
      class="rounded-[7px] border border-border bg-popover px-4 py-[7px] text-sm hover:bg-accent"
      >New section…</button
    >
    <span></span>
    <button
      onclick={removeSection}
      disabled={!section}
      class="rounded-[7px] border border-border bg-popover px-4 py-[7px] text-sm hover:bg-accent disabled:opacity-50"
      >Remove section</button
    >
  </div>

  <div class="flex flex-col">
    <button
      onclick={() => (filesOpen = !filesOpen)}
      class="flex items-center gap-2 rounded-[7px] border border-border bg-card px-3 py-2 text-left text-sm hover:bg-accent/40 {filesOpen
        ? 'rounded-b-none border-b-0'
        : ''}"
    >
      <span class="text-muted-foreground">{#if filesOpen}<ChevronDown class="size-4" />{:else}<ChevronRight class="size-4" />{/if}</span>
      <span class="flex-1">Installed files</span>
      {#if !filesOpen}
        <span class="text-muted-foreground">{files.length} files</span>
      {/if}
    </button>
    {#if filesOpen}
      <FileTree
        paths={files}
        maxHeight="12rem"
        class="rounded-t-none border-t-0"
      />
    {/if}
  </div>

  {#if warn}<p class="font-semibold text-[#dbb977]">{warn}</p>{/if}
  <div class="flex items-center gap-2">
    <span class="flex-1"></span>
    <button
      onclick={cancel}
      class="rounded-[7px] border border-border bg-popover px-4 py-2 text-sm hover:bg-accent"
      ><X class="size-4" /> Cancel</button
    >
    <button
      onclick={save}
      class="rounded-[7px] bg-primary px-[18px] py-2 text-sm font-semibold text-primary-foreground hover:brightness-110"
      >Save</button
    >
  </div>
</div>
