<script lang="ts">
  // Custom options of the Monitor page (spec/monitor/fan-control.md): only `curves`.
  // Windows has no fan interface in the system. Reading RPM or setting a speed needs a vendor
  // driver, which Notifi does not ship, so the control states that instead of drawing a graph
  // that could not do anything.
  import type { CustomProps } from '../sections/customs';

  let { option }: CustomProps = $props();
</script>

{#if option.id === 'curves'}
  <div class="row">
    <span class="name">{option.label}<small>Windows gives apps no way to read or set fan speeds without a vendor driver, and Notifi does not install one. Your fans stay under the system's control, and the options below have no effect on this PC.</small></span>
    <span class="state">Not available</span>
  </div>
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
  }
  small {
    color: var(--mut);
    font-size: 11.5px;
  }
  .state {
    flex: none;
    color: var(--mut);
    font-size: 12.5px;
  }
</style>
