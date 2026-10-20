<script lang="ts">
  // Custom option of the App updates page (spec/apps/app-updates.md): `rules`, the skipped
  // versions and ignored apps, each undone here.
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import type { Rules } from '../apps/api';
  import type { CustomProps } from '../sections/customs';

  let { option }: CustomProps = $props();
  let rules = $state<Rules>({ skipped: [], ignored: [] });

  const empty = $derived(!rules.skipped.length && !rules.ignored.length);

  onMount(() => {
    const load = () => invoke<Rules>('apps_rules').then((r) => (rules = r));
    load();
    const off = listen('apps://changed', load);
    return () => off.then((f) => f());
  });

  async function undo(kind: 'skip' | 'ignore', manager: string, id: string, version?: string) {
    rules = await invoke<Rules>('apps_rule_remove', { kind, manager, id, version });
  }
</script>

{#if option.id === 'rules'}
  {#if empty}
    <div class="row"><span class="name">No rules<small>Right-click an app in the update list to skip a version or stop checking it.</small></span></div>
  {/if}
  {#each rules.ignored as r (r.manager + r.id)}
    <div class="row">
      <span class="name">{r.name}<small>Never checked ({r.manager})</small></span>
      <button class="ghost" onclick={() => undo('ignore', r.manager, r.id)}>Undo</button>
    </div>
  {/each}
  {#each rules.skipped as r (r.manager + r.id + r.version)}
    <div class="row">
      <span class="name">{r.name}<small>Version {r.version} skipped ({r.manager})</small></span>
      <button class="ghost" onclick={() => undo('skip', r.manager, r.id, r.version)}>Undo</button>
    </div>
  {/each}
{/if}
