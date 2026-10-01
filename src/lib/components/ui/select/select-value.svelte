<script lang="ts" module>
	import { Select as SelectPrimitive } from "bits-ui";
</script>

<script lang="ts">
	let { ref = $bindable(null), ...restProps }: SelectPrimitive.ValueProps = $props();
</script>

<SelectPrimitive.Value bind:ref data-slot="select-value" {...restProps} />
