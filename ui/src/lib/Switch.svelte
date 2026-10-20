<script lang="ts">
  let { checked, label, onchange }: { checked: boolean; label: string; onchange: (on: boolean) => void } = $props();
</script>

<button class="switch" role="switch" aria-checked={checked} aria-label={label} onclick={() => onchange(!checked)}>
  <span class="knob"></span>
</button>

<style>
  .switch {
    --w: 38px;
    --h: 22px;
    position: relative;
    flex: none;
    width: var(--w);
    height: var(--h);
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: 99px;
    background: var(--track);
    cursor: pointer;
    transition: background-color var(--m-toggleSwitch-dur) var(--m-toggleSwitch-ease),
      border-color var(--m-toggleSwitch-dur) var(--m-toggleSwitch-ease);
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--knob);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform var(--m-toggleSwitch-dur) var(--m-toggleSwitch-ease);
  }
  .switch[aria-checked='true'] {
    background: var(--acc);
    border-color: var(--acc);
  }
  .switch[aria-checked='true'] .knob {
    transform: translateX(16px);
    background: #fff;
  }
  .switch:focus-visible {
    outline: 2px solid var(--acc);
    outline-offset: 2px;
  }
</style>
