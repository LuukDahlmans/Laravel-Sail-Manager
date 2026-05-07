<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { goto } from '$app/navigation';
  import { projectStore } from '$lib/projects.svelte';
  import { ui } from '$lib/uiState.svelte';
  import type { Project, ProjectStatus } from '$lib/types';
  import Icon from './Icon.svelte';

  type Item = {
    id: string;
    icon: string;
    title: string;
    subtitle?: string;
    hint?: string;
    group: string;
    run: () => void | Promise<void>;
  };

  let open = $state(false);
  let query = $state('');
  let highlight = $state(0);
  let inputEl: HTMLInputElement | null = $state(null);
  let listEl: HTMLDivElement | null = $state(null);

  const isMac =
    typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);

  function show() {
    query = '';
    highlight = 0;
    open = true;
  }

  function hide() {
    open = false;
  }

  function statusLabel(s: ProjectStatus): string {
    switch (s) {
      case 'running':
        return 'running';
      case 'stopped':
        return 'stopped';
      case 'starting':
        return 'starting…';
      case 'stopping':
        return 'stopping…';
      case 'error':
        return 'error';
    }
  }

  function statusIcon(s: ProjectStatus): string {
    if (s === 'running') return 'play';
    if (s === 'error') return 'x';
    return 'box';
  }

  const projectItems = $derived.by<Item[]>(() =>
    projectStore.projects.map((p: Project) => ({
      id: `project:${p.id}`,
      icon: statusIcon(p.status),
      title: p.name,
      subtitle: statusLabel(p.status),
      hint: 'Open',
      group: 'Projects',
      run: () => goto(`/projects/${p.id}`),
    })),
  );

  const projectQuickActions = $derived.by<Item[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const matches = projectStore.projects.filter((p) =>
      p.name.toLowerCase().includes(q),
    );
    const out: Item[] = [];
    for (const p of matches) {
      if (p.status === 'stopped' || p.status === 'error') {
        out.push({
          id: `start:${p.id}`,
          icon: 'play',
          title: `Start ${p.name}`,
          subtitle: 'docker compose up -d',
          group: 'Project actions',
          run: () => projectStore.start(p.id),
        });
      }
      if (p.status === 'running' || p.status === 'error') {
        out.push({
          id: `stop:${p.id}`,
          icon: 'stop',
          title: `Stop ${p.name}`,
          subtitle: 'docker compose down',
          group: 'Project actions',
          run: () => projectStore.stop(p.id),
        });
      }
      if (p.status === 'running') {
        out.push({
          id: `open:${p.id}`,
          icon: 'external',
          title: `Open ${p.name} in browser`,
          subtitle: projectStore.localUrlFor(p) ?? `localhost:${p.ports.find((x) => x.service === 'app')?.host ?? ''}`,
          group: 'Project actions',
          run: async () => {
            const url =
              projectStore.localUrlFor(p) ??
              `http://localhost:${p.ports.find((x) => x.service === 'app')?.host ?? ''}`;
            const opener = await import('@tauri-apps/plugin-opener');
            await opener.openUrl(url);
          },
        });
      }
    }
    return out;
  });

  const staticActions = $derived.by<Item[]>(() => {
    const list: Item[] = [
      {
        id: 'act:create',
        icon: 'plus',
        title: 'Create new project',
        subtitle: 'Scaffold a fresh Laravel app via Sail',
        group: 'Actions',
        run: () => {
          ui.showCreateModal = true;
        },
      },
      {
        id: 'act:clone',
        icon: 'external',
        title: 'Clone from Git',
        subtitle: 'Pull an existing repo and register it',
        group: 'Actions',
        run: () => {
          ui.showCloneModal = true;
        },
      },
      {
        id: 'act:import',
        icon: 'folder',
        title: 'Import existing project',
        subtitle: 'Register a folder that already has Sail',
        group: 'Actions',
        run: () => {
          ui.showImportModal = true;
        },
      },
      {
        id: 'act:settings',
        icon: 'settings',
        title: 'Open Settings',
        group: 'Navigate',
        run: () => goto('/settings'),
      },
      {
        id: 'act:templates',
        icon: 'layers',
        title: 'Open Templates',
        group: 'Navigate',
        run: () => goto('/templates'),
      },
      {
        id: 'act:start-all',
        icon: 'play',
        title: 'Start all',
        subtitle: 'Bring up every stopped project',
        group: 'Bulk',
        run: () => projectStore.startAll(),
      },
      {
        id: 'act:stop-all',
        icon: 'stop',
        title: 'Stop all',
        subtitle: 'Tear down every running project',
        group: 'Bulk',
        run: () => projectStore.stopAll(),
      },
    ];
    if (projectStore.settings?.localUrlsEnabled) {
      list.push({
        id: 'act:resync',
        icon: 'refresh',
        title: 'Resync local URLs',
        subtitle: 'Rebuild Traefik + dnsmasq config',
        group: 'Bulk',
        run: () => projectStore.resyncLocalUrls(),
      });
    }
    return list;
  });

  const items = $derived.by<Item[]>(() => {
    const all = [...projectItems, ...projectQuickActions, ...staticActions];
    const q = query.trim().toLowerCase();
    if (!q) return all;
    return all.filter((it) => {
      const hay = `${it.title} ${it.subtitle ?? ''}`.toLowerCase();
      return hay.includes(q);
    });
  });

  $effect(() => {
    void items;
    highlight = 0;
  });

  $effect(() => {
    if (open && inputEl) {
      const el = inputEl;
      queueMicrotask(() => el.focus());
    }
  });

  $effect(() => {
    function onKey(e: KeyboardEvent) {
      const mod = isMac ? e.metaKey : e.ctrlKey;
      if (mod && (e.key === 'k' || e.key === 'K')) {
        e.preventDefault();
        if (open) hide();
        else show();
        return;
      }
      if (!open) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        hide();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        if (items.length === 0) return;
        highlight = (highlight + 1) % items.length;
        scrollToHighlight();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        if (items.length === 0) return;
        highlight = (highlight - 1 + items.length) % items.length;
        scrollToHighlight();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        runActive();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  function scrollToHighlight() {
    queueMicrotask(() => {
      const el = listEl?.querySelector<HTMLElement>(`[data-idx="${highlight}"]`);
      el?.scrollIntoView({ block: 'nearest' });
    });
  }

  async function runActive() {
    const it = items[highlight];
    if (!it) return;
    hide();
    try {
      await it.run();
    } catch {
      // store/toast bridge handles surfacing
    }
  }

  function onItemClick(idx: number) {
    highlight = idx;
    runActive();
  }

  function onBackdrop() {
    hide();
  }

  const grouped = $derived.by(() => {
    const out: Array<{ group: string; items: Array<{ item: Item; idx: number }> }> = [];
    items.forEach((item, idx) => {
      const last = out[out.length - 1];
      if (last && last.group === item.group) {
        last.items.push({ item, idx });
      } else {
        out.push({ group: item.group, items: [{ item, idx }] });
      }
    });
    return out;
  });

  const modKey = isMac ? '⌘' : 'Ctrl';
</script>

{#if open}
  <div
    class="backdrop"
    onclick={onBackdrop}
    role="presentation"
    transition:fade={{ duration: 120 }}
  ></div>

  <div
    class="palette"
    role="dialog"
    aria-modal="true"
    aria-label="Command palette"
    transition:scale={{ duration: 140, start: 0.97, opacity: 0 }}
  >
    <div class="search">
      <Icon name="search" size={14} />
      <input
        bind:this={inputEl}
        bind:value={query}
        type="text"
        placeholder="Search projects and actions…"
        spellcheck="false"
        autocomplete="off"
      />
      <span class="kbd">esc</span>
    </div>

    <div class="list" bind:this={listEl}>
      {#if items.length === 0}
        <div class="empty">No matches for "{query}"</div>
      {:else}
        {#each grouped as section (section.group)}
          <div class="section-label">{section.group}</div>
          {#each section.items as { item, idx } (item.id)}
            <button
              type="button"
              class="row"
              class:active={idx === highlight}
              data-idx={idx}
              onclick={() => onItemClick(idx)}
              onmousemove={() => (highlight = idx)}
            >
              <span class="row-icon"><Icon name={item.icon} size={14} /></span>
              <span class="row-text">
                <span class="row-title">{item.title}</span>
                {#if item.subtitle}
                  <span class="row-sub">{item.subtitle}</span>
                {/if}
              </span>
              {#if item.hint}
                <span class="row-hint">{item.hint}</span>
              {/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>

    <footer class="foot">
      <span class="foot-group">
        <span class="kbd">{modKey}</span><span class="kbd">K</span>
        <span class="foot-label">toggle</span>
      </span>
      <span class="foot-group">
        <span class="kbd">↑</span><span class="kbd">↓</span>
        <span class="foot-label">navigate</span>
      </span>
      <span class="foot-group">
        <span class="kbd">⏎</span>
        <span class="foot-label">run</span>
      </span>
    </footer>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(3px);
    z-index: 80;
  }
  .palette {
    position: fixed;
    top: 18%;
    left: 50%;
    transform: translateX(-50%);
    width: min(600px, calc(100vw - 32px));
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.55), 0 1px 0 rgba(255, 255, 255, 0.04) inset;
    z-index: 81;
    overflow: hidden;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--text-dim);
  }
  .search input {
    flex: 1;
    background: transparent;
    border: none;
    padding: 0;
    font-size: 14px;
    color: var(--text);
  }
  .search input:focus {
    outline: none;
    border: none;
  }
  .search input::placeholder {
    color: var(--text-faint);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 6px 0 8px;
  }
  .section-label {
    padding: 8px 14px 4px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 14px;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: 0;
    color: var(--text);
    transition: background 0.08s var(--ease-quick);
  }
  .row.active {
    background: var(--accent-soft);
  }
  .row.active .row-icon {
    color: var(--accent);
  }
  .row-icon {
    display: inline-flex;
    width: 22px;
    height: 22px;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
    background: var(--bg-3);
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }
  .row.active .row-icon {
    background: transparent;
  }
  .row-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .row-title {
    font-size: 13px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-sub {
    font-size: 11px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-hint {
    font-size: 10px;
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .empty {
    padding: 28px 16px;
    text-align: center;
    color: var(--text-dim);
    font-size: 12px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg);
    font-size: 11px;
    color: var(--text-dim);
  }
  .foot-group {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .foot-label {
    margin-left: 2px;
  }
  .kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    font-size: 10px;
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", monospace;
    color: var(--text-dim);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
</style>
