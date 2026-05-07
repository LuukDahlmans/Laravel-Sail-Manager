<script lang="ts">
  import type { ProjectStatus } from '$lib/types';

  interface Props {
    status: ProjectStatus;
    showLabel?: boolean;
  }
  let { status, showLabel = true }: Props = $props();

  const labels: Record<ProjectStatus, string> = {
    running: 'Running',
    stopped: 'Stopped',
    starting: 'Starting…',
    stopping: 'Stopping…',
    error: 'Error',
  };
</script>

<span class="dot {status}" aria-label={labels[status]}>
  <span class="circle"></span>
  {#if showLabel}
    <span class="label">{labels[status]}</span>
  {/if}
</span>

<style>
  .dot {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 3px 9px 3px 8px;
    border-radius: 999px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .circle {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-faint);
    flex-shrink: 0;
  }
  .running {
    background: var(--success-soft);
    border-color: transparent;
  }
  .running .circle {
    background: var(--success);
    box-shadow: 0 0 0 3px var(--success-soft), 0 0 8px var(--success-glow);
    animation: live 2.4s ease-in-out infinite;
  }
  .running .label {
    color: var(--success);
  }
  .stopped .circle {
    background: var(--text-faint);
  }
  .starting,
  .stopping {
    background: var(--warning-soft);
    border-color: transparent;
  }
  .starting .circle,
  .stopping .circle {
    background: var(--warning);
    box-shadow: 0 0 0 3px var(--warning-soft);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .starting .label,
  .stopping .label {
    color: var(--warning);
  }
  .error {
    background: var(--error-soft);
    border-color: transparent;
  }
  .error .circle {
    background: var(--error);
    box-shadow: 0 0 0 3px var(--error-soft);
  }
  .error .label {
    color: var(--error);
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }
  /* "Heartbeat" — subtle, communicates that the project is genuinely alive
     without being distracting at small sizes. */
  @keyframes live {
    0%, 100% {
      box-shadow: 0 0 0 3px var(--success-soft), 0 0 6px var(--success-glow);
    }
    50% {
      box-shadow: 0 0 0 3px var(--success-soft), 0 0 12px var(--success-glow);
    }
  }
</style>
