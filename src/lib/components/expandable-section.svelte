<script lang="ts">
  import { ChevronDown, ChevronRight } from 'lucide-svelte';
  interface Props {
    open: boolean;
    label: string;
    count?: number;
    children?: import('svelte').Snippet;
    onToggle: () => void;
  }
  let { open, label, count, children, onToggle }: Props = $props();
</script>

<div class="flex flex-col">
  <button
    onclick={onToggle}
    class="flex items-center gap-2 rounded-[7px] border border-border bg-card px-3 py-2 text-left text-sm hover:bg-accent/40 {open ? 'rounded-b-none border-b-0' : ''}"
  >
    <span class="text-muted-foreground">
      {#if open}
        <ChevronDown class="size-4" />
      {:else}
        <ChevronRight class="size-4" />
      {/if}
    </span>
    <span class="flex-1">{label}</span>
    {#if !open && count !== undefined}
      <span class="text-muted-foreground">{count} files</span>
    {/if}
  </button>
  {#if open}
    <div class="rounded-t-none border-t-0">
      {@render children?.()}
    </div>
  {/if}
</div>
