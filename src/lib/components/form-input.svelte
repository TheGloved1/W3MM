<script lang="ts">
  interface Props {
    value: string | number;
    type?: string;
    placeholder?: string;
    mono?: boolean;
    width?: string;
    class?: string;
    disabled?: boolean;
    oninput?: (e: Event) => void;
  }
  let {
    value = $bindable(''),
    type = 'text',
    placeholder = '',
    mono = false,
    width = '',
    class: className = '',
    disabled = false,
    oninput,
  }: Props = $props();

  const base = 'rounded-[7px] border border-input bg-card px-2.5 py-1.5 outline-none focus:border-primary';
  const monoClass = mono ? 'font-mono text-[13px]' : 'text-sm';
  const widthClass = width ? `w-[${width}]` : '';
</script>

<input
  {type}
  placeholder={placeholder}
  {disabled}
  class="{base} {monoClass} {widthClass} {className}"
  bind:value
  {oninput}
/>
