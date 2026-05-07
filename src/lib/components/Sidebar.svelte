<script lang="ts">
  import { page } from '$app/state';
  import Icon from './Icon.svelte';
  import { projectStore } from '$lib/projects.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  const navItems = [
    { href: '/', label: 'Projects', icon: 'home' },
    { href: '/templates', label: 'Templates', icon: 'box' },
    { href: '/settings', label: 'Settings', icon: 'settings' },
  ];

  function isActive(href: string) {
    if (href === '/') return page.url.pathname === '/' || page.url.pathname.startsWith('/projects');
    return page.url.pathname.startsWith(href);
  }

  const REPO = 'https://github.com/LuukDahlmans/Laravel-Sail-Manager';
  const WEBSITE = 'https://sailmanager.app';
  const DOCS = 'https://sailmanager.app/docs';
  function openExternal(url: string) {
    return (e: MouseEvent) => {
      e.preventDefault();
      openUrl(url).catch(() => {});
    };
  }

  const sys = $derived(projectStore.dockerSystem);
  function fmtBytes(n: number): string {
    if (n <= 0) return '0';
    const u = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < u.length - 1) {
      v /= 1024;
      i++;
    }
    return `${v.toFixed(v < 10 && i > 0 ? 1 : 0)} ${u[i]}`;
  }
  const cpuPct = $derived(sys ? Math.min(100, sys.totalCpuPercent) : 0);
  const memPct = $derived(
    sys && sys.memTotalBytes > 0
      ? Math.min(100, (sys.memUsedBytes / sys.memTotalBytes) * 100)
      : 0,
  );
  const diskTotal = $derived(
    sys
      ? sys.diskImagesBytes +
          sys.diskContainersBytes +
          sys.diskVolumesBytes +
          sys.diskCacheBytes
      : 0,
  );
</script>

