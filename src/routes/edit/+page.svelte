<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ChevronDown, ChevronRight, X } from "lucide-svelte";
  import FileTree from "$lib/components/file-tree.svelte";
  import FormInput from "$lib/components/form-input.svelte";
  import FormSelect from "$lib/components/form-select.svelte";
  import ExpandableSection from "$lib/components/expandable-section.svelte";
  import PrimaryButton from "$lib/components/primary-button.svelte";
  import PopoverButton from "$lib/components/popover-button.svelte";
  import ModForm from "$lib/components/mod-form.svelte";
  import FormActions from "$lib/components/form-actions.svelte";
  import FilesSection from "$lib/components/files-section.svelte";
  import PageHeader from "$lib/components/page-header.svelte";
  import WarningBanner from "$lib/components/warning-banner.svelte";

  type Row = {
    id: string;
    sep: boolean;
    name: string;
    version: string;
    nexus: string;
    archive: string;
    updated: number;
    targets?: string[];
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
    if (!files.length && m.targets?.length) files = [...m.targets];
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
  <PageHeader title="Edit mod" subtitle="{archive} &nbsp;·&nbsp; installed {fmtWhen(updated)}" />

  <ModForm bind:name bind:version bind:nexus bind:section sections={sections} onNewSection={newSection} onRemoveSection={removeSection} />

  <FilesSection open={filesOpen} label="Installed files" paths={files} maxHeight="12rem" onToggle={(o)=> filesOpen = o} />

  <WarningBanner message={warn} />
  <FormActions cancelLabel="Cancel" onCancel={cancel} primaryLabel="Save" onPrimary={save} primaryClass="px-[18px] py-2 text-sm">
    {#snippet cancelIcon()}<X class="inline size-4" />{/snippet}
  </FormActions>
</div>
