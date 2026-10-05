<script lang="ts" module>
  export interface DataListColumn {
    id: string;
    label: string;
    /** Show sort control and arrow affordance. */
    sortable?: boolean;
    align?: "left" | "right";
  }
</script>

<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import * as Table from "./ui/table/index.js";
  import SortHeader from "./sort-header.svelte";
  import type { SortDir } from "$lib/table-sort";

  interface Props {
    /** Column definitions (single shared header row — header and body can never drift). */
    columns: DataListColumn[];
    items: T[];
    keyOf: (item: T) => string | number;
    isSelected: (item: T) => boolean;
    sortKey?: string | null;
    sortDir?: SortDir;
    onSort?: (id: string) => void;
    onSelect?: (item: T, e: MouseEvent | KeyboardEvent) => void;
    /** Right-click on a row. */
    onContextMenu?: (item: T, e: MouseEvent) => void;
    /** Double-click / Enter on a focused row. */
    onActivate?: (item: T) => void;
    onBackgroundClear?: () => void;
    /** Return false to disable dragging for an item (e.g. while sorted). */
    isDraggable?: (item: T) => boolean;
    onReorder?: (
      fromKey: string | number,
      toKey: string | number,
      pos: "before" | "after",
    ) => void;
    /** Table cells (<Table.Cell>) for one item. Parent owns actions. */
    row: Snippet<[item: T, selected: boolean]>;
    /** Shown when items is empty (loading / no data / no matches). */
    empty?: Snippet;
  }

  let {
    columns,
    items,
    keyOf,
    isSelected,
    sortKey = null,
    sortDir = "asc",
    onSort,
    onSelect,
    onContextMenu,
    onActivate,
    onBackgroundClear,
    isDraggable,
    onReorder,
    row,
    empty,
  }: Props = $props();

  let dragFromKey: string | number | null = $state(null);
  let dragOverKey: string | number | null = $state(null);
  let dragOverPos: "before" | "after" | null = $state(null);
  let scroller: HTMLDivElement | null = $state(null);
  let headerWrap: HTMLDivElement | null = $state(null);
  let bodyTable: HTMLTableElement | null = $state(null);
  let colWidths: Record<string, number> = $state({});
  /** Body column widths mirrored onto the fixed header so both stay aligned
   *  (they are two tables now, so auto layout would size them differently). */
  let measured: Record<string, number> = $state({});
  let resizeCol: string | null = $state(null);
  let resizeStartX = 0;
  let resizeStartW = 0;
  // Insertion-line position in scroll-content coordinates. A real overlay
  // element: box-shadow on <tr> doesn't paint under border-collapse.
  let indicatorTop: number | null = $state(null);

  function headerTotal(): number {
    const sum = columns.reduce((n, c) => n + (measured[c.id] ?? 0), 0);
    return sum || 100;
  }

  function syncHeaderWidths() {
    if (!bodyTable) return;
    const row = Array.from(bodyTable.querySelectorAll("tbody tr")).find(
      (r) => (r as HTMLTableRowElement).cells.length === columns.length,
    ) as HTMLTableRowElement | undefined;
    if (!row) return;
    const next: Record<string, number> = {};
    Array.from(row.cells).forEach((cell, i) => {
      const col = columns[i];
      if (col) next[col.id] = Math.round(cell.getBoundingClientRect().width);
    });
    if (Object.keys(next).length === columns.length) measured = next;
  }

  /** Keep the fixed header aligned while the body scrolls sideways. */
  function syncHeaderScroll() {
    if (headerWrap && scroller) headerWrap.scrollLeft = scroller.scrollLeft;
  }

  $effect(() => {
    // Re-measure when rows/columns change and on container resize.
    items.length;
    columns.length;
    if (!bodyTable) return;
    syncHeaderWidths();
    const ro = new ResizeObserver(syncHeaderWidths);
    ro.observe(bodyTable);
    return () => ro.disconnect();
  });

  function clearDrag() {
    dragFromKey = null;
    dragOverKey = null;
    dragOverPos = null;
    indicatorTop = null;
  }
</script>

