<script lang="ts" module>
  import type { Snippet } from "svelte";

  export interface DataListColumn<T> {
    id: string;
    label: string;
    /** Initial width in px. The grid scales these to fill the viewport. */
    width?: number;
    align?: "left" | "center" | "right";
    /** Contents of one cell. Receives a single params object, not positional
     *  arguments — destructure it: `{#snippet cell({ item })}`. */
    cell?: Snippet<[{ item: T; selected: boolean }]>;
  }

  /** Modifier state at the time of the click. SvGrid's row callbacks carry
   *  the row but not the MouseEvent, so this is captured up front. */
  export interface ClickModifiers {
    shiftKey: boolean;
    ctrlKey: boolean;
    metaKey: boolean;
  }
</script>

<script lang="ts" generics="T">
  import { SvGrid, renderSnippet, type ColumnDef, type RowData } from "@svgrid/grid";

  interface Props {
    columns: DataListColumn<T>[];
    items: T[];
    keyOf: (item: T) => string;
    isSelected: (item: T) => boolean;
    /** Right-click on a row. */
    onContextMenu?: (item: T, e: MouseEvent) => void;
    /** Double-click on a row. */
    onActivate?: (item: T) => void;
    /** Click on a row, with the modifiers held at the time. */
    onSelect?: (item: T, mods: ClickModifiers) => void;
    onBackgroundClear?: () => void;
    /** Row renders as one full-width cell instead of per-column cells. */
    isDetail?: (item: T) => boolean;
    /** Contents of a full-width row (see `isDetail`). */
    detail?: Snippet<[ctx: { row: T; rowIndex: number }]>;
    /** Master drag-reorder on/off. Off also drops the grab cursor. */
    canReorder?: boolean;
    onReorder?: (
      fromKey: string,
      toKey: string,
      pos: "before" | "after",
    ) => void;
    /** Shown instead of the grid when there are no rows. */
    empty?: Snippet;
  }

  let {
    columns,
    items,
    keyOf,
    isSelected,
    onContextMenu,
    onActivate,
    onSelect,
    onBackgroundClear,
    isDetail,
    detail,
    canReorder = false,
    onReorder,
    empty,
  }: Props = $props();

  /** SvGrid's row callbacks carry row data but not the MouseEvent, so the
   *  modifiers that ctrl/shift multi-select depends on are stashed here. */
  let mods: ClickModifiers = $state({
    shiftKey: false,
    ctrlKey: false,
    metaKey: false,
  });

  type GridCol = ColumnDef<never, T & RowData>;

  const cols: GridCol[] = $derived(
    columns.map((c) => {
      const cell = c.cell;
      return {
        id: c.id,
        header: c.label,
        width: c.width,
        align: c.align === "center" ? "center" : (c.align ?? "left"),
        // Read-only in every respect: no sort, no filter, no inline editor, and
        // no auto-toggle of boolean-looking cells (our "enabled" checkbox is
        // rendered by `cell`, and the grid would otherwise fight our toggle()).
        sortable: false,
        filterable: false,
        editable: false,
        ...(cell
          ? {
              // The `cell` renderer is the one callback that receives a Row
              // wrapper, so the item has to come off `.original` here.
              cell: (ctx: { row: { original: T } }) => {
                const item = ctx.row.original;
                return renderSnippet(cell, {
                  item,
                  selected: isSelected(item),
                });
              },
            }
          : {}),
      };
    }) as GridCol[],
  );

  /** Row under a DOM event, via the index SvGrid stamps on every cell. */
  function rowAt(e: Event): T | null {
    const cell = (e.target as HTMLElement | null)?.closest?.(
      "td[data-svgrid-row], .sv-grid-detail-cell",
    ) as HTMLElement | null;
    if (!cell) return null;
    const tr = cell.closest("tr");
    const i =
      cell.classList.contains("sv-grid-detail-cell")
        ? Number(tr?.getAttribute("aria-rowindex")) - 1
        : Number(cell.dataset.svgridRow);
    return Number.isInteger(i) && i >= 0 ? (items[i] ?? null) : null;
  }
</script>

<!-- `role="presentation"` keeps this a plain container: the handlers below are
     event delegation over the grid, not interaction on a semantic element. -->
<div
  class="data-list relative flex min-h-0 flex-1 flex-col"
  role="presentation"
  onmousedown={(e) => {
    mods = { shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey };
  }}
  onclick={(e) => {
    // A click on the grid's own chrome (header, empty space under the rows)
    // rather than a row clears the selection, as clicking the background did.
    if ((e.target as HTMLElement | null)?.closest(".sv-grid-row")) return;
    onBackgroundClear?.();
  }}
  oncontextmenu={(e) => {
    if (!onContextMenu) return;
    const item = rowAt(e);
    if (!item) return;
    // SvGrid only opens its own menu when `contextMenu` is set; ours is
    // rendered by the caller, so suppress the native menu here.
    e.preventDefault();
    onContextMenu(item, e);
  }}
>
  {#if items.length === 0}
    <div class="min-h-0 flex-1 select-none overflow-auto">
      {@render empty?.()}
    </div>
  {:else}
    <SvGrid
      data={items as (T & RowData)[]}
      columns={cols}
      getRowId={keyOf}
      containerHeight="100%"
      rowHeight={42}
      headerHeight={40}
      autoRowHeight
      fitColumns
      columnResize
      virtualization={false}
      columnVirtualization={false}
      enableRowHover
      sortable={false}
      filterable={false}
      editable={false}
      enableInlineEditing={false}
      selectable={false}
      selectionMode="none"
      // NB: every callback below except `cell` receives the raw item, not a
      // Row wrapper. Only the cell renderer needs `.original`.
      rowClass={({ row }) => (isSelected(row) ? "dl-selected" : "")}
      isDetailRow={(row) => !!isDetail?.(row)}
      renderDetailRow={detail}
      rowDragManaged={canReorder}
      onRowDrop={
        onReorder
          ? ({ row, target, side }) => {
              // `into` only occurs on tree/group rows, which we don't have.
              if (!target || side === "into") return;
              onReorder(keyOf(row), keyOf(target), side);
            }
          : undefined
      }
      onRowClick={({ row }) => {
        if (isDetail?.(row)) return;
        onSelect?.(row, mods);
      }}
      onRowDoubleClick={({ row }) => {
        if (isDetail?.(row)) return;
        onActivate?.(row);
      }}
    />
  {/if}
</div>

<style>
  /* Selection tint. Every cell paints its own background, so the row's marker
     class has to reach down to them. */
  .data-list :global(.sv-grid-row.dl-selected > .sv-grid-cell) {
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
  }
  .data-list :global(.sv-grid-row.dl-selected:hover > .sv-grid-cell) {
    background: color-mix(in srgb, var(--color-primary) 20%, transparent);
  }
</style>
