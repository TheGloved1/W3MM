<script lang="ts">
  import ExpandableSection from './expandable-section.svelte';
  import FileTree from './file-tree.svelte';

  interface Props {
    open?: boolean;
    label: string;
    paths: string[];
    maxHeight?: string;
    onToggle?: (open: boolean) => void;
  }
  let {
    open = false,
    label,
    paths,
    maxHeight = '12rem',
    onToggle,
  }: Props = $props();

  let internalOpen = $state(open);
  const isOpen = $derived(internalOpen || open);

  function toggle() {
    internalOpen = !internalOpen;
    onToggle?.(internalOpen);
  }
</script>

<ExpandableSection open={isOpen} label={label} count={paths.length} onToggle={toggle}>
  <FileTree paths={paths} maxHeight={maxHeight} class="rounded-t-none border-t-0" />
</ExpandableSection>
