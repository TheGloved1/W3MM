<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { assets } from '$app/paths';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { loadConfigNative } from '$lib/config';
  import {
    queueState,
    setQueue,
    refreshQueue,
    dlSameFile,
    offerInstall,
  } from '$lib/downloads.svelte';
  import type { AppState } from '$lib/types';
  import { Download, Layers, Settings, Users } from 'lucide-svelte';
  let { children } = $props();

  const appVersion = __APP_VERSION__;
  // Tool windows (install/edit/merges) share this root layout — they are
  // separate Tauri windows, and must render their page content bare, with
  // no sidebar chrome. The main window is the only one with the app shell.
  const isMain = getCurrentWindow().label === 'main';

  const themes = ['default','rose-pine','rose-pine-moon','rose-pine-dawn','catppuccin-mocha','catppuccin-macchiato','catppuccin-frappe','catppuccin-latte'];
  const fonts = ['inter','jetbrains','geist','space','manrope','sora'];

  function apply(cfg: any) {
    const t = themes.includes(cfg?.theme) ? cfg.theme : 'default';
    const f = fonts.includes(cfg?.font) ? cfg.font : 'inter';
    document.documentElement.setAttribute('data-theme', t);
    document.documentElement.setAttribute('data-font', f);
  }

  const queueUnsubs = ((window as any).__yawmmQueueUnsubs ??= []) as Array<() => void>;
  function unsubscribeQueueListeners() {
    // Window scope persists across dev HMR reloads, unlike module state —
    // so the previous layout instance's listeners really do get removed.
    for (const u of queueUnsubs.splice(0)) {
      try { u(); } catch {}
    }
  }
  
  function forwardConsole() {
    const origDebug = console.debug;
    console.debug = (...args: any[]) => {
      origDebug(...args);
      invoke("frontend_log", { msg: args.map(String).join(" ") }).catch(() => {});
    };
    const origError = console.error;
    console.error = (...args: any[]) => {
      origError(...args);
      invoke("frontend_log", { msg: args.map(String).join(" ") }).catch(() => {});
    };
    window.addEventListener("error", (e) => {
      invoke("frontend_log", { msg: `window error: ${e.message}` }).catch(() => {});
    });
    window.addEventListener("unhandledrejection", (e) => {
      invoke("frontend_log", { msg: `unhandled rejection: ${e.reason}` }).catch(() => {});
    });
  }

  async function notify(title: string, body: string) {
    try {
      const { sendNotification } = await import('@tauri-apps/plugin-notification');
      sendNotification({ title, body });
    } catch {}
  }

  // Sidebar state: wide screens default expanded, narrow collapsed.
  // A manual toggle is remembered and wins over the breakpoint.
  let collapsed = $state(false);
  const wideQuery = "(min-width: 1024px)";
  function hasManualChoice() {
    try {
      return localStorage.getItem("sidebar-collapsed") !== null;
    } catch {
      return true;
    }
  }
  function applyAutoSidebar(e?: { matches: boolean }) {
    if (hasManualChoice()) return;
    const wide = e ? e.matches : window.matchMedia(wideQuery).matches;
    collapsed = !wide;
  }
  function toggleCollapsed() {
    collapsed = !collapsed;
    try { localStorage.setItem("sidebar-collapsed", String(collapsed)); } catch {}
  }

  let path = $derived(page.url.pathname);
  const isActive = (href: string) => href === '/' ? path === '/' : path.startsWith(href);
  const activeDownloads = $derived(
    queueState.queue.filter((qq) => qq.status === "active" || qq.status === "starting" || qq.status === "queued").length,
  );

  onMount(() => {
    forwardConsole();
    try {
      const saved = localStorage.getItem("sidebar-collapsed");
      if (saved !== null) collapsed = saved === "true";
      else applyAutoSidebar();
    } catch {}
    const mq = window.matchMedia(wideQuery);
    mq.addEventListener("change", applyAutoSidebar);

    let id: ReturnType<typeof setInterval> | undefined;
    (async () => {
      try {
        const cfg = await loadConfigNative();
        apply(cfg);
        id = setInterval(async () => {
          try { const c = await loadConfigNative(); apply(c); } catch {}
        }, 1000);
      } catch {}
    })();

    // Shared Nexus queue: deep-link + progress/done/meta live here so the
    // queue survives route changes between Mods and Downloads.
    let unlistenUrl: (() => void) | undefined;
    let unlistenP: (() => void) | undefined;
    let unlistenD: (() => void) | undefined;
    let unlistenMeta: (() => void) | undefined;
    let unlistenInstalled: (() => void) | undefined;
    (async () => {
      // Only the main window owns the download queue: tool windows (install,
      // edit, settings, merges) each load this layout too, and letting every
      // one subscribe would multiply dlNxm per nxm:// event and stack
      // offerInstall calls. Events are window-local JS anyway, so the queue
      // UI (main window only) is the sole subscriber by design.
      if (getCurrentWindow().label !== 'main') return;
      // Dev HMR re-mounts this layout; tear down listeners from the previous
      // module instance before re-registering, or every hot reload doubles
      // them (duplicate nxm handling = same download raced in parallel).
      unsubscribeQueueListeners();
      const track = (u: () => void): (() => void) => { queueUnsubs.push(u); return u; };
      try {
        // Persisted history merges into the live queue once at boot;
        // refreshQueue polls live state from then on.
        const { loadDownloadHistory } = await import('$lib/downloads.svelte');
        await loadDownloadHistory();
        await refreshQueue();
        const { getCurrent, onOpenUrl } = await import('@tauri-apps/plugin-deep-link');
        const cur = await getCurrent().catch(() => []);
        if (cur?.length) {
          const { dlNxm } = await import('$lib/downloads.svelte');
          try { await dlNxm(cur[0]); } catch (e) { console.error("[yawmm] deep-link failed", e); }
          goto('/downloads').catch(() => {});
        }
        unlistenUrl = track(await onOpenUrl(async (urls) => {
          if (urls?.length) {
            const { dlNxm } = await import('$lib/downloads.svelte');
            try { await dlNxm(urls[0]); } catch (e) { console.error("[yawmm] deep-link failed", e); }
            goto('/downloads').catch(() => {});
          }
        }));
        unlistenP = track(await listen<{ id: string; done: number; total: number; speed?: number }>(
          "download-progress",
          (e) => {
            setQueue(
              queueState.queue.map((qq) =>
                qq.id === e.payload.id
                  ? { ...qq, done: e.payload.done, total: e.payload.total, speed: e.payload.speed ?? qq.speed, status: "active" }
                  : qq,
              ),
            );
          },
        ));
        unlistenD = track(await listen<{ id: string; path: string; error?: string }>(
          "download-done",
          async (e) => {
            await refreshQueue();
            const row = queueState.queue.find((qq) => qq.id === e.payload.id);
            if (e.payload.error) return;
            await notify("YAWMM", e.payload.path.split("/").pop() ?? "download done");
            if (row && row.status === "done") {
              // Suppress the auto Install window when it's the exact file
              // already installed; check against the live mod list.
              let mods: AppState['mods'] | undefined;
              try {
                const st = await invoke<AppState>("list_mods");
                mods = st.mods;
              } catch {}
              if (!dlSameFile(mods, row)) {
                await offerInstall(row, e.payload.path).catch(() => {});
              }
            }
          },
        ));
        unlistenMeta = track(await listen<{ id: string }>("download-meta", async () => {
          await refreshQueue().catch(() => {});
        }));
        // A finished Install window sends the user back to the mod list,
        // wherever in the app they were.
        unlistenInstalled = track(await listen("mod-installed", () => {
          goto('/').catch(() => {});
        }));
      } catch {}
    })();

    return () => {
      if (id) clearInterval(id);
      mq.removeEventListener("change", applyAutoSidebar);
      unsubscribeQueueListeners();
    };
  });
