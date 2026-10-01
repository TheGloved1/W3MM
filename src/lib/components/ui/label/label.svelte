<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type LabelProps = WithElementRef<HTMLAttributes<HTMLLabelElement>> & {
		for?: string;
	};
</script>

<script lang="ts">
	let { class: className, ref = $bindable(null), for: forId = undefined, children, ...restProps }: LabelProps = $props();
</script>

<label bind:this={ref} data-slot="label" class={cn("label", className)} for={forId} {...restProps}>
	{@render children?.()}
</label>
