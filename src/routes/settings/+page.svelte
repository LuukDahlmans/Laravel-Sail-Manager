<script lang="ts">
  import { projectStore } from '$lib/projects.svelte';
  import { goto } from '$app/navigation';
  import Icon from '$lib/components/Icon.svelte';
  import ConfirmModal from '$lib/components/ConfirmModal.svelte';
  import { toast } from '$lib/toast.svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type { EditorChoice, ThemeChoice } from '$lib/types';

  let projectsRoot = $state(projectStore.envCheck?.projectsRoot ?? '');
  let editor = $state<EditorChoice>(projectStore.settings?.editor ?? '');
  let editorSaving = $state(false);
  let changingRoot = $state(false);

  // Change where NEW projects are scaffolded. Existing projects stay where
  // they are — this only affects future create/clone destinations.
  async function changeProjectsRoot() {
    if (changingRoot) return;
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: 'Choose where to keep new Laravel projects',
        defaultPath: projectsRoot || undefined,
      });
      if (typeof selected !== 'string' || selected.length === 0) return;
      changingRoot = true;
      await projectStore.setProjectsRoot(selected);
      projectsRoot = projectStore.envCheck?.projectsRoot ?? selected;
      toast.success('Projects root updated');
    } catch (e) {
      projectStore.reportError(String(e), 'Could not change projects root');
    } finally {
      changingRoot = false;
    }
  }

  async function checkForUpdates() {
    const result = await projectStore.checkForUpdate();
    if (result === 'available') {
      toast.success(
        `Version ${projectStore.updateAvailable?.version} is available — install it from the sidebar.`,
      );
    } else if (result === 'up-to-date') {
      toast.info('You are running the latest version.');
    } else {
      projectStore.reportError(
        'Could not reach the update server. Check your connection and try again.',
        'Update check failed',
      );
    }
  }

  $effect(() => {
    if (projectStore.envCheck?.projectsRoot) {
      projectsRoot = projectStore.envCheck.projectsRoot;
    }
  });

  $effect(() => {
    if (projectStore.settings?.editor != null && editor === '') {
      editor = projectStore.settings.editor;
    }
  });

  async function saveEditor(e: Event) {
    const value = (e.target as HTMLSelectElement).value as EditorChoice;
    editor = value;
    editorSaving = true;
    try {
      await projectStore.setEditor(value);
    } finally {
      editorSaving = false;
    }
  }

  const localUrlsOn = $derived(projectStore.settings?.localUrlsEnabled ?? false);
  const httpsOn = $derived(projectStore.settings?.localUrlsHttps ?? false);
  const tld = $derived(projectStore.settings?.localUrlTld ?? 'sail');
  let httpsError = $state<string | null>(null);

  let tldDraft = $state('');
  let tldError = $state<string | null>(null);

  $effect(() => {
    if (projectStore.settings?.localUrlTld && tldDraft === '') {
      tldDraft = projectStore.settings.localUrlTld;
    }
  });

  const tldDirty = $derived(tldDraft !== '' && tldDraft !== tld);

  async function toggleLocalUrls() {
    try {
      await projectStore.setLocalUrlsEnabled(!localUrlsOn);
    } catch {
      // error already shown via banner
    }
  }

  async function toggleHttps() {
    httpsError = null;
    try {
      await projectStore.setLocalUrlsHttps(!httpsOn);
    } catch (e) {
      httpsError = String(e);
    }
  }

  async function resync() {
    try {
      await projectStore.resyncLocalUrls();
    } catch {
      // error already shown via banner
    }
  }

  async function saveTld() {
    tldError = null;
    try {
      await projectStore.setLocalUrlTld(tldDraft);
    } catch (e) {
      tldError = String(e);
    }
  }

  const theme = $derived<ThemeChoice>(projectStore.settings?.theme ?? 'system');
  let themeSaving = $state(false);

  async function pickTheme(value: ThemeChoice) {
    if (value === theme || themeSaving) return;
    themeSaving = true;
    try {
      await projectStore.setTheme(value);
    } finally {
      themeSaving = false;
    }
  }

  let confirmResetOpen = $state(false);
  async function performReset() {
    try {
      await projectStore.resetApplication();
      toast.success('Application reset. Project folders on disk were left intact.');
      // Settings.firstRunCompleted is false again → layout's $effect will
      // bounce us to /welcome. Send them home explicitly anyway.
      confirmResetOpen = false;
      await goto('/welcome');
    } catch (e) {
      confirmResetOpen = false;
      toast.error(`Reset failed: ${e}`);
    }
  }
