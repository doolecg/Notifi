<script lang="ts">
  // Custom options of the Radial menu page: only `profiles`, the wheel editor.
  import type { CustomProps } from '../sections/customs';
  import WheelEditor from '../radial/WheelEditor.svelte';

  let { option }: CustomProps = $props();
</script>

{#if option.id === 'profiles'}
  <div class="intro"><span class="name">{option.label}</span><small>{option.hint}</small></div>
  <WheelEditor />
{/if}

<style>
  .intro {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 14px 0;
    font-size: 13px;
  }
  small {
    color: var(--mut);
    font-size: 11.5px;
  }
</style>
