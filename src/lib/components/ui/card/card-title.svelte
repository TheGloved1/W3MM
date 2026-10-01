<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type CardTitleProps = WithElementRef<HTMLAttributes<HTMLHeadingElement>>;
</script>

<script lang="ts">
	let { class: className, ref = $bindable(null), children, ...restProps }: CardTitleProps = $props();
</script>

<h3 bind:this={ref} data-slot="card-title" class={cn("card-title", className)} {...restProps}>
	{@render children?.()}
</h3>
