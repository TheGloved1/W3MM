<script lang="ts">
  import Button from './button.svelte';

  interface Props {
    cancelLabel?: string;
    cancelIcon?: import('svelte').Snippet;
    onCancel?: () => void;
    primaryLabel: string;
    onPrimary: () => void;
    primaryDisabled?: boolean;
    primaryClass?: string;
    spacer?: boolean;
  }
  let {
    cancelLabel = 'Cancel',
    cancelIcon,
    onCancel,
    primaryLabel,
    onPrimary,
    primaryDisabled = false,
    primaryClass = '',
    spacer = true,
  }: Props = $props();
</script>

<div class="flex items-center gap-2">
  {#if spacer}<span class="flex-1"></span>{/if}
  {#if onCancel}
    <Button variant="secondary" size="md" onclick={onCancel}>
      {#if cancelIcon}{@render cancelIcon()}{/if}
      {cancelLabel}
    </Button>
  {/if}
  <Button variant="primary" size="md" onclick={onPrimary} disabled={primaryDisabled} class={primaryClass}>
    {primaryLabel}
  </Button>
</div>