</script>

<header class="page-header" data-tauri-drag-region>
  <h1>Settings</h1>
</header>

<section class="content">
  <div class="panel">
    <h2>Appearance</h2>
    <div class="field">
      <span class="label">Theme</span>
      <div class="theme-picker">
        <button
          type="button"
          class="theme-option"
          class:active={theme === 'system'}
          onclick={() => pickTheme('system')}
          disabled={themeSaving}
        >
          <div class="swatch swatch-system">
            <span class="left"></span>
            <span class="right"></span>
          </div>
          <span class="theme-label">System</span>
        </button>
        <button
          type="button"
          class="theme-option"
          class:active={theme === 'light'}
          onclick={() => pickTheme('light')}
          disabled={themeSaving}
        >
          <div class="swatch swatch-light"></div>
          <span class="theme-label">Light</span>
        </button>
        <button
          type="button"
          class="theme-option"
          class:active={theme === 'dark'}
          onclick={() => pickTheme('dark')}
          disabled={themeSaving}
        >
          <div class="swatch swatch-dark"></div>
          <span class="theme-label">Dark</span>
        </button>
      </div>
      <p class="hint">
        <strong>System</strong> follows your macOS appearance and switches automatically when it
        changes.
      </p>
    </div>
  </div>

  <div class="panel">
    <h2>Local URLs</h2>

    <div class="row-block">
      <div class="copy">
        <div class="title">Use <code>.{tld}</code> hostnames</div>
        <div class="desc">
          Routes <code>http://&lt;project&gt;.{tld}</code> to the right Sail container via a built-in
          Traefik proxy on port 80, with wildcard <code>*.{tld}</code> DNS handled by a local
          dnsmasq. You'll see a single macOS admin prompt the first time you enable this (to add
          <code>/etc/resolver/{tld}</code>) — not one per project.
        </div>
      </div>
      <button
        class="toggle"
        class:on={localUrlsOn}
        onclick={toggleLocalUrls}
        disabled={projectStore.togglingLocalUrls}
        aria-pressed={localUrlsOn}
        aria-label={localUrlsOn ? 'Disable local URLs' : 'Enable local URLs'}
      >
        <span class="knob"></span>
      </button>
    </div>

    <div class="tld-row">
      <div class="copy">
        <div class="title">TLD</div>
        <div class="desc">
          Lowercase letters, digits, hyphens. Default <code>sail</code>. Avoid <code>test</code>
          and <code>local</code> if Laravel Herd / Valet are installed.
        </div>
      </div>
      <div class="tld-input-wrap">
        <span class="tld-prefix">.</span>
        <input
          class="tld-input"
          type="text"
          bind:value={tldDraft}
          spellcheck="false"
          autocomplete="off"
          disabled={projectStore.togglingLocalUrls}
        />
        <button
          class="btn"
          onclick={saveTld}
          disabled={!tldDirty || projectStore.togglingLocalUrls}
        >
          Save
        </button>
      </div>
    </div>
    {#if tldError}
      <p class="hint error-hint">{tldError}</p>
    {/if}

    <div class="row-block" class:disabled={!localUrlsOn}>
      <div class="copy">
        <div class="title">
          HTTPS
          {#if httpsOn}
            <span class="https-tag">on</span>
          {/if}
        </div>
        <div class="desc">
          Serve <code>https://&lt;project&gt;.{tld}</code> alongside HTTP. First time
          you enable this, macOS asks for your password to trust Sail Manager's
          local Certificate Authority — after that, browsers stop warning.
        </div>
      </div>
      <button
        class="toggle"
        class:on={httpsOn}
        onclick={toggleHttps}
        disabled={!localUrlsOn || projectStore.togglingLocalUrlsHttps || projectStore.togglingLocalUrls}
        aria-pressed={httpsOn}
        aria-label={httpsOn ? 'Disable HTTPS for local URLs' : 'Enable HTTPS for local URLs'}
      >
        <span class="knob"></span>
      </button>
    </div>
    {#if httpsError}
      <p class="hint error-hint">{httpsError}</p>
    {/if}
    {#if projectStore.togglingLocalUrlsHttps}
      <p class="hint working">
        <span class="spinner"></span>
        {httpsOn ? 'Removing CA from keychain…' : 'Generating cert and trusting CA…'}
      </p>
    {/if}

    {#if localUrlsOn}
      {@const h = projectStore.localUrlsHealth}
      <div class="health">
        {#if !h}
          <span class="health-pill checking">Checking…</span>
        {:else if h.overallOk}
          <span class="health-pill ok">
            <span class="dot ok"></span>
            All good
          </span>
        {:else}
          <span class="health-pill bad">
            <span class="dot bad"></span>
            Needs repair
          </span>
        {/if}
        <button class="btn" onclick={resync} disabled={projectStore.togglingLocalUrls}>
          <Icon name="refresh" size={12} />
          {h && !h.overallOk ? 'Repair now' : 'Resync'}
        </button>
      </div>
      {#if h && !h.overallOk && h.issues.length > 0}
        <ul class="issues">
          {#each h.issues as issue (issue)}
            <li>{issue}</li>
          {/each}
        </ul>
      {/if}
    {/if}

    {#if projectStore.togglingLocalUrls}
      <p class="hint working">
        <span class="spinner"></span>
        {localUrlsOn ? 'Tearing down…' : 'Setting up Traefik and DNS resolver…'}
      </p>
    {/if}
  </div>

  <div class="panel">
    <h2>Paths</h2>
    <div class="field">
      <label for="projects-root">Projects root folder</label>
      <div class="root-row">
        <input id="projects-root" type="text" value={projectsRoot} readonly />
        <button class="btn" onclick={changeProjectsRoot} disabled={changingRoot}>
          {changingRoot ? 'Changing…' : 'Change…'}
        </button>
      </div>
      <p class="hint">
        Where new Laravel projects are created. Existing projects aren't moved.
      </p>
    </div>
  </div>

  <div class="panel">
    <h2>Editor</h2>
    <div class="field">
      <label for="editor">Open project in</label>
      <select id="editor" value={editor} onchange={saveEditor} disabled={editorSaving}>
        <option value="">— pick one —</option>
        <option value="phpstorm">PhpStorm</option>
        <option value="vscode">Visual Studio Code</option>
        <option value="cursor">Cursor</option>
        <option value="zed">Zed</option>
      </select>
      <p class="hint">
        Used by the <strong>Editor</strong> button on each project's detail page. Uses
        <code>open -a "&lt;App Name&gt;" &lt;path&gt;</code> — the app must be installed on this Mac.
      </p>
    </div>
  </div>

  <div class="panel">
    <h2>About</h2>
    <div class="about-row">
      <div class="about-logo">
        <Icon name="waves" size={20} />
      </div>
      <div class="about-text">
        <div class="about-name">Sail Manager</div>
        <!-- WHY: read from the store, not hardcoded. CI rewrites the version in
             tauri.conf.json / package.json / Cargo.toml per release but never
             touches this file, so a literal here would pin at 0.1.0 forever. -->
        <div class="about-version">
          {projectStore.appVersion ? `Version ${projectStore.appVersion} · ` : ''}MIT License
        </div>
        <p class="about-tagline">
          A native macOS app for running many Laravel Sail projects in parallel.
        </p>
        <div class="about-links">
          <a
            class="about-link"
            href="https://github.com/LuukDahlmans/Laravel-Sail-Manager"
            onclick={(e) => {
              e.preventDefault();
              import('@tauri-apps/plugin-opener').then((m) =>
                m.openUrl('https://github.com/LuukDahlmans/Laravel-Sail-Manager').catch(() => {}),
              );
            }}
          >
            <Icon name="external" size={11} />
            GitHub repository
          </a>
          <a
            class="about-link"
            href="https://github.com/LuukDahlmans/Laravel-Sail-Manager/issues"
            onclick={(e) => {
              e.preventDefault();
              import('@tauri-apps/plugin-opener').then((m) =>
                m.openUrl('https://github.com/LuukDahlmans/Laravel-Sail-Manager/issues').catch(
                  () => {},
                ),
              );
            }}
          >
            <Icon name="external" size={11} />
            Report a bug
          </a>
        </div>
      </div>
    </div>

    <div class="row-block">
      <div class="copy">
        <div class="title">Updates</div>
        <div class="desc">
          {#if projectStore.updateAvailable}
            Version {projectStore.updateAvailable.version} is ready — install it from the sidebar.
          {:else}
            Sail Manager checks automatically at launch and every 6 hours.
          {/if}
        </div>
      </div>
      <button class="btn" onclick={checkForUpdates} disabled={projectStore.updateChecking}>
        {projectStore.updateChecking ? 'Checking…' : 'Check for updates'}
      </button>
    </div>

    <p class="legal">
      <strong>Community project.</strong> This is an independent open-source project and is not
      affiliated with, endorsed by, or sponsored by the Laravel project, its maintainers, or
      Taylor Otwell. Laravel, Laravel Sail, and the Laravel logo are trademarks of Taylor Otwell.
      All other trademarks are the property of their respective owners.
    </p>
  </div>

  <div class="panel danger-panel">
    <h2>Danger zone</h2>
    <div class="row-block">
      <div class="copy">
        <div class="title">Reset application</div>
        <div class="desc">
          Wipes all registered projects, templates, history, auto-commands, and settings; tears
          down the Traefik proxy + dnsmasq containers and removes the <code>/etc/resolver</code>
          entry. <strong>Project folders on disk are not touched</strong> — your code stays where
          it is. Re-import any project you still want to manage afterwards.
        </div>
      </div>
      <button class="btn btn-confirm-danger" onclick={() => (confirmResetOpen = true)}>
        <Icon name="trash" size={12} />
        Reset application
      </button>
    </div>
  </div>
</section>

<ConfirmModal
  open={confirmResetOpen}
  title="Reset Sail Manager?"
  message="This wipes the app's tracking of every project, template, history entry and auto-command, takes down the proxy + DNS containers, and removes /etc/resolver/{tld}."
  detail="Project folders on disk are kept. macOS will prompt for your admin password to undo the resolver entry."
  confirmLabel="Reset everything"
  danger
  onConfirm={performReset}
  onCancel={() => (confirmResetOpen = false)}
/>

<style>
  .page-header {
    padding: 22px 28px 18px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: linear-gradient(180deg, var(--bg-1) 0%, var(--bg) 100%);
    backdrop-filter: blur(8px) saturate(140%);
    -webkit-backdrop-filter: blur(8px) saturate(140%);
    z-index: 5;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.03em;
  }
  .content {
    padding: 22px 28px 32px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 760px;
    margin: 0 auto;
    width: 100%;
  }
  .panel {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 18px 20px;
    box-shadow: var(--shadow-card);
  }
  .panel h2 {
    margin: 0 0 14px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }
  .row-block {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }
  .row-block.disabled {
    opacity: 0.55;
  }
  .https-tag {
    display: inline-block;
    margin-left: 8px;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    background: var(--success-soft);
    color: var(--success);
    vertical-align: 2px;
  }
  .copy {
    flex: 1;
  }
  .title {
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 4px;
  }
  .desc {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.5;
  }
  .desc code,
  .title code,
  .hint code {
    background: var(--bg);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    color: var(--text);
  }

  .toggle {
    flex-shrink: 0;
    width: 38px;
    height: 22px;
    border-radius: 999px;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    position: relative;
    transition: background 0.15s, border-color 0.15s;
    padding: 0;
  }
  .toggle .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text-dim);
    transition: transform 0.15s, background 0.15s;
  }
  .toggle.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .toggle.on .knob {
    background: white;
    transform: translateX(16px);
  }
  .toggle:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .health {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .health-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 500;
    flex: 1;
  }
  .health-pill.ok {
    color: var(--success);
    background: var(--success-soft);
    border: 1px solid var(--success);
  }
  .health-pill.bad {
    color: var(--error);
    background: var(--error-soft);
    border: 1px solid var(--error);
  }
  .health-pill.checking {
    color: var(--text-dim);
    background: var(--bg-3);
    border: 1px solid var(--border);
  }
  .health-pill .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .health-pill .dot.ok {
    background: var(--success);
  }
  .health-pill .dot.bad {
    background: var(--error);
  }
  .issues {
    margin: 8px 0 0;
    padding-left: 18px;
    font-size: 11.5px;
    color: var(--text-dim);
    line-height: 1.5;
  }
  .issues li {
    margin-bottom: 2px;
  }

  .tld-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  .tld-input-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .tld-prefix {
    color: var(--text-faint);
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
  .tld-input {
    width: 110px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  }
  .error-hint {
    color: var(--error);
    margin-top: 8px;
  }

  .theme-picker {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    margin-top: 4px;
  }
  .theme-option {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    cursor: pointer;
    text-align: center;
    transition: border-color 0.12s, background 0.12s;
  }
  .theme-option:hover {
    background: var(--bg-3);
  }
  .theme-option.active {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .theme-option:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .swatch {
    height: 56px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    overflow: hidden;
  }
  .swatch-light {
    background: linear-gradient(180deg, #ffffff 0%, #f1f1f4 100%);
  }
  .swatch-dark {
    background: linear-gradient(180deg, #18181a 0%, #0f0f10 100%);
  }
  .swatch-system {
    display: flex;
  }
  .swatch-system .left {
    flex: 1;
    background: linear-gradient(180deg, #ffffff 0%, #f1f1f4 100%);
  }
  .swatch-system .right {
    flex: 1;
    background: linear-gradient(180deg, #18181a 0%, #0f0f10 100%);
  }
  .theme-label {
    font-size: 12px;
    font-weight: 500;
  }

  .about-row {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .about-logo {
    width: 44px;
    height: 44px;
    border-radius: 11px;
    background: linear-gradient(135deg, var(--accent) 0%, var(--accent-hover) 100%);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.18) inset,
      0 4px 14px var(--accent-glow);
  }
  .about-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1;
  }
  .about-name {
    font-size: 14px;
    font-weight: 650;
    letter-spacing: -0.015em;
    color: var(--text);
  }
  .about-version {
    font-size: 11px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .about-tagline {
    margin: 6px 0 8px;
    font-size: 12.5px;
    color: var(--text-dim);
    line-height: 1.5;
  }
  .about-links {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
  }
  .about-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 500;
    color: var(--accent);
    text-decoration: none;
    transition: opacity 0.15s var(--ease-quick);
  }
  .about-link:hover {
    opacity: 0.75;
  }
  .legal {
    margin: 14px 0 0;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    font-size: 11.5px;
    color: var(--text-dim);
    line-height: 1.6;
  }
  .legal strong {
    color: var(--text);
    font-weight: 600;
  }

  .danger-panel {
    border-color: var(--error-soft);
  }
  .danger-panel h2 {
    color: var(--error);
  }
  .btn-confirm-danger {
    background: var(--error);
    border: 1px solid var(--error);
    color: white;
  }
  .btn-confirm-danger:hover {
    background: #b62519;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field label {
    font-size: 12px;
    color: var(--text-dim);
    font-weight: 500;
  }
  .field input,
  .field select {
    width: 100%;
  }
  .root-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .root-row input {
    flex: 1;
    min-width: 0;
  }
  .root-row .btn {
    flex-shrink: 0;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--text-faint);
    line-height: 1.5;
  }
  .hint.working {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--warning);
    margin-top: 12px;
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
</style>
