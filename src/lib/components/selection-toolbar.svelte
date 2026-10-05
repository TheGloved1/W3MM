<script lang="ts">
  import Button from './button.svelte';

  interface Props {
    count: number;
    onEnable: () => void;
    onDisable: () => void;
    onUninstall: () => void;
    onClear: () => void;
    onCheckUpdates?: () => void;
    checkingUpdates?: boolean;
    /** Sits flush against a panel above it: drops the top corners and the top
     *  border so the two read as one stacked control rather than two boxes. */
    attached?: boolean;
  }
  let {
    count,
    onEnable,
    onDisable,
    onUninstall,
    onClear,
    onCheckUpdates,
    checkingUpdates = false,
    attached = false,
  }: Props = $props();
</script>

{#if count > 0}
  <div
    class="flex shrink-0 items-center gap-2 rounded-b-[7px] border border-border px-[14px] py-1.5 text-sm {attached
      ? 'rounded-t-none border-t-0'
      : 'rounded-t-[7px]'}"
    /* A shade below the list's bg-card, mixed toward the page background so it
       stays a subtle step down in every theme rather than a hardcoded hex. */
    style="background: color-mix(in srgb, var(--color-card) 88%, var(--color-background));"
  >
    <span class="font-medium">{count} selected</span>
    <div class="h-4 w-px bg-border"></div>
    <Button variant="secondary" size="sm" onclick={onEnable}>Enable</Button>
    <Button variant="secondary" size="sm" onclick={onDisable}>Disable</Button>
    {#if onCheckUpdates}
      <Button variant="secondary" size="sm" onclick={onCheckUpdates} disabled={checkingUpdates}>
        {checkingUpdates ? 'Checking…' : 'Check updates'}
      </Button>
    {/if}
    <Button variant="secondary" size="sm" onclick={onUninstall}>Uninstall</Button>
    <Button variant="secondary" size="sm" onclick={onClear} class="ml-auto">Clear</Button>
  </div>
{/if}
