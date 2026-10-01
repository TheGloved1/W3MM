<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type TableRowProps = WithElementRef<HTMLAttributes<HTMLTableRowElement>>;
</script>

<script lang="ts">
	let { class: className, ref = $bindable(null), children, ...restProps }: TableRowProps = $props();
</script>

<tr bind:this={ref} data-slot="table-row" class={cn("border-b transition-colors", className)} {...restProps}>
	{@render children?.()}
</tr>
