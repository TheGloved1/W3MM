<script lang="ts">
  interface Props {
    count: number;
    replace: boolean;
    onReplace: (v: boolean) => void;
  }
  let { count, replace = $bindable(false), onReplace }: Props = $props();

  function toggle() {
    const v = !replace;
    (replace as any) = v;
    onReplace?.(v);
  }
</script>

<div class="rounded-[7px] border border-[#c9a45c]/40 bg-[#c9a45c]/10 px-3 py-2 text-[13px]">
  <div class="font-semibold text-[#dbb977]">Replaces {count} installed mod{count === 1 ? '' : 's'} (same files).</div>
  <label class="mt-1 flex cursor-pointer items-center gap-2">
    <input type="checkbox" bind:checked={replace} class="h-4 w-4 accent-[#c9a45c]" />
    <span>Uninstall {count === 1 ? 'it' : 'them'} and install this instead</span>
  </label>
</div>
