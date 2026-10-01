<script lang="ts" module>
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	export type CardProps = WithElementRef<HTMLAttributes<HTMLDivElement>>;
</script>

<script lang="ts">
	import CardHeader from "./card-header.svelte";
	import CardTitle from "./card-title.svelte";
	import CardDescription from "./card-description.svelte";
	import CardContent from "./card-content.svelte";

	let { class: className, ref = $bindable(null), children, ...restProps }: CardProps = $props();
</script>

<div bind:this={ref} data-slot="card" class={cn("card", className)} {...restProps}>
	{@render children?.()}
</div>
