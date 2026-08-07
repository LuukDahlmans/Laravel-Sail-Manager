<script lang="ts">
  import type { Project } from '$lib/types';
  import { projectStore } from '$lib/projects.svelte';
  import Icon from './Icon.svelte';
  import { goto } from '$app/navigation';
  import { openUrl } from '@tauri-apps/plugin-opener';

  interface Props {
    project: Project;
  }
  let { project }: Props = $props();

  const appPort = $derived(project.ports.find((p) => p.service === 'app'));
  const localhostUrl = $derived(appPort ? `http://localhost:${appPort.host}` : null);
  const localUrl = $derived(projectStore.localUrlFor(project));
  const url = $derived(localUrl ?? localhostUrl);
  const isBusy = $derived(project.status === 'starting' || project.status === 'stopping');
  const isRunning = $derived(project.status === 'running');

  $effect(() => {
    void project.id;
    if (!(project.id in projectStore.gitStatuses)) {
      projectStore.loadGitStatus(project.id);
    }
  });
  const gitStatus = $derived(projectStore.gitStatuses[project.id]);
  const liveStats = $derived(projectStore.statsFor(project));

  function formatBytes(n: number): string {
    if (n <= 0) return '0';
    const units = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i++;
    }
    return `${v.toFixed(v < 10 && i > 0 ? 1 : 0)} ${units[i]}`;
  }

  const cpuLabel = $derived(liveStats ? `${liveStats.cpuPercent.toFixed(0)}%` : '');
  const memLabel = $derived(liveStats ? formatBytes(liveStats.memUsedBytes) : '');
  // Bars: clamp CPU to 100 for the bar even though sum across containers can
  // technically exceed 100% (multi-core). Keep the numeric label honest, just
  // visually cap the bar.
  const cpuBar = $derived(
    liveStats ? Math.min(100, liveStats.cpuPercent) : 0,
  );
  const memBar = $derived(
    liveStats && liveStats.memLimitBytes > 0
      ? Math.min(100, (liveStats.memUsedBytes / liveStats.memLimitBytes) * 100)
      : 0,
  );

  const statusLabel = $derived(
    project.status === 'running'
      ? 'Running'
      : project.status === 'starting'
        ? 'Starting'
        : project.status === 'stopping'
          ? 'Stopping'
          : project.status === 'error'
            ? 'Error'
            : 'Idle',
  );

  // Service summary: comma-joined list, no chips. Cleaner.
  const servicesSummary = $derived(
    project.services.length === 0
      ? 'no services'
      : project.services.length <= 3
        ? project.services.join(' · ')
        : `${project.services.slice(0, 2).join(' · ')} +${project.services.length - 2} more`,
  );

  async function toggleStartStop(e: MouseEvent) {
    e.stopPropagation();
    if (isBusy) return;
    // The store surfaces the error (toast) and reconciles the stuck status;
    // swallow the rethrow here so the click handler doesn't reject uncaught.
    try {
      if (isRunning) {
        await projectStore.stop(project.id);
      } else {
        await projectStore.start(project.id);
      }
    } catch {
      // handled in the store
    }
  }

  async function openInBrowser(e: MouseEvent) {
    e.stopPropagation();
    if (!url) return;
    try {
      await openUrl(url);
    } catch (err) {
      projectStore.reportError(`Could not open ${url}: ${err}`);
    }
  }

  function navigate() {
    goto(`/projects/${project.id}`);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      navigate();
    }
  }
</script>

<div
  class="row"
  class:running={isRunning}
  class:error={project.status === 'error'}
  class:busy={isBusy}
  role="button"
  tabindex="0"
  onclick={navigate}
  onkeydown={onKey}
