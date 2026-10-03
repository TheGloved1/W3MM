<script lang="ts">
  interface Props {
    title?: string;
    children?: import('svelte').Snippet;
    class?: string;
    bg?: 'well' | 'card';
  }
  let {
    title,
    children,
    class: className = '',
    bg = 'well',
  }: Props = $props();

  const bgClass = bg === 'card' ? 'bg-card' : 'bg-well';
</script>

<div class="flex min-w-0 flex-col rounded-[7px] border border-border {bgClass} {className}">
  {#if title}
    <div class="border-b border-border px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">{title}</div>
  {/if}
  {@render children?.()}
</div>
