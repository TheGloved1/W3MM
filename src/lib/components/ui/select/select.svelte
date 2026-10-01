<script lang="ts" module>
	import { Select as SelectPrimitive } from "bits-ui";
</script>

<script lang="ts">
	let { open = $bindable(false), value = $bindable(), ...restProps }: SelectPrimitive.RootProps = $props();
</script>

<SelectPrimitive.Root bind:open bind:value={value as never} {...restProps} />