>
  <div class="indicator">
    <span class="status-dot"></span>
    {#if isRunning}<span class="ring"></span>{/if}
  </div>

  <div class="main">
    <div class="title">
      <span class="name">{project.name}</span>
      <span class="status-text">{statusLabel}</span>
    </div>
    <div class="secondary">
      {#if url}
        <a
          class="url"
          href={url}
          target="_blank"
          onclick={(e) => e.stopPropagation()}
        >{url.replace('http://', '')}</a>
      {:else}
        <span class="url placeholder">no port</span>
      {/if}
      <span class="dot-sep">·</span>
      <span class="meta">{servicesSummary}</span>
      {#if gitStatus}
        <span class="dot-sep">·</span>
        <span class="branch" class:dirty={gitStatus.dirty}>
          {gitStatus.branch}{#if gitStatus.dirty} <span class="dirty-mark">●</span>{/if}{#if gitStatus.ahead > 0} ↑{gitStatus.ahead}{/if}{#if gitStatus.behind > 0} ↓{gitStatus.behind}{/if}
        </span>
      {/if}
    </div>

    {#if isRunning && liveStats}
      <div class="resource-bars" aria-hidden="true">
        <div class="rb">
          <span class="rb-label">CPU</span>
          <div class="rb-track">
            <div
              class="rb-fill"
              class:rb-warn={cpuBar > 70}
              class:rb-hot={cpuBar > 95}
              style="width: {cpuBar}%"
            ></div>
          </div>
          <span class="rb-value">{cpuLabel}</span>
        </div>
        <div class="rb">
          <span class="rb-label">RAM</span>
          <div class="rb-track">
            <div
              class="rb-fill"
              class:rb-warn={memBar > 70}
              class:rb-hot={memBar > 90}
              style="width: {memBar}%"
            ></div>
          </div>
          <span class="rb-value">{memLabel}</span>
        </div>
      </div>
    {/if}
  </div>

  <div class="actions" onclick={(e) => e.stopPropagation()} role="presentation">
    <button
      class="btn primary-action"
      class:running={isRunning}
      onclick={toggleStartStop}
      disabled={isBusy}
      title={isRunning ? 'Stop' : 'Start'}
    >
      {#if isBusy}
        <span class="spinner"></span>
        {project.status === 'starting' ? 'Starting' : 'Stopping'}
      {:else if isRunning}
        <Icon name="stop" size={11} />
        Stop
      {:else}
        <Icon name="play" size={11} />
        Start
      {/if}
    </button>
    <button
      class="btn icon-btn"
      onclick={openInBrowser}
      disabled={!isRunning}
      title="Open in browser"
    >
      <Icon name="external" size={13} />
    </button>
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    gap: 14px;
    align-items: center;
    padding: 14px 18px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 12px;
    cursor: pointer;
    transition:
      background 0.15s var(--ease-quick),
      border-color 0.15s var(--ease-quick),
      transform 0.08s var(--ease-quick);
    text-align: left;
    width: 100%;
  }
  .row:hover {
    background: var(--bg-3);
    border-color: var(--border-strong);
  }
  .row:focus-visible {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  /* Status indicator: a single solid dot, color tracks status. The running
     state gets a soft expanding ring layered behind so it visibly "lives"
     without dominating the row. */
  .indicator {
    width: 28px;
    height: 28px;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .status-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text-faint);
    z-index: 1;
    transition: background 0.15s var(--ease-quick);
  }
  .running .status-dot {
    background: var(--success);
  }
  .error .status-dot {
    background: var(--error);
  }
  .busy .status-dot {
    background: var(--warning);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .ring {
    position: absolute;
    inset: 4px;
    border-radius: 50%;
    border: 1.5px solid var(--success);
    opacity: 0.55;
    animation: ring-expand 2.4s ease-out infinite;
  }

  .main {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }
  .name {
    font-size: 16px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status-text {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .running .status-text { color: var(--success); }
  .error .status-text { color: var(--error); }
  .busy .status-text { color: var(--warning); }

  .secondary {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-dim);
    font-size: 12px;
    min-width: 0;
    overflow: hidden;
  }
  .dot-sep {
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .url {
    color: var(--text-dim);
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    max-width: 280px;
  }
  .url:hover { color: var(--accent); }
  .url.placeholder {
    font-style: italic;
    color: var(--text-faint);
  }
  .meta {
    flex-shrink: 0;
    text-transform: lowercase;
  }
  .branch {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 11.5px;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .branch.dirty {
    color: var(--warning);
  }
  .dirty-mark {
    font-size: 10px;
  }

  .resource-bars {
    display: flex;
    gap: 14px;
    margin-top: 6px;
  }
  .rb {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .rb-label {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    text-transform: uppercase;
  }
  .rb-track {
    flex: 1;
    height: 4px;
    background: var(--bg-3);
    border-radius: 999px;
    overflow: hidden;
    min-width: 50px;
    max-width: 130px;
  }
  .rb-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--success) 0%, color-mix(in oklab, var(--success), white 20%) 100%);
    box-shadow: 0 0 6px var(--success-glow);
    border-radius: 999px;
    transition: width 0.4s var(--ease);
  }
  .rb-fill.rb-warn {
    background: linear-gradient(90deg, var(--warning) 0%, color-mix(in oklab, var(--warning), white 20%) 100%);
    box-shadow: none;
  }
  .rb-fill.rb-hot {
    background: linear-gradient(90deg, var(--error) 0%, color-mix(in oklab, var(--error), white 20%) 100%);
    box-shadow: none;
  }
  .rb-value {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    min-width: 38px;
  }

  .actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
    opacity: 0.6;
    transition: opacity 0.15s var(--ease-quick);
  }
  .row:hover .actions,
  .row:focus-within .actions,
  .row.running .actions,
  .row.busy .actions {
    opacity: 1;
  }
  .primary-action {
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 600;
  }
  .primary-action.running {
    background: var(--bg-3);
    border-color: var(--border-strong);
    color: var(--text);
  }
  .primary-action:not(.running):not(:disabled) {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      0 1px 0 rgba(0, 0, 0, 0.12);
  }
  .primary-action:not(.running):not(:disabled):hover {
    background: var(--accent-hover);
  }
  .icon-btn {
    width: 30px;
    height: 30px;
    padding: 0;
    background: transparent;
    border-color: transparent;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--bg-3);
    border-color: var(--border-strong);
    color: var(--text);
  }
  .icon-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .spinner {
    width: 11px;
    height: 11px;
    border: 1.5px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }
  @keyframes ring-expand {
    0% { transform: scale(0.6); opacity: 0.6; }
    100% { transform: scale(1.6); opacity: 0; }
  }
</style>
