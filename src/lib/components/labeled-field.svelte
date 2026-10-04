<script lang="ts">
  interface Props {
    label: string;
    labelWidth?: string;
    children?: import('svelte').Snippet;
    error?: string;
  }
  let {
    label,
    labelWidth = '110px',
    children,
    error,
  }: Props = $props();
</script>

<div class="flex items-center gap-3">
  <span class="shrink-0 text-sm" style="width:{labelWidth}">{label}</span>
  <div class="min-w-0 flex-1">{@render children?.()}</div>
  {#if error}
    <div class="text-[12px] text-[#e3735f]">{error}</div>
  {/if}
</div>
