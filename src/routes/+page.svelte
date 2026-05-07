<script lang="ts">
  import { projectStore } from '$lib/projects.svelte';
  import ProjectCard from '$lib/components/ProjectCard.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import CreateMenu from '$lib/components/CreateMenu.svelte';

  // Sort: running first, then transitioning, then errored, then idle. Within a
  // group, alphabetical so the order is stable as statuses change.
  const STATUS_RANK: Record<string, number> = {
    running: 0,
    starting: 1,
    stopping: 2,
    error: 3,
    stopped: 4,
  };
  const projects = $derived(
    [...projectStore.filtered].sort((a, b) => {
      const sa = STATUS_RANK[a.status] ?? 99;
      const sb = STATUS_RANK[b.status] ?? 99;
      if (sa !== sb) return sa - sb;
      return a.name.localeCompare(b.name);
    }),
  );

  const total = $derived(projectStore.projects.length);
  const runningCount = $derived(
    projectStore.projects.filter((p) => p.status === 'running').length,
  );
  const idleCount = $derived(
    projectStore.projects.filter((p) => p.status === 'stopped').length,
  );
  const errorCount = $derived(
    projectStore.projects.filter((p) => p.status === 'error').length,
  );
  const transitioningCount = $derived(
    projectStore.projects.filter(
      (p) => p.status === 'starting' || p.status === 'stopping',
    ).length,
  );
  const stoppedCount = $derived(idleCount + errorCount);
  let bulkBusy = $state(false);

  async function startAll() {
    if (bulkBusy) return;
    bulkBusy = true;
    try {
      await projectStore.startAll();
    } finally {
      bulkBusy = false;
    }
  }

  async function stopAll() {
    if (bulkBusy) return;
    bulkBusy = true;
    try {
      await projectStore.stopAll();
    } finally {
      bulkBusy = false;
    }
  }

  $effect(() => {
    const tick = () => projectStore.loadAllGitStatuses();
    const handle = setInterval(tick, 30_000);
    return () => clearInterval(handle);
  });

  // Stats are now polled by the layout so the sidebar sees them too — no
  // duplicate polling needed here.
</script>

<header class="page-header" data-tauri-drag-region>
  <div class="title-block" data-tauri-drag-region>
    <div>
      <h1>Projects</h1>
      <p class="subtitle">
        {#if total === 0}
          Get started with your first Laravel project
        {:else}
          {total} {total === 1 ? 'project' : 'projects'} managed by Sail
        {/if}
      </p>
    </div>
  </div>

  <div class="header-actions">
    <div class="search">
      <Icon name="search" size={13} />
      <input
        type="text"
        placeholder="Search projects…"
        bind:value={projectStore.search}
        autocomplete="off"
        spellcheck="false"
      />
    </div>
    <div class="create-menu-wrap">
      <CreateMenu />
    </div>
  </div>
</header>

{#if total > 0}
  <section class="stat-strip">
    <div class="stat" class:active={runningCount > 0}>
      <span class="stat-dot dot-running"></span>
      <span class="stat-num">{runningCount}</span>
      <span class="stat-label">running</span>
    </div>
    {#if transitioningCount > 0}
      <div class="stat active">
        <span class="stat-dot dot-warning"></span>
        <span class="stat-num">{transitioningCount}</span>
        <span class="stat-label">transitioning</span>
      </div>
    {/if}
    <div class="stat">
      <span class="stat-dot dot-idle"></span>
      <span class="stat-num">{idleCount}</span>
      <span class="stat-label">idle</span>
    </div>
    {#if errorCount > 0}
      <div class="stat active">
        <span class="stat-dot dot-error"></span>
        <span class="stat-num">{errorCount}</span>
        <span class="stat-label">error</span>
      </div>
    {/if}

    <div class="bulk">
      {#if runningCount > 0}
        <button class="btn btn-ghost small" onclick={stopAll} disabled={bulkBusy}>
          <Icon name="stop" size={11} /> Stop all
        </button>
      {/if}
      {#if stoppedCount > 0}
        <button class="btn btn-ghost small" onclick={startAll} disabled={bulkBusy}>
          <Icon name="play" size={11} /> Start all
        </button>
      {/if}
    </div>
  </section>
{/if}

<section class="rows">
  {#each projects as project (project.id)}
    <ProjectCard {project} />
  {/each}

  {#if projects.length === 0 && total > 0}
    <div class="empty small">
      <Icon name="search" size={20} />
      <p>No projects match "{projectStore.search}"</p>
      <button class="btn btn-ghost" onclick={() => (projectStore.search = '')}>Clear search</button>
    </div>
  {/if}

  {#if total === 0}
    <div class="empty">
      <div class="empty-mark"><Icon name="waves" size={28} /></div>
      <h2>No projects yet</h2>
      <p>Create a fresh Laravel project, clone one from Git, or import a Sail project you already have.</p>
      <div class="empty-action">
        <CreateMenu />
      </div>
    </div>
  {/if}
</section>

<style>
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 22px 28px 18px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background:
      linear-gradient(180deg, var(--bg-1) 0%, var(--bg) 100%);
    backdrop-filter: blur(8px) saturate(140%);
    -webkit-backdrop-filter: blur(8px) saturate(140%);
    z-index: 5;
  }
  .title-block {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.03em;
  }
  .subtitle {
    margin: 2px 0 0;
    color: var(--text-dim);
    font-size: 12px;
  }

  .stat-strip {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px 28px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
  }
  .stat {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 5px 11px 5px 9px;
    border-radius: 999px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    font-size: 11.5px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .stat.active {
    background: var(--bg-3);
    border-color: var(--border-strong);
  }
  .stat-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-faint);
    flex-shrink: 0;
  }
  .stat-num {
    font-size: 13px;
    font-weight: 650;
    color: var(--text);
    letter-spacing: -0.02em;
  }
  .stat-label {
    text-transform: lowercase;
  }
  .dot-running {
    background: var(--success);
    box-shadow: 0 0 6px var(--success-glow);
  }
  .dot-idle {
    background: var(--text-faint);
  }
  .dot-warning {
    background: var(--warning);
  }
  .dot-error {
    background: var(--error);
  }
  .bulk {
    margin-left: auto;
    display: flex;
    gap: 6px;
  }
  .btn.small {
    font-size: 11px;
    padding: 4px 9px;
  }

  .header-actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    color: var(--text-dim);
    transition: border-color 0.15s;
  }
  .search:focus-within {
    border-color: var(--accent);
    color: var(--text);
  }
  .search input {
    border: none;
    background: transparent;
    padding: 6px 0;
    width: 220px;
    color: var(--text);
  }
  .search input:focus {
    border: none;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 18px 28px 28px;
    max-width: 1100px;
    margin: 0 auto;
    width: 100%;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 72px 24px;
    margin-top: 32px;
    color: var(--text-dim);
    text-align: center;
    background: linear-gradient(180deg, var(--bg-2) 0%, transparent 100%);
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
  }
  .empty.small {
    padding: 40px 24px;
    margin-top: 0;
    background: transparent;
    border: none;
  }
  .empty-mark {
    width: 64px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 16px;
    background: linear-gradient(135deg, var(--accent-soft) 0%, transparent 100%);
    color: var(--accent);
    margin-bottom: 4px;
  }
  .empty h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
    color: var(--text);
    letter-spacing: -0.02em;
  }
  .empty p {
    margin: 0;
    max-width: 420px;
    line-height: 1.5;
  }
  .empty-action {
    margin-top: 14px;
    width: 240px;
  }
  .create-menu-wrap {
    width: 180px;
  }
</style>
