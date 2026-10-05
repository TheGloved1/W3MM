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
  }
  let {
    count,
    onEnable,
    onDisable,
    onUninstall,
    onClear,
    onCheckUpdates,
    checkingUpdates = false,
  }: Props = $props();
</script>

{#if count > 0}
  <div class="flex shrink-0 items-center gap-2 rounded-[7px] border border-border bg-card px-[14px] py-1.5 text-sm">
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
