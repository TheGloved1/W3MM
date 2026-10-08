<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { ModsView, Profile, ProfileView } from '$lib/types';
  import Button from '$lib/components/button.svelte';
  import HeaderBar from '$lib/components/header-bar.svelte';
  import Panel from '$lib/components/panel.svelte';
  import Badge from '$lib/components/badge.svelte';
  import FormSelect from '$lib/components/form-select.svelte';

  let profiles: ProfileView[] = $state([]);
  let active: string = $state('');
  let modsCount: number = $state(0);
  let newName: string = $state('');
  let renameTarget: string | null = $state(null);
  let renameValue: string = $state('');
  let error: string = $state('');
  let status: string = $state('');
  let busy: string = $state('');

  const ghost = 'shrink-0 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-40';
  const ghostDanger = 'shrink-0 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-[#e3735f]/15 hover:text-[#e3735f] disabled:opacity-40';

  async function refresh() {
    try {
      const st = await invoke<ModsView>('list_mods');
      profiles = st.profiles ?? [];
      active = st.active_profile ?? '';
      if (active && !profiles.some((p) => p.id === active)) active = '';
      modsCount = st.mods.filter((m) => !m.sep).length;
    } catch (e) {
      error = String(e);
    }
  }

  async function deployApplied(profileName: string) {
    const running = await invoke<boolean>('game_running');
    if (running) {
      error = 'Profile applied — close the game before deploying';
      return;
    }
    const res = await invoke<string[] | { deployed: string[]; removed: string[] }>('deploy');
    const files = Array.isArray(res) ? res : res.deployed;
    status = `“${profileName}” active — deployed ${files.length} file${files.length === 1 ? '' : 's'}`;
  }

  async function switchTo(id: string) {
    if (!id || id === active) return;
    error = '';
    busy = 'Applying…';
    try {
      await invoke('switch_profile', { id });
      const p = profiles.find((x) => x.id === id);
      await deployApplied(p?.name ?? 'Profile');
      await refresh();
    } catch (e) {
      error = String(e);
    }
    busy = '';
  }

  async function create() {
    const n = newName.trim();
    if (!n) return;
    error = '';
    try {
      await invoke('create_profile', { name: n });
      newName = '';
      status = 'Profile created from the current selection.';
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function duplicate(id: string) {
    error = '';
    try {
      await invoke('duplicate_profile', { id });
      status = 'Profile duplicated.';
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function startRename(p: Profile) {
    renameTarget = p.id;
    renameValue = p.name;
  }

  async function confirmRename() {
    if (!renameTarget || !renameValue.trim()) {
      renameTarget = null;
      return;
    }
    const target = profiles.find((p) => p.id === renameTarget);
    if (target && renameValue.trim() !== target.name) {
      error = '';
      try {
        await invoke('rename_profile', { id: renameTarget, name: renameValue.trim() });
        await refresh();
      } catch (e) {
        error = String(e);
      }
    }
    renameTarget = null;
  }

  async function remove(p: Profile) {
    if (profiles.length <= 1) return;
    if (!confirm(`Delete profile “${p.name}”?`)) return;
    error = '';
    try {
      await invoke('delete_profile', { id: p.id });
      status = 'Profile deleted.';
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function openProfilesDir() {
    try {
      const d = await invoke<string>('profiles_dir_path');
      await invoke('open_path', { target: d });
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    refresh();
  });
</script>

<div class="flex h-full min-h-0 flex-col">
  <HeaderBar title="Profiles" subtitle={`${profiles.length} profiles · ${modsCount} mods in store`}>
    {#snippet right()}
      {#if busy}<span class="shrink-0 text-xs text-muted-foreground">{busy}</span>{/if}
    {/snippet}
  </HeaderBar>
  <div class="min-h-0 flex-1 overflow-y-auto px-[22px] py-[14px]">
    <div class="mx-auto flex max-w-[720px] flex-col gap-4">
      {#if error}
        <div class="rounded-[7px] border border-[#e3735f]/40 bg-[#e3735f]/10 px-3 py-2 text-sm text-[#e3735f]">{error}</div>
      {/if}
      {#if status}
        <div class="rounded-[7px] border border-border bg-card px-3 py-2 text-sm">{status}</div>
      {/if}

      <Panel title="Active profile" bg="card">
        <div class="flex flex-col gap-2 p-3">
          <span class="text-xs text-muted-foreground">Switching applies that selection and deploys it to the game.</span>
          <FormSelect
            bind:value={active}
            options={profiles.map((p) => ({ value: p.id, label: p.id === active ? `${p.name} • active` : p.name }))}
            class="w-full"
            onchange={(e) => switchTo((e.target as HTMLSelectElement).value)}
          />
        </div>
      </Panel>

      <Panel title="Create profile" bg="card">
        <div class="flex flex-col gap-2 p-3">
          <span class="text-xs text-muted-foreground">Saves the currently enabled mods as a new profile.</span>
          <div class="flex gap-2">
            <input
              bind:value={newName}
              placeholder="my-new-profile"
              class="min-w-0 flex-1 rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground/70 focus:border-primary"
              onkeydown={(e) => { if (e.key === 'Enter') create(); }}
            />
            <Button variant="primary" size="md" onclick={create}>Create</Button>
          </div>
        </div>
      </Panel>

      <div class="flex flex-col gap-2">
        <div class="text-xs font-semibold tracking-widest text-muted-foreground uppercase">All profiles</div>
        {#each profiles as p}
          <Panel bg="card" class={p.id === active ? 'border-primary' : ''}>
            <div class="flex items-center gap-3 p-3">
              <div class="min-w-0 flex-1">
                {#if renameTarget === p.id}
                  <div class="flex gap-2">
                    <input
                      bind:value={renameValue}
                      class="h-7 min-w-0 flex-1 rounded-[7px] border border-input bg-card px-2 text-sm outline-none focus:border-primary"
                      onkeydown={(e) => { if (e.key === 'Enter') confirmRename(); if (e.key === 'Escape') renameTarget = null; }}
                    />
                    <Button variant="primary" size="md" onclick={confirmRename}>Save</Button>
                    <Button variant="secondary" size="md" onclick={() => (renameTarget = null)}>Cancel</Button>
                  </div>
                {:else}
                  <div class="flex items-center gap-2">
                    <span class="truncate text-sm font-medium">{p.name}</span>
                    {#if p.id === active}<Badge>Active</Badge>{/if}
                  </div>
                  <div class="text-xs text-muted-foreground">{p.enabled}/{p.mods} mods enabled{p.id === active ? ' · currently deployed selection' : ''}</div>
                {/if}
              </div>
              {#if renameTarget !== p.id}
                <div class="flex shrink-0 items-center gap-1">
                  <button class={ghost} onclick={() => switchTo(p.id)} disabled={p.id === active}>Activate</button>
                  <button class={ghost} onclick={() => duplicate(p.id)}>Duplicate</button>
                  <button class={ghost} onclick={() => startRename(p)}>Rename</button>
                  <button class={ghostDanger} disabled={profiles.length <= 1} onclick={() => remove(p)}>Delete</button>
                </div>
              {/if}
            </div>
          </Panel>
        {/each}
        {#if !profiles.length}
          <p class="text-[13px] leading-relaxed text-muted-foreground">No profiles yet — save one above.</p>
        {/if}
      </div>

      <Panel bg="card" class="border-dashed">
        <div class="flex items-center justify-between gap-2 p-3">
          <div class="min-w-0 truncate text-xs text-muted-foreground">Profiles live in the app store's <span class="font-mono">profiles/</span> dir</div>
          <div class="flex shrink-0 gap-1">
            <button class={ghost} onclick={openProfilesDir}>Open profiles dir</button>
            <button class={ghost} onclick={refresh}>Refresh</button>
          </div>
        </div>
      </Panel>
    </div>
  </div>
</div>
