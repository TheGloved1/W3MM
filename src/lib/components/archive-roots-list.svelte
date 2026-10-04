<script lang="ts">
  import { Check, ChevronDown } from 'lucide-svelte';
  import FormInput from './form-input.svelte';
  import FormSelect from './form-select.svelte';

  interface Root {
    prefix: string;
    kind: string;
    folder: string;
    files: number;
  }

  interface Props {
    roots: Root[];
    kinds: string[];
    rootLabel: (r: Root) => string;
  }

  let { roots, kinds, rootLabel }: Props = $props();
</script>

<div class="rounded-[7px] border border-border bg-card px-3 py-2">
  <div class="flex items-center gap-1 py-1 text-sm font-semibold">
    <ChevronDown class="size-4 text-muted-foreground" /> Archive contents
  </div>
  <div class="grid grid-cols-[minmax(0,1fr)_130px_minmax(0,1fr)] gap-2 px-1 pb-1 text-[12px] text-muted-foreground">
    <span class="pl-7">From archive</span><span>Type</span><span>Folder name</span>
  </div>
  {#each roots as r}
    <div class="grid grid-cols-[minmax(0,1fr)_130px_minmax(0,1fr)] items-center gap-2 border-t border-border/50 px-1 py-1.5">
      <span class="flex min-w-0 items-center gap-2">
        <span class="flex h-[18px] w-[18px] items-center justify-center rounded-[4px] border-[1.5px] border-[#c9a45c] bg-[#c9a45c] text-[#1c2127]"><Check class="size-3" /></span>
        <span class="truncate font-mono text-[12px]">{rootLabel(r)}</span>
      </span>
      <FormSelect bind:value={r.kind} options={kinds.map(k=>({value:k,label:k}))} class="rounded-[7px] border border-input bg-background px-2 py-1 text-[13px] outline-none focus:border-primary" />
      <FormInput bind:value={r.folder} mono class="rounded-[7px] border border-input bg-background px-2 py-1 text-[12px]" />
    </div>
  {/each}
</div>
