<script lang="ts">
  interface Props {
    paths: string[];
    maxHeight?: string;
  }
  let { paths, maxHeight = "16rem" }: Props = $props();

  type Node = {
    name: string;
    full: string;
    children: Node[];
    count: number;
    file: boolean;
  };

  const tree = $derived.by(() => {
    const root: Node = { name: "", full: "", children: [], count: 0, file: false };
    for (const p of paths) {
      const parts = p.split("/").filter(Boolean);
      let cur = root;
      for (let i = 0; i < parts.length; i++) {
        const name = parts[i];
        const full = parts.slice(0, i + 1).join("/");
        let child = cur.children.find((c) => c.name === name);
        if (!child) {
          child = { name, full, children: [], count: 0, file: i === parts.length - 1 };
          cur.children.push(child);
        } else if (i === parts.length - 1) {
          child.file = true;
        }
        cur = child;
      }
    }
    const count = (n: Node): number => {
      let c = 0;
      for (const ch of n.children) c += ch.file ? 1 : count(ch);
      n.count = c;
      return c;
    };
    count(root);
    return root;
  });

  let expanded: Record<string, boolean> = $state({});
  let selected: string | null = $state(null);

  function isOpen(full: string): boolean {
    return full in expanded ? expanded[full] : true;
  }
  function toggle(full: string) {
    expanded = { ...expanded, [full]: !isOpen(full) };
  }

  type Row = { node: Node; depth: number; open: boolean };
  function flatten(node: Node, depth: number, out: Row[]) {
    for (const child of node.children) {
      const open = isOpen(child.full);
      out.push({ node: child, depth, open });
      if (open && child.children.length) flatten(child, depth + 1, out);
    }
  }
  const rows = $derived.by(() => {
    const out: Row[] = [];
    flatten(tree, 0, out);
    return out;
  });
</script>

<div class="overflow-hidden rounded-[7px] border border-border bg-card">
  <div
    class="grid grid-cols-[minmax(0,1fr)_56px] border-b border-border px-2 py-1.5 text-[11px] uppercase tracking-wide text-muted-foreground"
  >
    <span>Path</span>
    <span class="text-right">Files</span>
  </div>
  <div
    class="overflow-auto py-1 font-mono text-[12px]"
    style="max-height:{maxHeight}"
  >
    {#each rows as row (row.node.full)}
      <button
        class="grid w-full grid-cols-[minmax(0,1fr)_56px] items-center px-2 py-[3px] text-left hover:bg-accent/40 {selected ===
        row.node.full
          ? 'bg-[#c9a45c]/30'
          : ''}"
        onclick={() => {
          if (row.node.children.length) toggle(row.node.full);
          else selected = row.node.full;
        }}
      >
        <span
          class="flex min-w-0 items-center"
          style="padding-left:{row.depth * 15}px"
        >
          {#if row.node.children.length}
            <span class="mr-1 inline-block w-4 text-center text-muted-foreground"
              >{row.open ? "⌄" : "›"}</span
            >
          {:else}
            <span class="mr-1 inline-block w-4"></span>
          {/if}
          <span class="truncate"
            >{row.node.name}{#if row.node.children.length}/{/if}</span
          >
        </span>
        <span class="text-right text-muted-foreground">{row.node.count}</span>
      </button>
    {/each}
    {#if !rows.length}
      <div class="px-2 py-3 text-muted-foreground">No files</div>
    {/if}
  </div>
</div>
