<script lang="ts">
  const items = [
    { name: 'Input devices', why: 'Global keys and mouse remaps, and watching typed keys for text snippet triggers. On Linux Wayland this means joining the input group.', usedBy: 'Keyboard and mouse, Text snippets' },
    { name: 'Screen capture', why: 'Screenshots, recordings, app switcher previews and Watch.', usedBy: 'Utilities, Windows, Island' },
    { name: 'Microphone and camera', why: 'Recording audio and showing a camera preview. Nothing is sent anywhere.', usedBy: 'Utilities, Sound' },
    { name: 'Notifications', why: 'Reads Windows notifications for dynamic cards on the Island, and shows alerts.', usedBy: 'Monitor, Island' },
    { name: 'Clipboard', why: 'Saves what you copy as unencrypted text and files on this PC.', usedBy: 'Clipboard history' },
    { name: 'App menus', why: 'Reads the menus of the app in front through Windows UI Automation.', usedBy: 'Command bar' },
    { name: 'AI tool files', why: 'Reads the session logs your coding agents write, and the plan name from Claude Code's login file. Nothing from them is sent anywhere.', usedBy: 'Island' },
    { name: 'Deleting files', why: 'Removes files you choose, to the Recycle Bin where Windows allows. Always asks first.', usedBy: 'Apps, Disk space' },
    { name: 'Internet', why: 'Updates (GitHub), lyrics (LRCLIB), weather (Open-Meteo), speed tests (LibreSpeed) and share links (your server).', usedBy: 'Updates, Island, Monitor, Utilities' },
    { name: 'Helper service', why: 'Admin rights for fans, sensors and monitor brightness.', usedBy: 'Monitor, Energy and display' },
    { name: 'Desktop extension', why: 'Window control on GNOME and KDE Wayland.', usedBy: 'Windows, Island' },
  ];
</script>

<div class="card">
  {#each items as item}
    <div class="row">
      <span class="name">{item.name}<small>{item.why} Used by {item.usedBy}.</small></span>
    </div>
  {/each}
</div>
<p class="foot">Every feature is optional. One you turn off uses none of this.</p>

<style>
  .foot {
    margin: 12px 4px 0;
    color: var(--mut);
    font-size: 12.5px;
  }
</style>
