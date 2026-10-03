<script lang="ts">
  interface Props {
    value: string;
    options: { value: string; label: string }[];
    class?: string;
  }
  let {
    value = $bindable(''),
    options,
    class: className = '',
  }: Props = $props();

  const base = 'rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none focus:border-primary';
</script>

<select bind:value class="{base} {className}">
  {#each options as opt}
    <option value={opt.value}>{opt.label}</option>
  {/each}
</select>
