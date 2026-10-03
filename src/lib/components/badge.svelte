<script lang="ts">
  interface Props {
    children?: import('svelte').Snippet;
    class?: string;
    variant?: 'gold' | 'default';
  }
  let {
    children,
    class: className = '',
    variant = 'gold',
  }: Props = $props();

  const variantClass = variant === 'gold' ? 'bg-[#c9a45c]/15 text-[#c9a45c]' : 'bg-muted text-muted-foreground';
</script>

<span class="rounded-full px-2 py-[1px] text-[11px] font-semibold {variantClass} {className}">
  {@render children?.()}
</span>
