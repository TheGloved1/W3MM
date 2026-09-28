<script lang="ts">
  import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-svelte";

  interface Props {
    label: string;
    active?: boolean;
    dir?: "asc" | "desc";
    align?: "left" | "right";
    title?: string;
    onclick: () => void;
  }

  let {
    label,
    active = false,
    dir = "asc",
    align = "left",
    title,
    onclick,
  }: Props = $props();
</script>

<button
  class="flex items-center gap-1 uppercase hover:text-foreground {active
    ? 'text-foreground'
    : ''} {align === 'right' ? 'justify-end' : 'text-left'}"
  title={title ?? `Sort by ${label.toLowerCase()}`}
  aria-label={active ? `Sort by ${label.toLowerCase()}, currently ${dir}ending` : `Sort by ${label.toLowerCase()}`}
  {onclick}
>
  {label}
  {#if active}
    {#if dir === "asc"}<ArrowUp class="size-3" />{:else}<ArrowDown class="size-3" />{/if}
  {:else}
    <ChevronsUpDown class="size-3 opacity-50" />
  {/if}
</button>
