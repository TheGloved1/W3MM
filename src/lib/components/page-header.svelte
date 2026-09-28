<!-- Shared top bar for every page. Contract (do not drift from this):
  - Fixed `h-12` (48px) so page headers align exactly with the sidebar
    header in `routes/+layout.svelte`. Use `wrap` for toolbars whose
    content may exceed one row — height stays >= h-12.
  - `px-3`, hairline `border-b`, translucent card background.
  New pages: wrap your top bar content in <PageHeader>, don't hand-roll
  another header div. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    /** Bold title shown at the left (e.g. "Settings"). */
    title?: string;
    /** Muted subline under the title. */
    subtitle?: string;
    /** Allow the bar to grow past h-12 with wrapped toolbar content. */
    wrap?: boolean;
    /** Keep the bar visible while the page scrolls (e.g. long forms). */
    sticky?: boolean;
    /** Rendered before the title (e.g. icon badge, back button). */
    before?: Snippet;
    /** Main left content (buttons, selectors, dividers). */
    children?: Snippet;
    /** Right-aligned content. Wrapper adds `ml-auto` for you. */
    right?: Snippet;
  }

  let {
    title,
    subtitle,
    wrap = false,
    sticky = false,
    before,
    children,
    right,
  }: Props = $props();
</script>

<div
  class="flex shrink-0 items-center gap-1.5 border-b bg-card/70 px-3 backdrop-blur supports-[backdrop-filter]:bg-card/70 {wrap
    ? 'min-h-12 flex-wrap py-1.5'
    : 'h-12'} {sticky ? 'sticky top-0 z-10' : ''}"
>
  {@render before?.()}
  {#if title}
    <div class="mr-1 leading-tight">
      <div class="text-sm font-semibold tracking-tight">{title}</div>
      {#if subtitle}
        <div class="text-[11px] text-muted-foreground">{subtitle}</div>
      {/if}
    </div>
  {/if}
  {@render children?.()}
  {#if right}
    <div class="ml-auto flex items-center gap-1.5">
      {@render right()}
    </div>
  {/if}
</div>
