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
  class="rounded-[7px] bg-primary px-3 py-1 text-[13px] font-semibold text-primary-foreground hover:brightness-110 disabled:opacity-50 {className}"
>
  {@render children?.()}
</button>