</script>

<svelte:head>
  <title>YAWMM</title>
</svelte:head>

{#if isMain}
<div class="flex h-screen bg-background text-foreground overflow-hidden">
  <aside class="shrink-0 flex flex-col border-r bg-gradient-to-b from-card to-background transition-all duration-200 {collapsed ? 'w-[56px] items-center' : 'w-[220px]'}">
    <div class="h-12 flex items-center gap-2 px-3 border-b shrink-0 w-full {collapsed ? 'justify-center' : ''}">
      <img src="{assets}/yawmm_logo.svg" alt="YAWMM" class="h-7 w-7 shrink-0 rounded-md shadow-lg shadow-primary/25" />
      {#if !collapsed}
        <div class="leading-tight min-w-0">
          <div class="text-sm font-semibold tracking-tight truncate">YAWMM</div>
          <div class="text-[11px] text-muted-foreground">v{appVersion}</div>
        </div>
      {/if}
    </div>
    <nav class="flex-1 flex flex-col gap-1 w-full p-1.5 overflow-y-auto">
      <a href="/" title="Mods" class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition {isActive('/') ? 'bg-primary text-primary-foreground shadow-md shadow-primary/20' : 'text-muted-foreground hover:bg-muted hover:text-foreground'} {collapsed ? 'justify-center px-1' : ''}">
        <Layers class="size-4 shrink-0" />
        {#if !collapsed}<span>Mods</span>{/if}
      </a>
      <a href="/profiles" title="Profiles" class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition {isActive('/profiles') ? 'bg-primary text-primary-foreground shadow-md shadow-primary/20' : 'text-muted-foreground hover:bg-muted hover:text-foreground'} {collapsed ? 'justify-center px-1' : ''}">
        <Users class="size-4 shrink-0" />
        {#if !collapsed}<span>Profiles</span>{/if}
      </a>
      <a href="/downloads" title="Downloads" class="relative flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition {isActive('/downloads') ? 'bg-primary text-primary-foreground shadow-md shadow-primary/20' : 'text-muted-foreground hover:bg-muted hover:text-foreground'} {collapsed ? 'justify-center px-1' : ''}">
        <Download class="size-4 shrink-0" />
        {#if !collapsed}<span class="flex-1">Downloads</span>{/if}
        {#if activeDownloads}
          {#if collapsed}
            <span class="pointer-events-none absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-[#c9a45c] px-0.5 text-[10px] font-bold text-[#1c2127]">{activeDownloads}</span>
          {:else}
            <span class="flex h-5 min-w-5 items-center justify-center rounded-full bg-[#c9a45c] px-1 text-[11px] font-bold text-[#1c2127]">{activeDownloads}</span>
          {/if}
        {/if}
      </a>
      <a href="/settings" title="Settings" class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition {isActive('/settings') ? 'bg-primary text-primary-foreground shadow-md shadow-primary/20' : 'text-muted-foreground hover:bg-muted hover:text-foreground'} {collapsed ? 'justify-center px-1' : ''}">
        <Settings class="size-4 shrink-0" />
        {#if !collapsed}<span>Settings</span>{/if}
      </a>
    </nav>
    <div class="p-2 w-full">
      <button
        class="h-7 w-full rounded-md border bg-background/60 hover:bg-muted hover:border-muted-foreground/30 flex items-center justify-center text-xs text-muted-foreground hover:text-foreground transition"
        onclick={toggleCollapsed}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        title={collapsed ? "Expand" : "Collapse"}
      >
        <span class="text-xs">{collapsed ? "›" : "‹"}</span>
        {#if !collapsed}<span class="ml-1.5 text-xs">Collapse</span>{/if}
      </button>
    </div>
  </aside>

  <div class="flex flex-1 flex-col min-w-0 overflow-auto bg-background">
    {@render children()}
  </div>
</div>
{:else}
<!-- Tool windows (install/edit/merges): content only, no app chrome. -->
<div class="h-screen overflow-auto bg-background text-foreground">
  {@render children()}
</div>
{/if}
