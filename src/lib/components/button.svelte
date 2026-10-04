<script lang="ts">
  interface Props {
    children?: import('svelte').Snippet;
    disabled?: boolean;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    variant?: 'primary' | 'secondary';
    size?: 'sm' | 'md';
    type?: 'button' | 'submit';
  }
  let {
    children,
    disabled = false,
    class: className = '',
    onclick,
    variant = 'secondary',
    size = 'md',
    type = 'button',
  }: Props = $props();

  const base = 'inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-[7px] text-sm font-medium transition-colors disabled:pointer-events-none disabled:opacity-50';
  const variants = {
    primary: 'bg-primary font-semibold text-primary-foreground hover:brightness-110',
    secondary: 'border border-border bg-popover hover:bg-accent',
  };
  const sizes = {
    sm: 'px-3 py-1.5 text-[13px]',
    md: 'px-4 py-2 text-sm',
  };
</script>

<button
  {type}
  {onclick}
  {disabled}
  class="{base} {variants[variant]} {sizes[size]} {className}"
>
  {@render children?.()}
</button>
