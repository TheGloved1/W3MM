<script lang="ts">
  interface Props {
    children?: import('svelte').Snippet;
    disabled?: boolean;
    class?: string;
    onclick?: (e: MouseEvent) => void;
  }
  let {
    children,
    disabled = false,
    class: className = '',
    onclick,
  }: Props = $props();
</script>

<button
  {onclick}
  disabled={disabled}
  class="rounded-[7px] border border-border bg-popover px-4 py-[7px] text-sm hover:bg-accent disabled:opacity-50 {className}"
>
  {@render children?.()}
</button>
