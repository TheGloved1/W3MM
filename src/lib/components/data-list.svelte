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
  // Insertion-line position in scroll-content coordinates. A real overlay
  // element: box-shadow on <tr> doesn't paint under border-collapse.
  let indicatorTop: number | null = $state(null);

  function clearDrag() {
    dragFromKey = null;
    dragOverKey = null;
    dragOverPos = null;
    indicatorTop = null;
  }
</script>

<div
  bind:this={scroller}
  class="relative min-h-0 flex-1 select-none overflow-auto"
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
  {#if indicatorTop !== null}
    <div
      class="pointer-events-none absolute right-0 left-0 z-20 h-[2px] -translate-y-1/2 bg-primary shadow-[0_0_10px_1px_var(--color-ring)]"
      style="top: {indicatorTop}px"
    ></div>
  {/if}
  {#if items.length === 0}
    {@render empty?.()}
  {:else}
    <table class="w-full caption-bottom text-sm">
      <Table.Header class="sticky top-0 z-10 bg-muted/70 backdrop-blur supports-[backdrop-filter]:bg-muted/70">
        <Table.Row class="border-b border-border hover:bg-transparent">
          {#each columns as col}
            <Table.Head
              class="text-[11px] tracking-wide text-muted-foreground {col.align ===
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
            </Table.Head>
          {/each}
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each items as item (keyOf(item))}
          {@const selected = isSelected(item)}
          {@const draggable = isDraggable?.(item) ?? false}
          {@const isOver = dragOverKey !== null && dragOverKey === keyOf(item)}
          <Table.Row
            tabindex={0}
            {draggable}
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
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelect?.(item, e);
              }
            }}
            ondblclick={() => onActivate?.(item)}
            title={draggable ? "Drag to reorder" : undefined}
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
  {/if}
</div>
