<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type TableHeaderProps = WithElementRef<HTMLAttributes<HTMLElement>>;
</script>

<script lang="ts">
	let { class: className, ref = $bindable(null), children, ...restProps }: TableHeaderProps = $props();
</script>

<thead bind:this={ref} data-slot="table-header" class={cn("[&_tr]:border-b", className)} {...restProps}>
	{@render children?.()}
</thead>
