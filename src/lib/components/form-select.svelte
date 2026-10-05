<script lang="ts">
  interface Props {
    value: string;
    options: { value: string; label: string }[];
    class?: string;
    disabled?: boolean;
    onchange?: (e: Event) => void;
  }
  let {
    value = $bindable(''),
    options,
    class: className = '',
    disabled = false,
    onchange,
  }: Props = $props();

  const base = 'rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none focus:border-primary';
</script>

<select bind:value {disabled} {onchange} class="{base} {className}">
  {#each options as opt}
    <option value={opt.value}>{opt.label}</option>
  {/each}
</select>
