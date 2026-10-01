<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type CardDescriptionProps = WithElementRef<HTMLAttributes<HTMLParagraphElement>>;
</script>

<script lang="ts">
	let { class: className, ref = $bindable(null), children, ...restProps }: CardDescriptionProps = $props();
</script>

<p bind:this={ref} data-slot="card-description" class={cn("text-sm opacity-70", className)} {...restProps}>
	{@render children?.()}
</p>
