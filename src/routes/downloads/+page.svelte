<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { AppState } from '$lib/types';
  import {
    queueState,
    refreshQueue,
    humanSize,
    dlMetaLine,
    dlStatus,
    dlAction,
    dlSameFile,
    openDownloadsFolder,
    dlRemove,
    dlTrash,
    dlMain,
  } from '$lib/downloads.svelte';
  import { Check, ChevronDown, ChevronRight, Trash2 } from 'lucide-svelte';

  let mods: AppState['mods'] | undefined = $state(undefined);
  let error: string = $state('');
  let dlCollapsed: Record<string, boolean> = $state({});

  const groupedQueue = $derived(
    queueState.queue.reduce(
      (acc, qq) => {
        const key = qq.mod_id || qq.file_id || qq.filename;
        if (!acc[key]) acc[key] = { key, mod_id: qq.mod_id, mod_name: qq.mod_name || qq.filename, rows: [] };
        acc[key].rows.push(qq);
        if (!acc[key].mod_name && qq.mod_name) acc[key].mod_name = qq.mod_name;
        return acc;
      },
      {} as Record<string, { key: string; mod_id: string; mod_name: string; rows: typeof queueState.queue }>,
    ),
  );

  const dlSummary = $derived(
    queueState.queue.length
      ? (() => {
          const bytes = queueState.queue.reduce((a, qq) => a + (qq.total || qq.done || 0), 0);
          const n = Object.keys(groupedQueue).length;
          return `${n} mod${n === 1 ? '' : 's'}  ·  ${humanSize(bytes)}`;
        })()
      : '',
  );

  onMount(async () => {
    await refreshQueue();
    try {
      const st = await invoke<AppState>('list_mods');
      mods = st.mods;
    } catch {}
  });
</script>

