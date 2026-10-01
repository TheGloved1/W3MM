<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type TableCellProps = WithElementRef<HTMLAttributes<HTMLTableCellElement>>;
</script>

<script lang="ts">
	let { class: className, ref = $bindable(null), children, ...restProps }: TableCellProps = $props();
</script>

<td bind:this={ref} data-slot="table-cell" class={cn("p-2 align-middle", className)} {...restProps}>
	{@render children?.()}
</td>
