<script lang="ts">
  import { slide, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { projectStore } from '$lib/projects.svelte';
  import { toast } from '$lib/toast.svelte';
  import Icon from './Icon.svelte';
  import type { UntrackedSailProject } from '$lib/types';

  const candidates = $derived(projectStore.pendingSailImports);
  const importable = $derived(candidates.filter((c) => c.importable));
  const runningCount = $derived(candidates.filter((c) => c.running).length);
  const busy = $derived(projectStore.adopting !== null);

  let expanded = $state(false);
  let importingAll = $state(false);

  // Collapse when the last candidate goes away, so re-appearing later starts
  // from the compact bar again.
  $effect(() => {
    if (candidates.length === 0) expanded = false;
  });

  function shortPath(path: string): string {
    const home = '/Users/';
    if (!path.startsWith(home)) return path;
    const rest = path.slice(home.length);
    const slash = rest.indexOf('/');
    return slash === -1 ? path : `~${rest.slice(slash)}`;
  }

  function appPortOf(project: { ports: { service: string; host: number }[] }): number | undefined {
    return project.ports.find((p) => p.service === 'app')?.host;
  }

  async function importOne(c: UntrackedSailProject) {
    try {
      const outcome = await projectStore.adoptSailProject(c.composeProject);
      if (outcome.needsRestart) {
        toast.warning(
          `${outcome.project.name} was on a port another project already claims, so it moved to ${appPortOf(outcome.project)}. Restart it to apply.`,
          'Imported with new ports',
        );
      } else if (outcome.pinnedKeys.length > 0) {
        toast.success(
          `Wrote ${outcome.pinnedKeys.join(', ')} into .env so its ports can't collide.`,
          `Imported ${outcome.project.name}`,
        );
      } else {
        toast.success(`${outcome.project.name} is now managed by Sail Manager.`, 'Imported');
      }
    } catch {
      // adoptSailProject already routed the error to a toast.
    }
  }

  async function importAll() {
    if (importingAll) return;
    importingAll = true;
    try {
      for (const c of [...importable]) {
        await importOne(c);
      }
    } finally {
      importingAll = false;
    }
  }

  async function dismissAll() {
    for (const c of [...candidates]) {
      await projectStore.dismissSailImport(c.composeProject);
    }
  }
</script>

{#if candidates.length > 0}
  <div
    class="banner"
    role="region"
    aria-label="Sail projects not managed by Sail Manager"
    transition:fly={{ y: 24, duration: 240, easing: cubicOut }}
  >
    <div class="inner">
      {#if expanded}
        <ul class="list" transition:slide={{ duration: 180, easing: cubicOut }}>
          {#each candidates as c (c.composeProject)}
            <li class="row" class:blocked={!c.importable}>
              <div class="row-main">
                <span class="dot" class:live={c.running}></span>
                <span class="row-name">{c.name || c.composeProject}</span>
                <span class="row-path" title={c.path}>{shortPath(c.path)}</span>
              </div>
              <div class="row-meta">
                <span class="chip">{c.running ? 'running' : 'stopped'}</span>
                {#if c.phpVersion}
                  <span class="chip">PHP {c.phpVersion}</span>
                {/if}
                {#if c.running && c.appPort}
                  <span class="chip">:{c.appPort}</span>
                {/if}
                <span class="chip">{c.services.length} containers</span>
              </div>
              {#if c.importable}
                <button
                  class="btn small"
                  onclick={() => importOne(c)}
                  disabled={busy || importingAll}
                >
                  {projectStore.adopting === c.composeProject ? 'Importing…' : 'Import'}
                </button>
              {:else}
                <span class="blocked-why">Can't import — {c.blockedReason}</span>
              {/if}
              <button
                class="btn btn-ghost btn-icon"
                onclick={() => projectStore.dismissSailImport(c.composeProject)}
                aria-label="Ignore {c.name}"
                title="Ignore this project"
              >
                <Icon name="x" size={12} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <div class="bar">
        <div class="mark"><Icon name="box" size={14} /></div>
        <div class="text">
          <span class="headline">
            {candidates.length} Sail {candidates.length === 1 ? 'project' : 'projects'} in Docker
            {candidates.length === 1 ? "isn't" : "aren't"} managed by Sail Manager
          </span>
          <span class="sub">
            {#if expanded}
              Importing reserves each project's ports. Running stacks keep the ports they're on;
              stopped ones get free ones.
            {:else}
              {runningCount > 0 ? `${runningCount} running · ` : ''}{candidates
                .map((c) => c.name || c.composeProject)
                .slice(0, 3)
                .join(', ')}{candidates.length > 3 ? `, +${candidates.length - 3} more` : ''}
            {/if}
          </span>
        </div>

        <div class="actions">
          <button class="btn btn-ghost small" onclick={() => (expanded = !expanded)}>
            {expanded ? 'Hide' : 'Details'}
          </button>
          <button class="btn btn-ghost small" onclick={dismissAll} disabled={busy || importingAll}>
            Ignore
          </button>
          {#if importable.length > 0}
            <button
              class="btn btn-primary small"
              onclick={importAll}
              disabled={busy || importingAll}
            >
              {#if importingAll}
                Importing…
              {:else if importable.length === 1}
                Import {importable[0].name || importable[0].composeProject}
              {:else}
                Import all {importable.length}
              {/if}
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .banner {
    position: fixed;
    left: 230px;
    right: 0;
    bottom: 0;
    /* Below the toaster (100) so transient toasts still read on top. */
    z-index: 40;
    background: var(--bg-1);
    border-top: 1px solid var(--border-strong);
    box-shadow: 0 -8px 28px rgba(0, 0, 0, 0.18);
    backdrop-filter: blur(12px) saturate(140%);
    -webkit-backdrop-filter: blur(12px) saturate(140%);
  }
  .inner {
    max-width: 1100px;
    margin: 0 auto;
    padding: 0 28px;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 0;
  }
  .mark {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    background: var(--info-soft);
    color: var(--info);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }
  .headline {
    font-size: 12.5px;
    font-weight: 550;
    letter-spacing: -0.01em;
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .btn.small {
    font-size: 11px;
    padding: 4px 9px;
  }

  .list {
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid var(--border);
    padding: 8px 0;
    max-height: 240px;
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 0;
    font-size: 11.5px;
  }
  .row.blocked {
    opacity: 0.6;
  }
  .row-main {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    align-self: center;
    background: var(--text-faint);
  }
  .dot.live {
    background: var(--success);
    box-shadow: 0 0 6px var(--success-glow);
  }
  .row-name {
    font-weight: 550;
    color: var(--text);
  }
  .row-path {
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-meta {
    display: flex;
    gap: 5px;
    flex-shrink: 0;
  }
  .chip {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    font-size: 10.5px;
  }
  .blocked-why {
    color: var(--warning);
    font-size: 11px;
  }
</style>
