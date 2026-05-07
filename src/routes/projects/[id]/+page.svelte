<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { projectStore } from '$lib/projects.svelte';
  import StatusDot from '$lib/components/StatusDot.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';
  import { invoke } from '@tauri-apps/api/core';
  import ConfirmModal from '$lib/components/ConfirmModal.svelte';
  import ResourcePanel from '$lib/components/ResourcePanel.svelte';
  import ShellTerminal from '$lib/components/ShellTerminal.svelte';
  import type { HistoryEntry, AutoCommand, AutoCommandRunMode } from '$lib/types';
  import { AUTO_COMMAND_PRESETS, PRESET_GROUP_LABELS } from '$lib/autoCommandPresets';

  const id = $derived(page.params.id ?? '');
  const project = $derived(projectStore.byId(id));

  type Tab = 'overview' | 'logs' | 'shell' | 'env' | 'database' | 'autocmd' | 'runcmd' | 'history';
  let activeTab = $state<Tab>('overview');

  const tabs: { id: Tab; label: string; icon: string }[] = [
    { id: 'overview', label: 'Overview', icon: 'layers' },
    { id: 'logs', label: 'Logs', icon: 'logs' },
    { id: 'shell', label: 'Shell', icon: 'terminal' },
    { id: 'env', label: 'Environment', icon: 'settings' },
    { id: 'database', label: 'Database', icon: 'database' },
    { id: 'autocmd', label: 'Auto-commands', icon: 'terminal' },
    { id: 'runcmd', label: 'Run command', icon: 'play' },
    { id: 'history', label: 'History', icon: 'refresh' },
  ];

  const isRunning = $derived(project?.status === 'running');
  const isBusy = $derived(project?.status === 'starting' || project?.status === 'stopping');

  async function toggleStartStop() {
    if (!project || isBusy) return;
    if (isRunning) {
      await projectStore.stop(project.id);
    } else {
      await projectStore.start(project.id);
    }
  }

  let confirmDeleteOpen = $state(false);

  function askDelete() {
    confirmDeleteOpen = true;
  }

  async function performDelete() {
    if (!project) return;
    try {
      await projectStore.remove(project.id, true);
      confirmDeleteOpen = false;
      goto('/');
    } catch (e) {
      confirmDeleteOpen = false;
      // error already surfaced via projectStore.error banner
    }
  }

  async function openInBrowser() {
    if (!project) return;
    const local = projectStore.localUrlFor(project);
    const fallback = appPort ? `http://localhost:${appPort.host}` : null;
    const url = local ?? fallback;
    if (url) await openUrl(url);
  }

  async function revealInFinder() {
    if (!project) return;
    try {
      await revealItemInDir(project.path);
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  async function openTerminal() {
    if (!project) return;
    try {
      await invoke('open_in_terminal', { path: project.path });
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  const appPort = $derived(project?.ports.find((p) => p.service === 'app'));
  const mysqlPort = $derived(project?.ports.find((p) => p.service === 'mysql'));

  const editorLabels: Record<string, string> = {
    phpstorm: 'PhpStorm',
    vscode: 'VS Code',
    cursor: 'Cursor',
    zed: 'Zed',
  };
  const editorLabel = $derived(
    projectStore.settings?.editor ? editorLabels[projectStore.settings.editor] ?? 'Editor' : null,
  );

  async function openInEditor() {
    if (!project) return;
    if (!projectStore.settings?.editor) {
      projectStore.error = 'No editor configured. Pick one in Settings.';
      return;
    }
    try {
      await invoke('open_in_editor', { path: project.path });
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  let logsError = $state<string | null>(null);
  let logContainer: HTMLPreElement | null = $state(null);

  const logs = $derived(
    projectStore.liveLogs && project && projectStore.liveLogs.id === project.id
      ? projectStore.liveLogs.lines
      : [],
  );
  const logsActive = $derived(!!project && projectStore.liveLogs?.id === project.id);

  // Service filter for the Logs tab. `null` = all services.
  let composeServices = $state<string[]>([]);
  let logFilter = $state<string | null>(null);

  $effect(() => {
    if (!project) return;
    const pid = project.id;
    if (activeTab === 'logs') {
      logsError = null;
      // Refresh the service list each time the tab opens; the compose file
      // could have changed under us (user edited disk).
      projectStore.listComposeServices(pid).then((s) => (composeServices = s));
      const svc = logFilter;
      projectStore.startLogStream(pid, svc).catch((e) => (logsError = String(e)));
      return () => {
        projectStore.stopLogStream(pid);
      };
    }
  });

  async function changeLogFilter(next: string | null) {
    if (!project) return;
    logFilter = next;
    logsError = null;
    try {
      await projectStore.setLogFilter(project.id, next);
    } catch (e) {
      logsError = String(e);
    }
  }

  $effect(() => {
    void logs.length;
    if (!logContainer) return;
    const el = logContainer;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (distanceFromBottom < 80) {
      queueMicrotask(() => {
        el.scrollTop = el.scrollHeight;
      });
    }
  });

  function clearLogs() {
    if (!project) return;
    const pid = project.id;
    const svc = logFilter;
    projectStore.stopLogStream(pid).then(() => projectStore.startLogStream(pid, svc));
  }

  function dsn(): string | null {
    if (!project || !mysqlPort) return null;
    return `mysql://sail:password@127.0.0.1:${mysqlPort.host}/${project.composeProjectName}`;
  }

  let dsnCopied = $state(false);
  async function copyDsn() {
    const s = dsn();
    if (!s) return;
    try {
      await navigator.clipboard.writeText(s);
      dsnCopied = true;
      setTimeout(() => (dsnCopied = false), 1500);
    } catch (e) {
      projectStore.error = `Could not copy DSN: ${e}`;
    }
  }

  async function openInTablePlus() {
    const s = dsn();
    if (!s) return;
    try {
      await openUrl(s);
    } catch (e) {
      projectStore.error = `Could not open TablePlus: ${e}`;
    }
  }

  const mailpitPort = $derived(project?.ports.find((p) => p.service === 'mailpit_ui'));
  async function openMailpit() {
    if (!project || !mailpitPort) return;
    try {
      await openUrl(`http://localhost:${mailpitPort.host}`);
    } catch (e) {
      projectStore.error = `Could not open Mailpit: ${e}`;
    }
  }

  // ---- History tab state ----
  let history = $state<HistoryEntry[]>([]);
  let historyLoading = $state(false);

  async function loadHistory() {
    if (!project) return;
    historyLoading = true;
    try {
      history = await projectStore.listHistory(project.id, 100);
    } catch (e) {
      projectStore.error = String(e);
    } finally {
      historyLoading = false;
    }
  }

  // ---- Auto-commands tab state ----
  let autoCommands = $state<AutoCommand[]>([]);
  let autoCmdLoading = $state(false);
  let editingCmd = $state<AutoCommand | null>(null);
  let cmdLabel = $state('');
  let cmdCommand = $state('');
  let cmdRunMode = $state<AutoCommandRunMode>('service');
  let cmdEnabled = $state(true);

  async function loadAutoCommands() {
    if (!project) return;
    autoCmdLoading = true;
    try {
      autoCommands = await projectStore.listAutoCommands(project.id);
    } catch (e) {
      projectStore.error = String(e);
    } finally {
      autoCmdLoading = false;
    }
  }

  function startEditCmd(cmd: AutoCommand | null) {
    editingCmd = cmd;
    if (cmd) {
      cmdLabel = cmd.label;
      cmdCommand = cmd.command;
      cmdRunMode = cmd.runMode;
      cmdEnabled = cmd.enabled;
    } else {
      cmdLabel = '';
      cmdCommand = '';
      cmdRunMode = 'service';
      cmdEnabled = true;
    }
  }

  async function saveAutoCmd() {
    if (!project) return;
    if (!cmdLabel.trim() || !cmdCommand.trim()) return;
    try {
      await projectStore.upsertAutoCommand({
        id: editingCmd?.id,
        projectId: project.id,
        label: cmdLabel.trim(),
        command: cmdCommand.trim(),
        runMode: cmdRunMode,
        enabled: cmdEnabled,
        sortOrder: editingCmd?.sortOrder ?? autoCommands.length,
      });
      editingCmd = null;
      cmdLabel = '';
      cmdCommand = '';
      await loadAutoCommands();
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  async function toggleAutoCmd(cmd: AutoCommand) {
    try {
      await projectStore.upsertAutoCommand({
        id: cmd.id,
        projectId: cmd.projectId,
        label: cmd.label,
        command: cmd.command,
        runMode: cmd.runMode,
        enabled: !cmd.enabled,
        sortOrder: cmd.sortOrder,
      });
      await loadAutoCommands();
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  async function deleteAutoCmd(cmd: AutoCommand) {
    if (!confirm) return;
    try {
      await projectStore.deleteAutoCommand(cmd.id);
      if (editingCmd?.id === cmd.id) editingCmd = null;
      await loadAutoCommands();
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  async function runAutoNow() {
    if (!project) return;
    projectStore.resetAutoLogs(project.id);
    try {
      await projectStore.runAutoCommandsNow(project.id);
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  // Map of currently-installed commands by command-string so we can disable
  // preset chips that are already added.
  const installedCommands = $derived(new Set(autoCommands.map((c) => c.command)));

  const groupedPresets = $derived.by(() => {
    const order = ['workers', 'frontend', 'tooling', 'maintenance'] as const;
    return order
      .map((g) => ({
        group: g,
        items: AUTO_COMMAND_PRESETS.filter((p) => p.group === g),
      }))
      .filter((g) => g.items.length > 0);
  });

  async function addPreset(preset: (typeof AUTO_COMMAND_PRESETS)[number]) {
    if (!project) return;
    try {
      await projectStore.upsertAutoCommand({
        projectId: project.id,
        label: preset.label,
        command: preset.command,
        runMode: preset.runMode,
        enabled: true,
        sortOrder: autoCommands.length,
      });
      await loadAutoCommands();
    } catch (e) {
      projectStore.error = String(e);
    }
  }

  // Auto-cmd output panel: per-command tabs.
  const autoStreams = $derived(
    project && projectStore.autoLogs?.projectId === project.id
      ? projectStore.autoLogs.streams
      : [],
  );

  // Selected tab — null means "all" (interleaved). Otherwise commandId.
  let activeAutoTab = $state<string | null>(null);

  // Auto-select the most recently updated stream when new output arrives, but
  // only if the user hasn't pinned a different tab yet.
  let lastSeenStreamCount = $state(0);
  $effect(() => {
    if (autoStreams.length === 0) {
      activeAutoTab = null;
      lastSeenStreamCount = 0;
      return;
    }
    if (autoStreams.length > lastSeenStreamCount && activeAutoTab === null) {
      // First stream appearing — pin to it so user immediately sees output.
      activeAutoTab = autoStreams[0].commandId;
    }
    lastSeenStreamCount = autoStreams.length;
  });

  // Reset selected tab when project changes.
  $effect(() => {
    void project?.id;
    activeAutoTab = null;
    lastSeenStreamCount = 0;
  });

  const activeStream = $derived(
    activeAutoTab === null
      ? null
      : autoStreams.find((s) => s.commandId === activeAutoTab) ?? null,
  );

  // For the "All" view, interleave entries from all streams sorted by time,
  // each line prefixed with the command's label.
  const allInterleaved = $derived.by(() => {
    if (autoStreams.length === 0) return [];
    const merged = autoStreams.flatMap((s) =>
      s.entries.map((e) => ({ ...e, label: s.label })),
    );
    merged.sort((a, b) => a.at - b.at);
    return merged;
  });

  let autoLogContainer: HTMLPreElement | null = $state(null);
  // Auto-scroll on new output for the currently selected tab.
  $effect(() => {
    void (activeStream ? activeStream.updatedAt : allInterleaved.length);
    if (!autoLogContainer) return;
    const el = autoLogContainer;
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (distance < 80) {
      queueMicrotask(() => {
        el.scrollTop = el.scrollHeight;
      });
    }
  });

  function clearAutoLog() {
    if (!project) return;
    projectStore.resetAutoLogs(project.id);
    activeAutoTab = null;
  }

  // ---- Run command tab state ----
  const RUN_PRESETS: { label: string; command: string }[] = [
    { label: 'Migrate', command: 'php artisan migrate' },
    { label: 'Migrate fresh + seed', command: 'php artisan migrate:fresh --seed' },
    { label: 'composer install', command: 'composer install' },
    { label: 'composer update', command: 'composer update' },
    { label: 'npm install', command: 'npm install' },
    { label: 'npm run build', command: 'npm run build' },
    { label: 'Tinker', command: 'php artisan tinker' },
    { label: 'optimize:clear', command: 'php artisan optimize:clear' },
  ];

  let runCommand = $state('');
  let runCmdContainer: HTMLPreElement | null = $state(null);

  const runEntries = $derived(
    project && projectStore.oneShotLogs?.projectId === project.id
      ? projectStore.oneShotLogs.entries
      : [],
  );
  const runRunning = $derived(
    !!project &&
      projectStore.oneShotLogs?.projectId === project.id &&
      projectStore.oneShotLogs.running,
  );

  async function runCmdNow() {
    if (!project) return;
    const cmd = runCommand.trim();
    if (!cmd || runRunning) return;
    try {
      await projectStore.runOneShot(project.id, cmd);
    } catch {
      // already surfaced via projectStore.error
    }
  }

  async function stopRunCmd() {
    if (!project) return;
    await projectStore.stopOneShot(project.id);
  }

  function clearRunOutput() {
    if (!project) return;
    projectStore.clearOneShotLog(project.id);
  }

  function applyRunPreset(p: (typeof RUN_PRESETS)[number]) {
    runCommand = p.command;
  }

  function onRunInputKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      runCmdNow();
    }
  }

  // Auto-scroll the one-shot output panel.
  $effect(() => {
    void runEntries.length;
    if (!runCmdContainer) return;
    const el = runCmdContainer;
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (distance < 80) {
      queueMicrotask(() => {
        el.scrollTop = el.scrollHeight;
      });
    }
  });

  $effect(() => {
    if (activeTab === 'history' && project) {
      loadHistory();
    }
  });
  $effect(() => {
    if (activeTab === 'autocmd' && project) {
      loadAutoCommands();
    }
  });

  function formatTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleString();
  }

  const envContent = $derived(
    project
      ? `APP_NAME=${project.name}
APP_PORT=${project.ports.find((p) => p.service === 'app')?.host ?? 80}
APP_URL=http://localhost:${project.ports.find((p) => p.service === 'app')?.host ?? 80}

COMPOSE_PROJECT_NAME=${project.composeProjectName}
WWWGROUP=20
WWWUSER=501

VITE_PORT=${project.ports.find((p) => p.service === 'vite')?.host ?? 5173}
${project.ports.find((p) => p.service === 'mysql') ? `FORWARD_DB_PORT=${project.ports.find((p) => p.service === 'mysql')!.host}\nDB_HOST=mysql\nDB_DATABASE=${project.composeProjectName}` : ''}
${project.ports.find((p) => p.service === 'redis') ? `FORWARD_REDIS_PORT=${project.ports.find((p) => p.service === 'redis')!.host}` : ''}
${project.ports.find((p) => p.service === 'mailpit_ui') ? `FORWARD_MAILPIT_DASHBOARD_PORT=${project.ports.find((p) => p.service === 'mailpit_ui')!.host}` : ''}
`.trim()
      : '',
  );
</script>

{#if !project}
  {#if projectStore.loading}
    <div class="not-found">
      <p>Loading…</p>
    </div>
  {:else}
    <div class="not-found">
      <p>Project not found.</p>
      <button class="btn" onclick={() => goto('/')}>Back to projects</button>
    </div>
  {/if}
{:else}
  <div class="page-top">
  <header class="detail-header" data-tauri-drag-region>
    <button class="back btn btn-ghost" onclick={() => goto('/')}>
      <Icon name="chevron" size={13} class="flip" />
      Projects
    </button>

    <div class="title-block" data-tauri-drag-region>
      <h1>{project.name}</h1>
      <StatusDot status={project.status} />
    </div>

    <div class="header-actions">
      <button
        class="btn"
        class:btn-primary={!isRunning && !isBusy}
        onclick={toggleStartStop}
        disabled={isBusy}
      >
        {#if isBusy}
          <span class="spinner"></span>
          {project.status === 'starting' ? 'Starting' : 'Stopping'}
        {:else if isRunning}
          <Icon name="stop" size={12} />
          Stop
        {:else}
          <Icon name="play" size={12} />
          Start
        {/if}
      </button>

      <button class="btn btn-ghost" onclick={openInBrowser} disabled={!isRunning}>
        <Icon name="external" size={13} />
        Open
      </button>

      {#if mailpitPort}
        <button class="btn btn-ghost" onclick={openMailpit} disabled={!isRunning} title="Open Mailpit">
          <Icon name="logs" size={13} />
          Mailpit
        </button>
      {/if}

      <button class="btn btn-ghost" onclick={openTerminal}>
        <Icon name="terminal" size={13} />
        Terminal
      </button>

      <button class="btn btn-ghost" onclick={revealInFinder}>
        <Icon name="folder" size={13} />
        Reveal
      </button>

      <button
        class="btn btn-ghost"
        onclick={openInEditor}
        title={projectStore.settings?.editor
          ? `Open in ${editorLabel}`
          : 'Pick an editor in Settings first'}
      >
        <Icon name="layers" size={13} />
        {editorLabel ?? 'Editor'}
      </button>

      <button class="btn btn-danger" onclick={askDelete} title="Delete project">
        <Icon name="trash" size={13} />
      </button>
    </div>
  </header>

  <nav class="tabs">
    {#each tabs as tab (tab.id)}
      <button class="tab" class:active={activeTab === tab.id} onclick={() => (activeTab = tab.id)}>
        <Icon name={tab.icon} size={13} />
        {tab.label}
      </button>
    {/each}
  </nav>
  </div>

  <section class="tab-body">
    {#if activeTab === 'overview'}
      <div class="grid-cols">
        <div class="panel">
          <h3>Ports</h3>
          <table class="ports-table">
            <thead>
              <tr>
                <th>Service</th>
                <th>Container</th>
                <th>Host</th>
              </tr>
            </thead>
            <tbody>
              {#each project.ports as port (port.service)}
                <tr>
                  <td>{port.label}</td>
                  <td class="mono">
                    {port.service === 'app'
                      ? '80'
                      : port.service === 'mysql'
                        ? '3306'
                        : port.service === 'redis'
                          ? '6379'
                          : port.service === 'vite'
                            ? '5173'
                            : port.service === 'mailpit_ui'
                              ? '8025'
                              : '—'}
                  </td>
                  <td class="mono accent">:{port.host}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <div class="panel">
          <h3>Project</h3>
          <dl class="kv">
            <dt>Path</dt>
            <dd class="mono selectable">{project.path}</dd>
            <dt>Compose name</dt>
            <dd class="mono selectable">{project.composeProjectName}</dd>
            <dt>PHP version</dt>
            <dd>{project.phpVersion}</dd>
            <dt>Starter kit</dt>
            <dd>{project.starterKit === 'none' ? 'plain Laravel' : project.starterKit}</dd>
            <dt>Created</dt>
            <dd>{new Date(project.createdAt).toLocaleDateString()}</dd>
            {#if project.lastStarted}
              <dt>Last started</dt>
              <dd>{new Date(project.lastStarted).toLocaleString()}</dd>
            {/if}
          </dl>
        </div>

        <div class="panel">
          <h3>Services</h3>
          <ul class="service-list">
            {#each project.services as service (service)}
              <li>
                <span class="bullet"></span>
                {service}
              </li>
            {/each}
          </ul>
        </div>
      </div>
      <ResourcePanel
        projectId={project.id}
        active={activeTab === 'overview'}
        running={isRunning}
      />
    {:else if activeTab === 'logs'}
      <div class="panel logs">
        <div class="logs-head">
          <h3>
            Container logs
            {#if logsActive}
              <span class="live-dot" title="Streaming live"></span>
            {/if}
          </h3>
          <div class="logs-actions">
            <label class="log-filter" title="Filter logs by service">
              <span>Service</span>
              <select
                value={logFilter ?? ''}
                onchange={(e) => changeLogFilter((e.currentTarget as HTMLSelectElement).value || null)}
              >
                <option value="">All services</option>
                {#each composeServices as svc (svc)}
                  <option value={svc}>{svc}</option>
                {/each}
              </select>
            </label>
            <button class="btn btn-ghost btn-icon" title="Clear & reattach" onclick={clearLogs}>
              <Icon name="refresh" size={13} />
            </button>
          </div>
        </div>
        {#if logsError}
          <div class="log-output error">{logsError}</div>
        {:else if logs.length === 0}
          <div class="log-output dim">Waiting for log output…</div>
        {:else}
          <pre class="log-output selectable" bind:this={logContainer}>{logs.join('\n')}</pre>
        {/if}
      </div>
    {:else if activeTab === 'shell'}
      <div class="panel">
        <div class="logs-head">
          <h3>Shell</h3>
          <span class="hint">
            Interactive <code>docker compose exec -it laravel.test bash</code>. Type as you would in Terminal.
          </span>
        </div>
        <ShellTerminal projectId={project.id} active={activeTab === 'shell'} />
      </div>
    {:else if activeTab === 'env'}
      <div class="panel">
        <div class="logs-head">
          <h3>.env</h3>
          <span class="hint">Read-only preview · edit on disk and restart to apply</span>
        </div>
        <pre class="env-output selectable">{envContent}</pre>
      </div>
    {:else if activeTab === 'autocmd'}
      <div class="panel">
        <div class="logs-head">
          <h3>Auto-commands</h3>
          <div class="logs-actions">
            <button
              class="btn btn-ghost"
              onclick={runAutoNow}
              disabled={!isRunning || autoCommands.filter((c) => c.enabled).length === 0}
              title="Run all enabled auto-commands now"
            >
              <Icon name="play" size={12} />
              Run now
            </button>
          </div>
        </div>
        <p class="hint">
          Commands run inside the <code>laravel.test</code> container after every successful start.
          You can write them as you would in your terminal — <code>sail artisan horizon</code>,
          <code>sail npm run dev</code>, etc. The <code>sail</code> prefix is stripped automatically.
          <strong>service</strong> runs in the background and keeps running until containers stop;
          <strong>once</strong> blocks until the command exits.
        </p>

        <div class="presets">
          <div class="presets-head">
            <h4>Quick add</h4>
            <span class="presets-sub">Common Laravel + Sail workers and helpers</span>
          </div>
          {#each groupedPresets as g (g.group)}
            <div class="preset-group">
              <div class="preset-group-label">{PRESET_GROUP_LABELS[g.group]}</div>
              <div class="preset-list">
                {#each g.items as p (p.command)}
                  {@const added = installedCommands.has(p.command)}
                  <button
                    type="button"
                    class="preset-chip"
                    class:added
                    onclick={() => addPreset(p)}
                    disabled={added}
                    title={added ? 'Already added' : p.description}
                  >
                    <span class="preset-mode preset-mode-{p.runMode}">{p.runMode}</span>
                    <span class="preset-label">{p.label}</span>
                    {#if added}
                      <span class="preset-added">added</span>
                    {:else}
                      <Icon name="plus" size={11} />
                    {/if}
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        </div>

        {#if autoCmdLoading}
          <p class="hint">Loading…</p>
        {/if}

        {#each autoCommands as cmd (cmd.id)}
          <div class="autocmd-row" class:disabled={!cmd.enabled}>
            <div class="autocmd-text">
              <div class="autocmd-label">{cmd.label}</div>
              <code class="autocmd-cmd selectable">{cmd.command}</code>
              <span class="autocmd-mode">{cmd.runMode}</span>
            </div>
            <div class="autocmd-actions">
              <button class="btn btn-ghost" onclick={() => toggleAutoCmd(cmd)}>
                {cmd.enabled ? 'Disable' : 'Enable'}
              </button>
              <button class="btn btn-ghost" onclick={() => startEditCmd(cmd)}>Edit</button>
              <button class="btn btn-ghost btn-danger" onclick={() => deleteAutoCmd(cmd)}>
                <Icon name="trash" size={12} />
              </button>
            </div>
          </div>
        {/each}

        <div class="autocmd-form">
          <h4>{editingCmd ? 'Edit command' : 'Add command'}</h4>
          <div class="form-row">
            <input
              type="text"
              placeholder="Label, e.g. Horizon"
              bind:value={cmdLabel}
              class="form-label"
            />
            <select bind:value={cmdRunMode}>
              <option value="service">service (detached)</option>
              <option value="once">once (blocking)</option>
            </select>
          </div>
          <input
            type="text"
            placeholder="Command, e.g. sail artisan horizon"
            bind:value={cmdCommand}
          />
          <p class="hint">
            Runs as <code>bash -lc "&lt;command&gt;"</code> inside the laravel.test container.
            A leading <code>sail</code> is stripped (and <code>sail artisan&hellip;</code> →
            <code>php artisan&hellip;</code>).
          </p>
          <div class="form-actions">
            <label class="enabled-toggle">
              <input type="checkbox" bind:checked={cmdEnabled} />
              Enabled
            </label>
            {#if editingCmd}
              <button class="btn btn-ghost" onclick={() => startEditCmd(null)}>Cancel</button>
            {/if}
            <button
              class="btn btn-primary"
              onclick={saveAutoCmd}
              disabled={!cmdLabel.trim() || !cmdCommand.trim()}
            >
              {editingCmd ? 'Save changes' : 'Add command'}
            </button>
          </div>
        </div>
      </div>

      <div class="panel auto-output-panel">
        <div class="logs-head">
          <h3>Output</h3>
          <div class="logs-actions">
            <button
              class="btn btn-ghost btn-icon"
              onclick={clearAutoLog}
              disabled={autoStreams.length === 0}
              title="Clear output"
            >
              <Icon name="trash" size={12} />
            </button>
          </div>
        </div>

        {#if autoStreams.length === 0}
          <div class="log-output dim">
            No recent output. Click <strong>Run now</strong> or start the project to see output here.
          </div>
        {:else}
          <div class="auto-tabs" role="tablist">
            <button
              type="button"
              role="tab"
              class="auto-tab"
              class:active={activeAutoTab === null}
              onclick={() => (activeAutoTab = null)}
            >
              All
              <span class="tab-count">{allInterleaved.length}</span>
            </button>
            {#each autoStreams as s (s.commandId)}
              <button
                type="button"
                role="tab"
                class="auto-tab"
                class:active={activeAutoTab === s.commandId}
                onclick={() => (activeAutoTab = s.commandId)}
                title={s.label}
              >
                <span class="tab-dot"></span>
                {s.label}
                <span class="tab-count">{s.entries.length}</span>
              </button>
            {/each}
          </div>

          {#if activeStream}
            <pre class="log-output selectable" bind:this={autoLogContainer}>{#each activeStream.entries as e, i (i)}<span class="auto-line auto-line-{e.stream}">{e.line}
</span>{/each}</pre>
          {:else}
            <pre class="log-output selectable" bind:this={autoLogContainer}>{#each allInterleaved as e, i (i)}<span class="auto-line auto-line-{e.stream}"><span class="auto-line-label">[{e.label}]</span> {e.line}
</span>{/each}</pre>
          {/if}
        {/if}
      </div>
    {:else if activeTab === 'runcmd'}
      <div class="panel">
        <div class="logs-head">
          <h3>Run command</h3>
          <span class="hint">
            One-shot. Runs as <code>bash -lc "&lt;cmd&gt;"</code> inside <code>laravel.test</code>.
          </span>
        </div>

        <div class="run-presets">
          {#each RUN_PRESETS as p (p.command)}
            <button
              type="button"
              class="preset-chip"
              onclick={() => applyRunPreset(p)}
              title="Use this command"
            >
              <span class="preset-label">{p.label}</span>
            </button>
          {/each}
        </div>

        <div class="run-input-row">
          <input
            type="text"
            class="run-input"
            placeholder="e.g. php artisan migrate"
            bind:value={runCommand}
            onkeydown={onRunInputKey}
            disabled={runRunning}
          />
          {#if runRunning}
            <button class="btn btn-danger" onclick={stopRunCmd} title="Kill the running command">
              <Icon name="stop" size={12} />
              Stop
            </button>
          {:else}
            <button
              class="btn btn-primary"
              onclick={runCmdNow}
              disabled={!runCommand.trim() || !isRunning}
              title={isRunning ? 'Run now' : 'Project must be running'}
            >
              <Icon name="play" size={12} />
              Run
            </button>
          {/if}
        </div>
        {#if !isRunning}
          <p class="hint">Project containers are not running. Start the project first.</p>
        {/if}
      </div>

      <div class="panel auto-output-panel">
        <div class="logs-head">
          <h3>
            Output
            {#if runRunning}
              <span class="live-dot" title="Command running"></span>
            {/if}
          </h3>
          <div class="logs-actions">
            <button
              class="btn btn-ghost btn-icon"
              onclick={clearRunOutput}
              disabled={runEntries.length === 0}
              title="Clear output"
            >
              <Icon name="trash" size={12} />
            </button>
          </div>
        </div>

        {#if runEntries.length === 0}
          <div class="log-output dim">No output yet. Type a command and press Run (or Enter).</div>
        {:else}
          <pre class="log-output selectable" bind:this={runCmdContainer}>{#each runEntries as e, i (i)}<span class="auto-line auto-line-{e.stream}">{e.line}
</span>{/each}</pre>
        {/if}
      </div>
    {:else if activeTab === 'history'}
      <div class="panel">
        <div class="logs-head">
          <h3>History</h3>
          <div class="logs-actions">
            <button class="btn btn-ghost btn-icon" onclick={loadHistory} title="Refresh">
              <Icon name="refresh" size={13} />
            </button>
          </div>
        </div>
        {#if historyLoading && history.length === 0}
          <p class="hint">Loading…</p>
        {:else if history.length === 0}
          <p class="hint">No events recorded yet.</p>
        {:else}
          <ul class="history-list">
            {#each history as entry (entry.id)}
              <li>
                <span class="history-kind history-kind-{entry.kind}">{entry.kind}</span>
                <span class="history-when">{formatTime(entry.at)}</span>
                {#if entry.detail}
                  <span class="history-detail">{entry.detail}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {:else if activeTab === 'database'}
      <div class="panel">
        <h3>MySQL connection</h3>
        {#if mysqlPort}
          <dl class="kv">
            <dt>Host</dt>
            <dd class="mono selectable">127.0.0.1</dd>
            <dt>Port</dt>
            <dd class="mono selectable">{mysqlPort.host}</dd>
            <dt>Database</dt>
            <dd class="mono selectable">{project.composeProjectName}</dd>
            <dt>Username</dt>
            <dd class="mono selectable">sail</dd>
            <dt>Password</dt>
            <dd class="mono selectable">password</dd>
          </dl>
          <div class="db-actions">
            <button class="btn" onclick={openInTablePlus}>
              <Icon name="external" size={12} />
              Open in TablePlus
            </button>
            <button class="btn btn-ghost" onclick={copyDsn}>
              {dsnCopied ? 'Copied!' : 'Copy DSN'}
            </button>
          </div>
        {:else}
          <p class="hint">No database service enabled for this project.</p>
        {/if}
      </div>
    {/if}
  </section>

  <ConfirmModal
    open={confirmDeleteOpen}
    title="Delete {project.name}?"
    message="This will stop the project's containers and permanently remove its folder."
    detail={project.path}
    confirmLabel="Delete project"
    danger
    onConfirm={performDelete}
    onCancel={() => (confirmDeleteOpen = false)}
  />
{/if}

<style>
  /* Pinned wrapper that keeps the header AND the tab row visible while the
     tab content scrolls. Without this, switching to a long tab (Auto-commands,
     Logs) scrolled both out of view and looked like the navigation had
     disappeared. */
  .page-top {
    position: sticky;
    top: 0;
    z-index: 5;
    background: var(--bg);
  }

  .detail-header {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 26px 14px;
    border-bottom: 1px solid var(--border);
    background: linear-gradient(180deg, var(--bg-1) 0%, var(--bg) 100%);
    backdrop-filter: blur(8px) saturate(140%);
    -webkit-backdrop-filter: blur(8px) saturate(140%);
  }
  :global(.flip) {
    transform: rotate(180deg);
  }
  .title-block {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: 19px;
    font-weight: 650;
    letter-spacing: -0.025em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .header-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .tabs {
    display: flex;
    gap: 2px;
    padding: 0 26px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 12px 14px;
    color: var(--text-dim);
    font-size: 12.5px;
    font-weight: 500;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color 0.15s var(--ease-quick), border-color 0.15s var(--ease-quick);
    white-space: nowrap;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .tab-body {
    padding: 20px 24px;
  }

  .grid-cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 14px;
  }

  .panel {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px 16px;
  }
  .panel h3 {
    margin: 0 0 10px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }

  .ports-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .ports-table th {
    text-align: left;
    color: var(--text-faint);
    font-weight: 500;
    font-size: 11px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
  }
  .ports-table td {
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .ports-table tr:last-child td {
    border-bottom: none;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-variant-numeric: tabular-nums;
  }
  .accent {
    color: var(--accent);
  }

  .kv {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 14px;
    margin: 0;
    font-size: 12px;
  }
  .kv dt {
    color: var(--text-faint);
    font-weight: 500;
  }
  .kv dd {
    margin: 0;
    color: var(--text);
    word-break: break-all;
  }

  .service-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }
  .service-list li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .bullet {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }

  .logs-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }
  .logs-head h3 {
    margin: 0;
  }
  .hint {
    font-size: 11px;
    color: var(--text-faint);
  }

  .log-output,
  .env-output {
    background: var(--code-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 12px;
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12px;
    line-height: 1.5;
    color: var(--code-text);
    white-space: pre;
    overflow-x: auto;
    max-height: 480px;
    overflow-y: auto;
  }

  .db-actions {
    display: flex;
    gap: 6px;
    margin-top: 14px;
  }

  .not-found {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 14px;
    color: var(--text-dim);
  }

  .spinner {
    width: 11px;
    height: 11px;
    border: 1.5px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .presets {
    margin-bottom: 14px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
  }
  .presets-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .presets-head h4 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .presets-sub {
    font-size: 11px;
    color: var(--text-faint);
  }
  .preset-group {
    margin-top: 8px;
  }
  .preset-group-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    margin-bottom: 5px;
  }
  .preset-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .preset-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 9px 5px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--bg-2);
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.15s var(--ease-quick), border-color 0.15s var(--ease-quick);
  }
  .preset-chip:hover:not(:disabled) {
    background: var(--bg-3);
    border-color: var(--accent);
  }
  .preset-chip.added {
    opacity: 0.55;
    cursor: default;
    background: var(--bg);
  }
  .preset-mode {
    font-size: 9.5px;
    text-transform: uppercase;
    font-weight: 700;
    letter-spacing: 0.05em;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .preset-mode-service {
    color: var(--success);
    background: var(--success-soft);
  }
  .preset-mode-once {
    color: var(--warning);
    background: var(--warning-soft);
  }
  .preset-label {
    font-weight: 500;
  }
  .preset-added {
    font-size: 10px;
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .auto-output-panel {
    margin-top: 14px;
  }
  .auto-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 8px;
    padding: 4px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .auto-tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-dim);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s var(--ease-quick), color 0.15s var(--ease-quick);
  }
  .auto-tab:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .auto-tab.active {
    background: var(--bg-2);
    color: var(--text);
    border-color: var(--border-strong);
  }
  .tab-count {
    font-size: 10.5px;
    padding: 1px 6px;
    border-radius: 8px;
    background: var(--bg-3);
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .auto-tab.active .tab-count {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .tab-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--success);
  }
  .auto-line {
    display: block;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .auto-line-stderr {
    color: #ffb4a8;
  }
  .auto-line-label {
    color: var(--accent);
    font-weight: 600;
    margin-right: 6px;
  }

  .autocmd-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    margin-bottom: 6px;
    background: var(--bg);
  }
  .autocmd-row.disabled {
    opacity: 0.55;
  }
  .autocmd-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .autocmd-label {
    font-size: 13px;
    font-weight: 500;
  }
  .autocmd-cmd {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 11.5px;
    color: var(--text-dim);
    background: var(--bg-2);
    padding: 2px 6px;
    border-radius: 3px;
    width: fit-content;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .autocmd-mode {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-faint);
  }
  .autocmd-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .autocmd-form {
    margin-top: 14px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .autocmd-form h4 {
    margin: 0 0 4px;
    font-size: 12px;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .form-row {
    display: grid;
    grid-template-columns: 2fr 1fr;
    gap: 8px;
  }
  .form-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
  .enabled-toggle {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .history-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .history-list li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    border: 1px solid var(--border);
    font-size: 12px;
  }
  .history-kind {
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10.5px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 3px;
    background: var(--bg-3);
    color: var(--text-dim);
  }
  .history-kind-started { color: var(--success); }
  .history-kind-stopped { color: var(--text-dim); }
  .history-kind-errored { color: var(--error); background: var(--error-soft); }
  .history-kind-created { color: var(--accent); }
  .history-kind-cloned { color: var(--accent); }
  .history-kind-imported { color: var(--accent); }
  .history-when {
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
  }
  .history-detail {
    color: var(--text-dim);
    font-size: 11.5px;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .live-dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    margin-left: 6px;
    border-radius: 50%;
    background: var(--success);
    box-shadow: 0 0 0 0 var(--success-soft);
    animation: live-pulse 1.6s ease-out infinite;
    vertical-align: middle;
  }
  @keyframes live-pulse {
    0% { box-shadow: 0 0 0 0 rgba(16, 185, 129, 0.6); }
    70% { box-shadow: 0 0 0 6px rgba(16, 185, 129, 0); }
    100% { box-shadow: 0 0 0 0 rgba(16, 185, 129, 0); }
  }
  .log-output.dim {
    color: var(--text-faint);
    font-style: italic;
  }
  .log-output.error {
    color: var(--error);
  }

  .log-filter {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-dim);
  }
  .log-filter span {
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10.5px;
    color: var(--text-faint);
  }
  .log-filter select {
    font-size: 12px;
    padding: 3px 6px;
  }

  .run-presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }
  .run-input-row {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .run-input {
    flex: 1;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12.5px;
  }
</style>
