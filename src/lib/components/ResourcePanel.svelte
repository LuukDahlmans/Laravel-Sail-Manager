<script lang="ts">
  import { projectStore } from '$lib/projects.svelte';
  import Icon from './Icon.svelte';
  import type { ContainerStat } from '$lib/types';

  interface Props {
    projectId: string;
    /** Only poll when both this is true AND the project is running. */
    active: boolean;
    running: boolean;
  }
  let { projectId, active, running }: Props = $props();

  let stats = $state<ContainerStat[]>([]);
  let error = $state<string | null>(null);
  let lastFetched = $state<number | null>(null);
  let loading = $state(false);

  async function fetchOnce() {
    if (loading) return;
    loading = true;
    try {
      const next = await projectStore.getProjectStats(projectId);
      // Sort by container name for stable rendering; sail's `laravel.test`
      // service container is what users care about most so push it first.
      next.sort((a, b) => {
        const aIsApp = a.name.includes('laravel.test');
        const bIsApp = b.name.includes('laravel.test');
        if (aIsApp && !bIsApp) return -1;
        if (bIsApp && !aIsApp) return 1;
        return a.name.localeCompare(b.name);
      });
      stats = next;
      error = null;
      lastFetched = Date.now();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Poll while active + running. The cleanup tears down the interval as soon
  // as the user leaves the Overview tab or the project stops.
  $effect(() => {
    if (!active || !running) {
      stats = [];
      error = null;
      lastFetched = null;
      return;
    }
    fetchOnce();
    const handle = setInterval(fetchOnce, 3000);
    return () => clearInterval(handle);
  });

  // Pull a numeric percent out of "0.18%" so we can render the bar width.
  function parsePercent(raw: string): number {
    const match = raw.match(/([0-9]+(?:\.[0-9]+)?)/);
    if (!match) return 0;
    const n = Number.parseFloat(match[1]);
    return Number.isFinite(n) ? Math.min(100, Math.max(0, n)) : 0;
  }

  // Strip the compose-project prefix so the visible name is just the service.
  // Sail container names look like `<compose>-laravel.test-1`. If the format
  // doesn't match, we just return the original.
  function shortName(name: string): string {
    const trimmed = name.replace(/-1$/, '');
    const dash = trimmed.indexOf('-');
    return dash >= 0 ? trimmed.slice(dash + 1) : trimmed;
  }
</script>

<div class="panel resource-panel">
  <div class="head">
    <h3>
      <Icon name="activity" size={12} />
      Resource usage
    </h3>
    <span class="hint">
      {#if !running}
        Project is not running
      {:else if error}
        <span class="err">{error}</span>
      {:else if stats.length === 0 && lastFetched !== null}
        No containers reported
      {:else}
        Refreshes every 3s
      {/if}
    </span>
  </div>

  {#if running && stats.length > 0}
    <div class="rows">
      {#each stats as stat (stat.name)}
        {@const cpu = parsePercent(stat.cpuPercent)}
        {@const mem = parsePercent(stat.memPercent)}
        <div class="row">
          <div class="row-head">
            <span class="cname mono">{shortName(stat.name)}</span>
            <span class="pids">{stat.pids} pids</span>
          </div>
          <div class="bars">
            <div class="bar-line" title="CPU {stat.cpuPercent}">
              <span class="bar-label">CPU</span>
              <div class="bar-track">
                <div class="bar-fill cpu" style="width: {cpu}%"></div>
              </div>
              <span class="bar-value mono">{stat.cpuPercent}</span>
            </div>
            <div class="bar-line" title="Memory {stat.memUsage} ({stat.memPercent})">
              <span class="bar-label">MEM</span>
              <div class="bar-track">
                <div class="bar-fill mem" style="width: {mem}%"></div>
              </div>
              <span class="bar-value mono">{stat.memPercent}</span>
            </div>
          </div>
          <div class="io-row">
            <span class="io-label">net</span>
            <span class="io-val mono">{stat.netIo}</span>
            <span class="io-sep">·</span>
            <span class="io-label">block</span>
            <span class="io-val mono">{stat.blockIo}</span>
            <span class="io-sep">·</span>
            <span class="io-label">mem</span>
            <span class="io-val mono">{stat.memUsage}</span>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .resource-panel {
    margin-top: 14px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
    gap: 10px;
  }
  h3 {
    margin: 0;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .hint {
    font-size: 11px;
    color: var(--text-faint);
  }
  .err {
    color: var(--error);
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .row-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .cname {
    font-size: 12px;
    color: var(--text);
    font-weight: 500;
  }
  .pids {
    font-size: 10.5px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .bars {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .bar-line {
    display: grid;
    grid-template-columns: 32px 1fr 56px;
    align-items: center;
    gap: 8px;
  }
  .bar-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
    color: var(--text-faint);
  }
  .bar-track {
    height: 4px;
    background: var(--bg-3);
    border-radius: 999px;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    border-radius: 999px;
    transition: width 0.4s var(--ease-quick);
  }
  .bar-fill.cpu {
    background: var(--accent);
  }
  .bar-fill.mem {
    background: var(--success);
  }
  .bar-value {
    font-size: 10.5px;
    color: var(--text-dim);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .io-row {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 10.5px;
    color: var(--text-faint);
  }
  .io-label {
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .io-val {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .io-sep {
    color: var(--text-faint);
    opacity: 0.6;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  }
</style>
