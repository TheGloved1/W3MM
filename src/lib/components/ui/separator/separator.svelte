<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type SeparatorProps = WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		orientation?: "horizontal" | "vertical";
	};
</script>

<script lang="ts">
	let {
		class: className,
		ref = $bindable(null),
		orientation = "horizontal",
		children,
		...restProps
	}: SeparatorProps = $props();
</script>

<div
	bind:this={ref}
	data-slot="separator"
	data-orientation={orientation}
	role="separator"
	aria-orientation={orientation}
	class={cn(orientation === "vertical" ? "divider-horizontal" : "divider", className)}
	{...restProps}
>
	{@render children?.()}
</div>