<div class="relative flex min-h-0 flex-1 flex-col">
  {#if items.length === 0}
    <div
      class="min-h-0 flex-1 select-none overflow-auto"
      role="button"
      tabindex="0"
      onclick={(e) => {
        if (e.target === e.currentTarget) onBackgroundClear?.();
      }}
      onkeydown={(e) => {
        if ((e.key === "Enter" || e.key === " ") && e.target === e.currentTarget) {
          e.preventDefault();
          onBackgroundClear?.();
        }
      }}
    >
      {@render empty?.()}
    </div>
  {:else}
    <!-- Header sits OUTSIDE the scroller: no sticky needed, and an overlay
         scrollbar can never paint over it. -->
    <div bind:this={headerWrap} class="shrink-0 overflow-hidden bg-card">
      <table aria-hidden="true" class="caption-bottom text-sm" style="width: {headerTotal()}px">
        <colgroup>
          {#each columns as col}
            <col style="width: {measured[col.id] ?? 'auto'};" />
          {/each}
        </colgroup>
        <thead>
          <tr class="border-b border-border hover:bg-transparent">
            {#each columns as col, colIndex}
              <th
                class="relative h-10 px-2 align-middle text-left text-[11px] font-medium tracking-wide whitespace-nowrap text-muted-foreground {col.align ===
                'right'
                  ? 'text-right'
                  : ''}"
              >
                {#if col.sortable && onSort}
                  <SortHeader
                    label={col.label}
                    active={sortKey === col.id}
                    dir={sortDir}
                    align={col.align ?? "left"}
                    onclick={() => onSort(col.id)}
                  />
                {:else}
                  <span class="uppercase">{col.label}</span>
                {/if}
                {#if colIndex < columns.length - 1}
                  <span
                    role="presentation"
                    class="absolute inset-y-0 right-0 w-2 cursor-col-resize touch-none select-none bg-transparent hover:bg-primary/50"
                    onpointerdown={(e) => {
                      e.preventDefault();
                      resizeCol = col.id;
                      resizeStartX = e.clientX;
                      const th = (e.currentTarget as HTMLElement).parentElement;
                      resizeStartW = th?.getBoundingClientRect().width ?? 100;
                      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
                    }}
                    onpointermove={(e) => {
                      if (resizeCol !== col.id) return;
                      const w = Math.max(40, resizeStartW + (e.clientX - resizeStartX));
                      colWidths = { ...colWidths, [col.id]: w };
                      measured = { ...measured, [col.id]: w };
                    }}
                    onpointerup={() => (resizeCol = null)}
                    onpointercancel={() => (resizeCol = null)}
                  ></span>
                {/if}
              </th>
            {/each}
          </tr>
        </thead>
      </table>
    </div>

    <div
      bind:this={scroller}
      class="relative min-h-0 flex-1 select-none overflow-auto"
      role="button"
      tabindex="0"
      onscroll={syncHeaderScroll}
      onclick={(e) => {
        if (e.target === e.currentTarget) onBackgroundClear?.();
      }}
      onkeydown={(e) => {
        if ((e.key === "Enter" || e.key === " ") && e.target === e.currentTarget) {
          e.preventDefault();
          onBackgroundClear?.();
        }
      }}
    >
  {#if indicatorTop !== null}
    <div
      class="pointer-events-none absolute right-0 left-0 z-20 h-[2px] -translate-y-1/2 bg-primary shadow-[0_0_10px_1px_var(--color-ring)]"
      style="top: {indicatorTop}px"
    ></div>
  {/if}
    <table bind:this={bodyTable} class="w-full caption-bottom text-sm">
      <colgroup>
        {#each columns as col}
          <col style="width: {colWidths[col.id] ?? measured[col.id] ?? 'auto'};" />
        {/each}
      </colgroup>
      <Table.Body>
        {#each items as item (keyOf(item))}
          {@const selected = isSelected(item)}
          {@const draggable = isDraggable?.(item) ?? false}
          {@const isOver = dragOverKey !== null && dragOverKey === keyOf(item)}
          <Table.Row
            tabindex={0}
            draggable={draggable}
            ondragstart={(e) => {
              if (!draggable) return;
              dragFromKey = keyOf(item);
              if (e.dataTransfer) {
                e.dataTransfer.effectAllowed = "move";
                e.dataTransfer.setData("text/plain", String(keyOf(item)));
              }
            }}
            ondragover={(e) => {
              if (dragFromKey === null || dragFromKey === keyOf(item)) return;
              e.preventDefault();
              const r = (
                e.currentTarget as HTMLElement
              ).getBoundingClientRect();
              dragOverKey = keyOf(item);
              dragOverPos =
                e.clientY < r.top + r.height / 2 ? "before" : "after";
              // Position the overlay line at the row edge in scroll-content
              // coordinates so it stays glued while scrolling.
              const wrap = scroller?.getBoundingClientRect();
              if (scroller && wrap) {
                indicatorTop =
                  r.top -
                  wrap.top +
                  scroller.scrollTop +
                  (dragOverPos === "before" ? 0 : r.height);
              }
            }}
            ondrop={(e) => {
              e.preventDefault();
              if (dragFromKey !== null && dragFromKey !== keyOf(item))
                onReorder?.(dragFromKey, keyOf(item), dragOverPos ?? "before");
              clearDrag();
            }}
            ondragend={clearDrag}
            onclick={(e) => onSelect?.(item, e)}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onContextMenu?.(item, e);
            }}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelect?.(item, e);
              }
            }}
            ondblclick={() => onActivate?.(item)}
            class="{draggable
              ? 'cursor-grab active:cursor-grabbing'
              : 'cursor-pointer'} {selected
              ? 'bg-primary/10 hover:bg-primary/20'
              : isOver
                ? 'bg-primary/15'
                : 'hover:bg-muted/50'} {keyOf(item) === dragFromKey
              ? 'opacity-40'
              : ''}"
          >
            {@render row(item, selected)}
          </Table.Row>
        {/each}
      </Table.Body>
    </table>
    </div>
  {/if}
</div>
