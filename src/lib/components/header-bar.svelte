<script lang="ts">
  import type { Snippet } from 'svelte';
  // Top bar for the three main-window pages (Mods / Downloads / Settings).
  // Same h-12 + border-b geometry as the sidebar brand block in
  // +layout.svelte, so the two rules sit on one horizontal line.
  interface Props {
    title: string;
    subtitle?: string;
    right?: Snippet;
    tone?: 'default' | 'dark';
    padClass?: string;
  }
  let { title, subtitle, right, tone = 'default', padClass = 'px-[22px]' }: Props = $props();
  // No backdrop-blur: the bar is in-flow (nothing scrolls under it), and
  // backdrop-filter would trap dropdowns' z-50 inside the bar's stacking
  // context so the mod list paints over them. relative z-50 keeps the
  // "..." menu above the grid's sticky header (z-35) and scrollbars (z-40).
  const bar = $derived(
    tone === 'dark'
      ? 'border-[#363e48] bg-[#232930]'
      : 'border-border bg-card',
  );
  const titleCls = $derived(tone === 'dark' ? 'text-[#d9dee4]' : 'text-foreground');
  const subCls = $derived(tone === 'dark' ? 'text-[#8c96a1]' : 'text-muted-foreground');
</script>

<div class="relative z-50 flex h-12 shrink-0 items-center gap-3 border-b {padClass} {bar}">
  <div class="min-w-0 flex-1 leading-tight">
    <div class="truncate text-sm font-semibold {titleCls}">{title}</div>
    {#if subtitle}<div class="truncate text-xs {subCls}">{subtitle}</div>{/if}
  </div>
  {#if right}{@render right()}{/if}
</div>
