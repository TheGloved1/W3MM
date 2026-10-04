<script lang="ts">
  import PopoverButton from './popover-button.svelte';
  import PrimaryButton from './primary-button.svelte';

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
    <PopoverButton onclick={onCancel} class="px-4 py-2">
      {#if cancelIcon}{@render cancelIcon()}{/if}
      {cancelLabel}
    </PopoverButton>
  {/if}
  <PrimaryButton onclick={onPrimary} disabled={primaryDisabled} class={primaryClass}>
    {primaryLabel}
  </PrimaryButton>
</div>
