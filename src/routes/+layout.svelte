<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { loadConfigNative } from '$lib/config';
  let { children } = $props();

  const themes = ['default','rose-pine','rose-pine-moon','rose-pine-dawn','catppuccin-mocha','catppuccin-macchiato','catppuccin-frappe','catppuccin-latte'];
  const fonts = ['inter','jetbrains','geist','space','manrope','sora'];

  function apply(cfg: any) {
    const t = themes.includes(cfg?.theme) ? cfg.theme : 'default';
    const f = fonts.includes(cfg?.font) ? cfg.font : 'inter';
    document.documentElement.setAttribute('data-theme', t);
    document.documentElement.setAttribute('data-font', f);
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

  onMount(() => {
    forwardConsole();
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
</script>

<svelte:head>
  <title>W3MM</title>
</svelte:head>

<div class="h-screen bg-background text-foreground overflow-hidden">
  {@render children()}
</div>
