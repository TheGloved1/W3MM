<script lang="ts">
  interface Props {
    title: string;
    subtitle?: string;
    description?: string;
  }
  let { title, subtitle, description }: Props = $props();
</script>

<div class="flex flex-col gap-1">
  <div class="text-[14pt] font-semibold">{title}</div>
  {#if subtitle}
    <div class="truncate text-[13px] text-muted-foreground">{subtitle}</div>
  {/if}
  {#if description}
    <p class="text-sm text-muted-foreground">{description}</p>
  {/if}
</div>
