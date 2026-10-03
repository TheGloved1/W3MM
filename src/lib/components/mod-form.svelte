<script lang="ts">
  import FormInput from './form-input.svelte';
  import FormSelect from './form-select.svelte';
  import PopoverButton from './popover-button.svelte';

  interface Section {
    id: string;
    name: string;
  }

  interface Props {
    name: string;
    version: string;
    nexus: string;
    section: string;
    sections: Section[];
    onNewSection?: () => void;
    onRemoveSection?: () => void;
    showSectionActions?: boolean;
  }

  let {
    name = $bindable(''),
    version = $bindable(''),
    nexus = $bindable(''),
    section = $bindable(''),
    sections,
    onNewSection,
    onRemoveSection,
    showSectionActions = true,
  }: Props = $props();

  const sectionOptions = $derived([
    { value: '', label: 'No section' },
    ...sections.map(s => ({ value: s.id, label: s.name })),
  ]);
</script>

<div class="grid grid-cols-[auto_minmax(0,1fr)_auto_110px_auto_110px] items-center gap-x-3 gap-y-2">
  <span class="text-sm">Name in list</span>
  <FormInput bind:value={name} />
  <span class="text-sm">Version</span>
  <FormInput bind:value={version} width="110px" mono />
  <span class="text-sm">Nexus ID</span>
  <FormInput bind:value={nexus} width="110px" mono />
  <span class="text-sm">Section</span>
  <FormSelect bind:value={section} options={sectionOptions} />
  {#if showSectionActions}
    <span></span>
    <PopoverButton onclick={onNewSection}>New section…</PopoverButton>
    <span></span>
    <PopoverButton onclick={onRemoveSection} disabled={!section}>Remove section</PopoverButton>
  {:else}
    <span></span><span></span><span></span><span></span>
  {/if}
</div>
