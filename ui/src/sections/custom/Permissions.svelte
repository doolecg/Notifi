<script lang="ts">
  const items = [
    { name: 'Input devices', why: 'Global keys and mouse remaps. On Linux Wayland this means joining the input group.', usedBy: 'Keyboard and mouse, Windows' },
    { name: 'Screen capture', why: 'Screenshots, recordings, app switcher previews and Watch.', usedBy: 'Utilities, Windows, Island' },
    { name: 'Microphone and camera', why: 'Recording audio and showing a camera preview.', usedBy: 'Utilities, Sound' },
    { name: 'Notifications', why: 'Alerts when something you watch changes.', usedBy: 'Monitor, Island' },
    { name: 'Helper service', why: 'Admin rights for fans, sensors and monitor brightness.', usedBy: 'Monitor, Energy and display' },
    { name: 'Desktop extension', why: 'Window control on GNOME and KDE Wayland.', usedBy: 'Windows, Island' },
  ];
</script>

<div class="card">
  {#each items as item}
    <div class="row">
      <span class="name">{item.name}<small>{item.why} Used by {item.usedBy}.</small></span>
      <span class="chip">Not needed yet</span>
    </div>
  {/each}
</div>
<p class="foot">Notifi asks for a permission only when you turn on a feature that needs it.</p>

<style>
  .chip {
    flex: none;
    padding: 3px 10px;
    border-radius: 99px;
    background: var(--inset);
    color: var(--mut);
    font-size: 12px;
    font-weight: 600;
  }
  .foot {
    margin: 12px 4px 0;
    color: var(--mut);
    font-size: 12.5px;
  }
</style>
