<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { loadConfigNative } from '$lib/config';
  let { children } = $props();
  const appVersion = __APP_VERSION__;

  // TODO: add your pages here (route + label + icon).
  const nav = [
    { href: '/', label: 'Mods', icon: '◫' },
    { href: '/merges', label: 'Merges', icon: '⑂' },
    { href: '/downloads', label: 'Downloads', icon: '⤓' },
    { href: '/settings', label: 'Settings', icon: '⚙' },
  ];

  const themes = ['default','rose-pine','rose-pine-moon','rose-pine-dawn','catppuccin-mocha','catppuccin-macchiato','catppuccin-frappe','catppuccin-latte'];
  const fonts = ['inter','jetbrains','geist','space','manrope','sora'];

  function apply(cfg: any) {
    const t = themes.includes(cfg?.theme) ? cfg.theme : 'default';
    const f = fonts.includes(cfg?.font) ? cfg.font : 'inter';
    document.documentElement.setAttribute('data-theme', t);
    document.documentElement.setAttribute('data-font', f);
  }

  onMount(() => {
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
    return () => { if (id) clearInterval(id); };
  });

  let path = $derived(page.url.pathname);
  const isActive = (href: string) => href === '/' ? path === '/' : path.startsWith(href);

  let collapsed = $state(false);
  // Wide screens default to expanded, narrow screens to minimized.
  // A manual toggle is remembered and wins over the breakpoint.
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
  onMount(() => {
    try {
      const saved = localStorage.getItem("sidebar-collapsed");
      if (saved !== null) collapsed = saved === "true";
      else applyAutoSidebar();
    } catch {}
    const mq = window.matchMedia(wideQuery);
    mq.addEventListener("change", applyAutoSidebar);
    return () => mq.removeEventListener("change", applyAutoSidebar);
  });
  function toggleCollapsed() {
    collapsed = !collapsed;
    try { localStorage.setItem("sidebar-collapsed", String(collapsed)); } catch {}
  }
</script>

<svelte:head>
  <title>W3LMN</title>
</svelte:head>

<div class="flex h-screen bg-background text-foreground overflow-hidden">
  <aside class="shrink-0 flex flex-col border-r bg-gradient-to-b from-card to-background transition-all duration-200 {collapsed ? 'w-[56px] items-center' : 'w-[220px]'}">
    <div class="h-12 flex items-center gap-2 px-3 border-b shrink-0 w-full {collapsed ? 'justify-center' : ''}">
      <!-- TODO: replace with your logo/initials -->
      <div class="h-7 w-7 rounded-md bg-primary flex items-center justify-center text-primary-foreground font-black text-[11px] shrink-0 shadow-lg shadow-primary/25">W3</div>
      {#if !collapsed}
        <div class="leading-tight min-w-0">
          <div class="text-sm font-semibold tracking-tight truncate">W3LMN</div>
          <div class="text-[11px] text-muted-foreground">v{appVersion}</div>
        </div>
      {/if}
    </div>
    <nav class="flex-1 flex flex-col gap-1 w-full p-1.5 overflow-y-auto">
      {#each nav as item}
        <a href={item.href} title={item.label} class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition {isActive(item.href) ? 'bg-primary text-primary-foreground shadow-md shadow-primary/20' : 'text-muted-foreground hover:bg-muted hover:text-foreground'} {collapsed ? 'justify-center px-1' : ''}">
          <span class="text-sm leading-none shrink-0">{item.icon}</span>
          {#if !collapsed}<span>{item.label}</span>{/if}
        </a>
      {/each}
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
