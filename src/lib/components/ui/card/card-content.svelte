<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type CardContentProps = WithElementRef<HTMLAttributes<HTMLDivElement>>;
</script>

<script lang="ts">
	let { class: className, ref = $bindable(null), children, ...restProps }: CardContentProps = $props();
</script>

<div bind:this={ref} data-slot="card-content" class={cn("card-body pt-0", className)} {...restProps}>
	{@render children?.()}
</div>
