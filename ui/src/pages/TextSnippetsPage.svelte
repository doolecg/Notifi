<script lang="ts">
  // Custom options of the Text snippets page (spec/input/text-snippets.md): only `editor`.
  import type { CustomProps } from '../sections/customs';
  import Editor from '../snippets/Editor.svelte';

  let { option }: CustomProps = $props();
</script>

{#if option.id === 'editor'}
  <Editor />
{/if}
