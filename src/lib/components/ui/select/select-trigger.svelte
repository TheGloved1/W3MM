<script lang="ts" module>
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn, type WithoutChild } from "$lib/utils.js";
</script>

<script lang="ts">
	import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithoutChild<SelectPrimitive.TriggerProps> = $props();
</script>

<SelectPrimitive.Trigger
	bind:ref
	data-slot="select-trigger"
	class={cn(
		"border-base-content/20 bg-base-100 data-placeholder:text-base-content/50 focus:border-primary min-w-36 flex items-center justify-between gap-1.5 rounded-lg border px-2.5 py-2 text-sm transition-colors outline-none select-none disabled:cursor-not-allowed disabled:opacity-50",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<ChevronDownIcon class="text-base-content/50 pointer-events-none size-4" />
</SelectPrimitive.Trigger>
