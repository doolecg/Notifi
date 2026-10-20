<script lang="ts">
  let { value, tone = 'var(--acc)' }: { value: number; tone?: string } = $props();
</script>

<span class="bar"><span style:width="{Math.min(100, Math.max(0, value))}%" style:background={tone}></span></span>

<style>
  .bar {
    display: block;
    height: 6px;
    border-radius: 99px;
    background: var(--track);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    transition: width var(--m-tabSwitch-dur) var(--m-tabSwitch-ease);
  }
</style>
