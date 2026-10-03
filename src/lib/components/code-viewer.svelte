<script lang="ts">
  interface Props {
    lines: string[];
    codeFont: string;
    codeSize: number;
    highlight?: (i: number) => boolean;
    class?: string;
  }

  let {
    lines,
    codeFont,
    codeSize,
    highlight,
    class: className = '',
  }: Props = $props();

  const codeStyle = $derived(`font-family:'${codeFont}',monospace;font-size:${codeSize}pt;`);
</script>

<pre class="min-h-0 flex-1 overflow-auto p-2 leading-relaxed {className}" style={codeStyle}>
  {#each lines as l, i}
    <div class={highlight?.(i) ? 'bg-[#e3735f]/15' : ''}>
      <span class="mr-2 inline-block w-8 select-none text-right text-muted-foreground/50">{i + 1}</span>
      {l || ' '}
    </div>
  {/each}
</pre>
