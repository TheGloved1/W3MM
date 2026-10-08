<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { AppState, Profile } from '$lib/types';
  import Button from '$lib/components/button.svelte';
  import HeaderBar from '$lib/components/header-bar.svelte';
  import { Check, Pencil, Trash2 } from 'lucide-svelte';

  let profiles: Profile[] = $state([]);
  let enabledNow: number = $state(0);
  let name: string = $state('');
  let error: string = $state('');
  let status: string = $state('');
  let busy: string = $state('');

  async function refresh() {
    try {
      const st = await invoke<AppState>('list_mods');
      profiles = st.profiles ?? [];
      enabledNow = st.mods.filter((m) => !m.sep && m.enabled).length;
    } catch (e) {
      error = String(e);
    }
  }

  async function saveCurrent() {
    const n = name.trim();
    if (!n) {
      error = 'Name the profile first.';
      return;
    }
    error = '';
    busy = 'Saving…';
    try {
      await invoke('save_profile', { name: n });
      name = '';
      status = 'Profile saved.';
      await refresh();
    } catch (e) {
      error = String(e);
    }
    busy = '';
  }

  async function apply(id: string) {
    error = '';
    busy = 'Applying…';
    try {
      await invoke('apply_profile', { id });
      const running = await invoke<boolean>('game_running');
      if (running) {
        error = 'Profile applied — close the game before deploying';
        busy = '';
        await refresh();
        return;
      }
      const res = await invoke<string[] | { deployed: string[]; removed: string[] }>('deploy');
      const files = Array.isArray(res) ? res : res.deployed;
      status = `Profile applied — deployed ${files.length} file${files.length === 1 ? '' : 's'}`;
      await refresh();
    } catch (e) {
      error = String(e);
    }
    busy = '';
  }

  async function rename(id: string, current: string) {
    const n = prompt('Profile name', current);
    if (!n || n.trim() === current) return;
    error = '';
    try {
      await invoke('rename_profile', { id, name: n });
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(id: string, profileName: string) {
    if (!confirm(`Delete profile “${profileName}”?`)) return;
    error = '';
    try {
      await invoke('delete_profile', { id });
      status = 'Profile deleted.';
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    refresh();
  });
</script>

<div class="flex h-full min-h-0 flex-col">
  <HeaderBar title="Profiles" subtitle={`${profiles.length} saved · ${enabledNow} mods enabled`}>
    {#snippet right()}
      {#if busy}<span class="shrink-0 text-xs text-muted-foreground">{busy}</span>{/if}
    {/snippet}
  </HeaderBar>
  <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-[22px] py-[14px]">
    {#if error}
      <div class="rounded-[7px] border border-[#e3735f]/40 bg-[#e3735f]/10 px-3 py-2 text-sm text-[#e3735f]">{error}</div>
    {/if}
    {#if status}
      <div class="rounded-[7px] border border-border bg-card px-3 py-2 text-sm">{status}</div>
    {/if}

    <div class="flex items-center gap-2">
      <input
        bind:value={name}
        placeholder="New profile from current mods…"
        class="min-w-0 flex-1 rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground/70 focus:border-primary"
        onkeydown={(e) => { if (e.key === 'Enter') saveCurrent(); }}
      />
      <Button variant="primary" size="md" onclick={saveCurrent}>Save current</Button>
    </div>

    <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
      {#each profiles as p}
        <div class="flex items-center gap-2 rounded-[7px] border border-border bg-card px-3 py-2">
          <div class="min-w-0 flex-1 leading-tight">
            <div class="truncate text-sm font-semibold">{p.name}</div>
            <div class="text-xs text-muted-foreground">{p.enabled.length} mod{p.enabled.length === 1 ? '' : 's'} enabled</div>
          </div>
          <button
            onclick={() => apply(p.id)}
            title="Enable exactly this selection and deploy"
            class="flex shrink-0 items-center gap-1.5 rounded-[6px] bg-[#c9a45c] px-3 py-1.5 text-[13px] font-semibold text-[#1c2127] hover:bg-[#dbb977]"
          ><Check class="size-4" /> Apply</button>
          <button
            onclick={() => rename(p.id, p.name)}
            title="Rename profile"
            class="shrink-0 rounded-[6px] p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          ><Pencil class="size-4" /></button>
          <button
            onclick={() => remove(p.id, p.name)}
            title="Delete profile"
            class="shrink-0 rounded-[6px] p-1.5 text-muted-foreground hover:bg-[#e3735f]/15 hover:text-[#e3735f]"
          ><Trash2 class="size-4" /></button>
        </div>
      {/each}
      {#if !profiles.length}
        <p class="text-[13px] leading-relaxed text-muted-foreground">
          No profiles yet. Enable the mods you want, then save the selection above — applying it later restores exactly that set.
        </p>
      {/if}
    </div>
  </div>
</div>
