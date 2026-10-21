<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getVersion } from '@tauri-apps/api/app';
  import { onMount } from 'svelte';

  let version = $state('');
  onMount(() => {
    getVersion().then((v) => (version = v)).catch(() => {});
  });
</script>

<div class="card about">
  <div class="head">
    <img src="/icon.png" alt="" />
    <div>
      <div class="title">Notifi</div>
      <div class="sub">Version {version || '...'}</div>
    </div>
  </div>
  <div class="row">
    <span class="name">Licence<small>MIT. Free and open source.</small></span>
  </div>
  <div class="row">
    <span class="name">No warranty<small>Provided "as is", without warranty, under the MIT licence. To the extent permitted by law, the authors are not liable for any loss or damage from using it. Back up your work.</small></span>
  </div>
  <div class="row">
    <span class="name">Not affiliated<small>Notifi is not made by, affiliated with or endorsed by Vorssaint, Anthropic, OpenAI, GitHub, the makers of T3 Code or OpenCode, Microsoft, Spotify or Amazon. Their names belong to their owners and are only used to say what Notifi works with. AI agents' output can be wrong; check it before relying on it.</small></span>
  </div>
  <div class="row">
    <span class="name">Clean room<small>Written only from behaviour specs drawn from public docs and screenshots. No other project's code is read or copied. The name, icon and look are Notifi's own.</small></span>
  </div>
  <div class="row">
    <span class="name">Release notes<small>What changed in each version.</small></span>
    <button class="ghost" onclick={() => invoke('open_release_notes')}>Open</button>
  </div>
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 16px;
    border-bottom: 1px solid var(--line);
  }
  .head img {
    width: 48px;
    height: 48px;
  }
  .title {
    font-size: 18px;
    font-weight: 700;
  }
  .sub {
    color: var(--mut);
    font: 12px var(--mono);
  }
</style>
