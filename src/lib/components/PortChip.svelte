<script lang="ts">
  import type { Port } from '$lib/types';

  interface Props {
    port: Port;
    clickable?: boolean;
  }
  let { port, clickable = false }: Props = $props();
</script>

<span class="chip" class:clickable>
  <span class="label">{port.label}</span>
  <span class="port selectable">:{port.host}</span>
</span>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .label {
    color: var(--text-dim);
    font-weight: 500;
  }
  .port {
    color: var(--text);
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  }
  .clickable:hover {
    background: var(--bg-4);
    cursor: pointer;
  }
</style>