<!-- Same dark panel as the old Mods-page downloads sidebar, now full-page. -->
<div class="flex h-full min-h-0 flex-col gap-3 overflow-y-auto bg-[#232930] px-4 py-[18px]">
  <div class="flex items-start gap-1.5">
    <div class="flex-1 leading-tight">
      <div class="text-[13pt] font-semibold text-[#d9dee4]">Downloads</div>
      {#if dlSummary}
        <div class="mt-0.5 text-[12px] text-[#8c96a1]">{dlSummary}</div>
      {/if}
    </div>
    <button
      onclick={() => openDownloadsFolder().catch((e) => { error = String(e); })}
      title="Open the downloads folder"
      class="rounded px-2 py-1 text-[13px] text-[#8c96a1] hover:bg-[#2b323a] hover:text-[#d9dee4]"
      >Open folder</button
    >
  </div>
  {#if error}
    <div class="rounded-[8px] border border-[#e3735f]/40 bg-[#e3735f]/10 px-3 py-2 text-[13px] text-[#e3735f]">{error}</div>
  {/if}
  {#each Object.values(groupedQueue) as group}
    {@const rows = group.rows}
    {@const nested = rows.length > 1}
    {@const settled = rows.some((r) => r.status === 'done' && dlSameFile(mods, r))}
    {@const collapsed =
      dlCollapsed[group.key] ??
      rows.every(
        (r) =>
          (r.status === 'done' && dlSameFile(mods, r)) ||
          (settled && r.status === 'done'),
      )}
    {@const single = rows.length === 1 ? rows[0] : null}
    {@const running = rows.some(
      (r) =>
        r.status === 'active' ||
        r.status === 'starting' ||
        r.status === 'queued' ||
        r.status === 'paused',
    )}
    <div class="rounded-[8px] border border-[#363e48] bg-[#2b323a] p-[14px]">
      <div class="flex items-start gap-1.5">
        <button
          onclick={() => { dlCollapsed[group.key] = !collapsed; }}
          title={collapsed ? 'Expand' : 'Collapse'}
          class="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-[13px] text-[#8c96a1] hover:text-[#dbb977]"
        >{#if collapsed}<ChevronRight class="size-4" />{:else}<ChevronDown class="size-4" />{/if}</button>
        <button
          onclick={() => { dlCollapsed[group.key] = !collapsed; }}
          class="min-w-0 flex-1 text-left text-[13.5px] font-semibold leading-snug text-[#d9dee4]"
        >{group.mod_name || (group.mod_id ? `Nexus mod ${group.mod_id}` : 'Mod')}</button>
        {#if collapsed && rows.length && settled}
          <span title="Installed: this exact version" class="mt-0.5 shrink-0 text-[#7fbf8a]"><Check class="size-4" /></span>
        {/if}
        {#if single && !collapsed && !running && single.status === 'done'}
          <button
            onclick={() => dlTrash(single).catch((e) => { error = String(e); })}
            title="Move the downloaded file to the Trash"
            class="mt-0.5 shrink-0 rounded p-1 text-[#8c96a1] hover:bg-[#e3735f]/15 hover:text-[#e3735f]"
          ><Trash2 class="size-4" /></button>
        {/if}
      </div>
      {#if !collapsed}
        <div class={nested ? 'mt-3 flex flex-col gap-1 divide-y divide-[#363e48]/60 pl-4' : 'mt-3 flex flex-col gap-2'}>
          {#each rows as qq}
            {@const st = dlStatus(mods, qq)}
            {@const meta = dlMetaLine(qq)}
            {@const action = qq.status === 'done' ? dlAction(mods, qq) : ''}
            {@const isRunning =
              qq.status === 'active' ||
              qq.status === 'starting' ||
              qq.status === 'queued'}
            {@const mainLabel = isRunning
              ? 'Cancel'
              : qq.status === 'error' || qq.status === 'failed'
                ? 'Retry'
                : qq.status === 'paused' || qq.status === 'cancelled'
                  ? 'Retry'
                  : action}
            <div class={nested ? 'px-2 py-2' : ''}>
              {#if nested}
                <div class="mb-2 truncate text-[13px] text-[#d9dee4]" title={qq.filename ?? ''}>
                  {qq.filename || 'Downloading…'}
                </div>
              {/if}
              {#if isRunning}
                <div class="mb-2 h-[6px] overflow-hidden rounded bg-[#13171b]">
                  <div
                    class="h-full rounded bg-[#c9a45c]"
                    style="width:{qq.total ? Math.round((100 * qq.done) / Math.max(1, qq.total)) : 0}%"
                  ></div>
                </div>
              {/if}
              <div class="text-[13px]" style="color:{st.color}">{st.text}</div>
              <div class="mt-2 flex items-center gap-1.5">
                <span class="min-w-0 flex-1 truncate text-[12.5px] text-[#8c96a1]" title={meta}>{meta}</span>
                {#if !isRunning}
                  <button
                    onclick={() => dlRemove(qq).catch((e) => { error = String(e); })}
                    title="Take it off this list. A downloaded file stays in the downloads folder."
                    class="shrink-0 rounded-[6px] border border-[#363e48] bg-[#2b323a] px-3 py-1.5 text-[13px] text-[#d9dee4] hover:bg-[#363e48]"
                  >Remove</button>
                {/if}
                {#if mainLabel}
                  <button
                    onclick={() => dlMain(mods, qq).catch((e) => { error = String(e); })}
                    class="shrink-0 rounded-[6px] px-3.5 py-1.5 text-[13px] font-semibold {action === 'Downgrade'
                      ? 'border border-[#363e48] bg-[#2b323a] text-[#d9dee4] hover:bg-[#363e48]'
                      : 'bg-[#c9a45c] text-[#1c2127] hover:bg-[#dbb977]'}"
                  >{mainLabel}</button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
  {#if !queueState.queue.length}
    <p class="text-[13px] leading-relaxed text-[#8c96a1]">
      Click “Mod Manager Download” on a Witcher 3 mod's Nexus page. It
      downloads here, then the Install window opens.
    </p>
  {/if}
</div>
