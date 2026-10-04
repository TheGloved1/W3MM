<script lang="ts">
  interface Props {
    message?: string;
  }
  let { message }: Props = $props();
</script>

{#if message}
  <p class="font-semibold text-[#dbb977]">{message}</p>
{/if}