<aside class="sidebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <div class="logo">
      <Icon name="waves" size={18} />
    </div>
    <div class="brand-text">
      <div class="brand-name">Sail Manager</div>
      <div class="brand-sub">{projectStore.runningCount} running</div>
    </div>
  </div>

  <nav>
    {#each navItems as item (item.href)}
      <a class="nav-item" class:active={isActive(item.href)} href={item.href}>
        <Icon name={item.icon} size={15} />
        <span>{item.label}</span>
      </a>
    {/each}
  </nav>

  <div class="spacer" data-tauri-drag-region></div>

  <div class="footer">
    {#if projectStore.envCheck?.dockerOk}
      <div class="docker-status ok">
        <span class="dot ok"></span>
        <span class="text">Docker running</span>
      </div>

      {#if sys}
        <div class="sys-stats">
          <div class="sys-row">
            <span class="sys-label">CPU</span>
            <div class="sys-track">
              <div
                class="sys-fill"
                class:warn={cpuPct > 70}
                class:hot={cpuPct > 95}
                style="width: {cpuPct}%"
              ></div>
            </div>
            <span class="sys-val">{sys.totalCpuPercent.toFixed(0)}%</span>
          </div>
          <div class="sys-row">
            <span class="sys-label">RAM</span>
            <div class="sys-track">
              <div
                class="sys-fill"
                class:warn={memPct > 70}
                class:hot={memPct > 90}
                style="width: {memPct}%"
              ></div>
            </div>
            <span class="sys-val">{fmtBytes(sys.memUsedBytes)}</span>
          </div>
          <div class="sys-row">
            <span class="sys-label">Disk</span>
            <span class="sys-spacer"></span>
            <span class="sys-val">{fmtBytes(diskTotal)}</span>
          </div>
        </div>
      {/if}
    {:else}
      <div class="docker-status bad">
        <span class="dot bad"></span>
        <span class="text">
          {projectStore.startingDocker ? 'Starting Docker…' : 'Docker not running'}
        </span>
      </div>
      <button
        class="btn docker-start"
        onclick={() => projectStore.startDockerDesktop()}
        disabled={projectStore.startingDocker}
      >
        {#if projectStore.startingDocker}
          <span class="mini-spinner"></span>
          Launching…
        {:else}
          <Icon name="play" size={11} />
          Start Docker
        {/if}
      </button>
    {/if}

    <div class="meta-footer">
      <div class="meta-links">
        <a class="repo-link" href={WEBSITE} onclick={openExternal(WEBSITE)}>
          <Icon name="external" size={10} />
          Website
        </a>
        <a class="repo-link" href={DOCS} onclick={openExternal(DOCS)}>
          <Icon name="external" size={10} />
          Docs
        </a>
        <a class="repo-link" href={REPO} onclick={openExternal(REPO)}>
          <Icon name="external" size={10} />
          GitHub
        </a>
      </div>
      <p class="disclaimer">
        Community project · Not affiliated with the Laravel project. Laravel and Laravel Sail are
        trademarks of Taylor Otwell.
      </p>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    width: 230px;
    flex-shrink: 0;
    background: var(--sidebar-bg);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 40px 14px 14px;
    gap: 14px;
    position: relative;
  }
  /* Soft glow at the top of the sidebar to add depth and play with the
     accent identity without becoming loud. */
  .sidebar::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: var(--grid-fade), radial-gradient(60% 40% at 12% 8%, var(--accent-soft) 0%, transparent 60%);
    opacity: 0.55;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 4px 4px 10px;
    position: relative;
    z-index: 1;
    border-bottom: 1px solid var(--border);
    margin-bottom: 4px;
  }
  .logo {
    width: 34px;
    height: 34px;
    background: linear-gradient(135deg, var(--accent) 0%, var(--accent-hover) 100%);
    color: white;
    border-radius: 9px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.18) inset,
      0 0 0 1px rgba(255, 255, 255, 0.06),
      0 4px 14px var(--accent-glow);
  }
  .brand-text {
    display: flex;
    flex-direction: column;
    line-height: 1.15;
  }
  .brand-name {
    font-size: 13.5px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .brand-sub {
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 1px;
  }


  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 8px 11px;
    border-radius: 8px;
    color: var(--text-dim);
    font-size: 13px;
    font-weight: 500;
    transition: background 0.15s var(--ease-quick), color 0.15s var(--ease-quick),
      transform 0.08s var(--ease-quick);
    position: relative;
    z-index: 1;
  }
  .nav-item:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--accent-soft);
    color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent-soft) inset;
  }
  .nav-item.active::before {
    content: "";
    position: absolute;
    left: -14px;
    top: 8px;
    bottom: 8px;
    width: 3px;
    background: var(--accent);
    border-radius: 0 3px 3px 0;
    box-shadow: 0 0 8px var(--accent-glow);
  }

  .spacer {
    flex: 1;
  }

  .footer {
    padding: 10px 6px 4px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .docker-status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .docker-status.ok {
    color: var(--success);
  }
  .docker-status.bad {
    color: var(--error);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-faint);
    flex-shrink: 0;
  }
  .dot.ok {
    background: var(--success);
    box-shadow: 0 0 0 3px var(--success-soft);
  }
  .dot.bad {
    background: var(--error);
    box-shadow: 0 0 0 3px var(--error-soft);
    animation: blink 1.4s ease-in-out infinite;
  }
  @keyframes blink {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
  .sys-stats {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 8px 4px 4px;
  }
  .sys-row {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    gap: 8px;
  }
  .sys-label {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    text-transform: uppercase;
  }
  .sys-track {
    height: 4px;
    background: var(--bg-3);
    border-radius: 999px;
    overflow: hidden;
  }
  .sys-spacer {
    height: 4px;
  }
  .sys-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--success) 0%, color-mix(in oklab, var(--success), white 18%) 100%);
    box-shadow: 0 0 6px var(--success-glow);
    border-radius: 999px;
    transition: width 0.4s var(--ease);
  }
  .sys-fill.warn {
    background: linear-gradient(90deg, var(--warning) 0%, color-mix(in oklab, var(--warning), white 18%) 100%);
    box-shadow: none;
  }
  .sys-fill.hot {
    background: linear-gradient(90deg, var(--error) 0%, color-mix(in oklab, var(--error), white 18%) 100%);
    box-shadow: none;
  }
  .sys-val {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    text-align: right;
    min-width: 44px;
  }

  .meta-footer {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .meta-links {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .repo-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-dim);
    text-decoration: none;
    transition: color 0.15s var(--ease-quick);
    width: fit-content;
  }
  .repo-link:hover {
    color: var(--accent);
  }
  .disclaimer {
    margin: 0;
    font-size: 10px;
    line-height: 1.45;
    color: var(--text-faint);
  }

  .docker-start {
    width: 100%;
    justify-content: center;
    font-size: 11px;
    padding: 5px 8px;
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  .docker-start:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .mini-spinner {
    width: 9px;
    height: 9px;
    border: 1.5px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
