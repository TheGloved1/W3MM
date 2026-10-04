<script lang="ts">
  import FormInput from './form-input.svelte';
  import PopoverButton from './popover-button.svelte';

  interface MergerRep {
    config: string;
    wrong: [string, string, string][];
    unfixable: [string, string][];
  }

  interface Props {
    mergerPath: string;
    mergerState: string;
    mergerRep: MergerRep | null;
    open: boolean;
    onToggle: () => void;
    onPickMerger: () => void;
    onApplyFixes: () => void;
  }

  let {
    mergerPath = $bindable(''),
    mergerState = '',
    mergerRep = null,
    open,
    onToggle,
    onPickMerger,
    onApplyFixes,
  }: Props = $props();
</script>

<div class="rounded-lg border border-border bg-popover px-[14px] py-2">
  <div class="flex items-center gap-2">
    <button onclick={onToggle} class="flex-1 py-1 text-left text-sm font-semibold hover:text-primary">{open ? '▾' : '▸'} Legacy Script Merger</button>
    <span class="text-[12px] text-muted-foreground">{open ? mergerState : (mergerPath ? mergerState : 'Not set')}</span>
  </div>
  {#if open}
    <div class="flex items-center gap-3 py-1">
      <span class="w-[86px] shrink-0 text-sm">Path</span>
      <FormInput bind:value={mergerPath} mono placeholder="path to ScriptMerger.exe" class="min-w-0 flex-1" />
      <PopoverButton onclick={onPickMerger} class="px-3 py-[7px]">Browse…</PopoverButton>
    </div>
    {#if mergerRep}
      {#each mergerRep.wrong as [k, was, want]}
        <div class="my-1 rounded bg-[#c9a45c]/10 p-1.5 font-mono text-[11px]">{k}: {was} → {want}</div>
      {/each}
      {#each mergerRep.unfixable as [k, v]}
        <div class="my-1 rounded bg-[#e3735f]/10 p-1.5 font-mono text-[11px]">{k}: {v} (no fix known)</div>
      {/each}
      {#if mergerRep.wrong.length}
        <PopoverButton onclick={onApplyFixes} class="mb-1 px-3 py-1.5">Apply fixes</PopoverButton>
      {/if}
    {/if}
  {/if}
</div>
