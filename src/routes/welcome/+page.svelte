<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { goto } from '$app/navigation';
  import { fade } from 'svelte/transition';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import Icon from '$lib/components/Icon.svelte';
  import { projectStore } from '$lib/projects.svelte';
  import { ui } from '$lib/uiState.svelte';
  import type { ToolStatus } from '$lib/types';

  interface EnvCheck {
    dockerOk: boolean;
    dockerError: string | null;
    projectsRoot: string;
  }

  const REPO_URL = 'https://github.com/LuukDahlmans/Laravel-Sail-Manager';

  let step = $state(1);
  const totalSteps = 6;

  let retrying = $state(false);
  let finishing = $state(false);

  // Default `.sail` URLs to ON — they're the headline feature, and the only
  // friction is one macOS admin prompt when the user clicks "Enable & continue".
  let enableLocalUrls = $state(true);
  // HTTPS defaults to OFF: it adds a keychain-trust dialog and binds :443 (often
  // already held by Herd/Valet), so it's opt-in rather than a first-run gate.
  // Users can turn it on later in Settings.
  let enableHttps = $state(false);
  let tldDraft = $state('sail');
  let tldError = $state<string | null>(null);

  // System-check state. Loaded once on entering step 2; manual refresh button
  // re-runs the probe (e.g. after the user installs a missing tool).
  let dependencies = $state<ToolStatus[]>([]);
  let depsLoading = $state(false);
  let depsLoaded = $state(false);
  let showOptionalTools = $state(false);

  // Projects-folder state for step 3. Editable now: user can browse or type.
  let folderDraft = $state('');
  let folderSaving = $state(false);
  let folderError = $state<string | null>(null);

  let starred = $state(false);

  const dockerLive = $derived(projectStore.envCheck?.dockerOk ?? false);
  const dockerError = $derived(projectStore.envCheck?.dockerError ?? null);

  // For the "block Next" gate on step 2 we want both: (a) Docker binary
  // present (from the deps probe) AND (b) Docker daemon actually responding
  // (from check_environment). The latter is the real liveness signal.
  const dockerBinaryOk = $derived(
    dependencies.find((t) => t.id === 'docker')?.installed ?? false,
  );
  const canPassSystemCheck = $derived(dockerLive && dockerBinaryOk);

  $effect(() => {
    const t = projectStore.settings?.localUrlTld;
    if (t && tldDraft === 'sail') tldDraft = t;
  });

  // Pre-populate the folder draft once envCheck.projectsRoot is known so the
  // user sees their current path the moment step 3 opens.
  $effect(() => {
    const r = projectStore.envCheck?.projectsRoot;
    if (r && folderDraft === '') folderDraft = r;
  });

  const requiredTools = $derived(dependencies.filter((t) => t.required));
  const optionalTools = $derived(dependencies.filter((t) => !t.required));
  const optionalMissing = $derived(
    optionalTools.filter((t) => !t.installed).length,
  );

  $effect(() => {
    if (step === 2 && !depsLoaded) {
      depsLoaded = true;
      void loadDependencies();
    }
  });

  async function loadDependencies() {
    depsLoading = true;
    try {
      dependencies = await projectStore.checkDependencies();
    } finally {
      depsLoading = false;
    }
  }

  async function retryDockerCheck() {
    retrying = true;
    try {
      projectStore.envCheck = await invoke<EnvCheck>('check_environment');
      dependencies = await projectStore.checkDependencies();
    } catch (e) {
      projectStore.error = String(e);
    } finally {
      retrying = false;
    }
  }

  async function openInstallPage(url: string) {
    try {
      await openUrl(url);
    } catch {
      // Soft-fail: the URL is also visible inline in the description.
    }
  }

  function next() {
    if (step < totalSteps) step += 1;
  }

  function prev() {
    if (step > 1) step -= 1;
  }

  async function applyLocalUrls(): Promise<boolean> {
    if (!enableLocalUrls) return true;
    tldError = null;
    try {
      const cleaned = tldDraft.trim().replace(/^\./, '').toLowerCase();
      if (cleaned !== (projectStore.settings?.localUrlTld ?? '')) {
        await projectStore.setLocalUrlTld(cleaned);
      }
      await projectStore.setLocalUrlsEnabled(true);
      if (enableHttps) {
        // Triggers CA generation, wildcard cert with per-project SANs, and
        // the macOS keychain dialog to trust the CA. Failure here is
        // surfaced as a tldError-style message so the user can retry without
        // losing the local-URLs progress they just made.
        await projectStore.setLocalUrlsHttps(true);
      }
      return true;
    } catch (e) {
      tldError = String(e);
      return false;
    }
  }

  async function finish(openCreate: boolean) {
    finishing = true;
    try {
      await projectStore.completeFirstRun();
      await goto('/');
      if (openCreate) ui.showCreateModal = true;
    } catch (e) {
      projectStore.error = String(e);
    } finally {
      finishing = false;
    }
  }

  async function handleNextFromLocalUrls() {
    const ok = await applyLocalUrls();
    if (ok) next();
  }

  async function browseProjectsFolder() {
    folderError = null;
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: 'Choose where to keep Laravel projects',
        defaultPath: folderDraft || projectStore.envCheck?.projectsRoot || undefined,
      });
      if (typeof selected === 'string' && selected.length > 0) {
        folderDraft = selected;
      }
    } catch (e) {
      folderError = `Could not open folder picker: ${e}`;
    }
  }

  async function handleNextFromFolder() {
    folderError = null;
    const draft = folderDraft.trim();
    const current = projectStore.envCheck?.projectsRoot ?? '';
    if (draft && draft !== current) {
      folderSaving = true;
      try {
        await projectStore.setProjectsRoot(draft);
      } catch (e) {
        folderError = String(e);
        folderSaving = false;
        return;
      }
      folderSaving = false;
    }
    next();
  }

  async function openRepoAndStar() {
    starred = true;
    await openInstallPage(REPO_URL);
  }

  const progress = $derived(((step - 1) / (totalSteps - 1)) * 100);
</script>

<div class="welcome" data-tauri-drag-region>
  <div class="drag-strip" data-tauri-drag-region></div>

  <div class="shell">
    <div class="brand">
      <Icon name="waves" size={20} />
      <span>Sail Manager</span>
    </div>

    <div class="progress-wrap" aria-label="Setup progress">
      <ol class="stepper">
        {#each Array.from({ length: totalSteps }, (_, i) => i + 1) as n (n)}
          <li class="dot" class:active={step === n} class:done={step > n}>
            <span class="dot-num">{n}</span>
          </li>
        {/each}
      </ol>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {progress}%"></div>
      </div>
    </div>

    <main class="panel">
      {#if step === 1}
        <section class="step hero" in:fade={{ duration: 200, delay: 80 }}>
          <div class="hero-logo-wrap">
            <span class="ring r1"></span>
            <span class="ring r2"></span>
            <div class="hero-logo">
              <Icon name="waves" size={28} />
            </div>
          </div>

          <h1>Welcome to Sail Manager</h1>
          <p class="lede">
            A macOS dashboard for running every Laravel Sail project at once — without
            port conflicts, terminal-juggling, or hand-editing <code class="inline">.env</code>.
          </p>

          <div class="actions hero-actions">
            <button class="btn btn-primary btn-lg" onclick={next}>Get started</button>
            <span class="hero-hint">Takes about a minute.</span>
          </div>
        </section>
      {:else if step === 2}
        <section class="step" in:fade={{ duration: 200, delay: 80 }}>
          <h2>System check</h2>
          <p class="lede">
            Docker is the only hard requirement. Everything else is optional —
            handy if you sometimes work outside Sail.
          </p>

          <div class="tools" class:loading={depsLoading}>
            {#if depsLoading && dependencies.length === 0}
              <div class="tools-loading">
                <span class="spinner"></span>
                Probing your machine…
              </div>
            {:else}
              {#each requiredTools as tool (tool.id)}
                {@const isDocker = tool.id === 'docker'}
                {@const dockerLiveDown = isDocker && tool.installed && !dockerLive}
                <div
                  class="tool-row"
                  class:ok={tool.installed && !dockerLiveDown}
                  class:warn={dockerLiveDown}
                  class:missing={!tool.installed}
                >
                  <div class="tool-status-dot">
                    {#if tool.installed && !dockerLiveDown}
                      <Icon name="check" size={11} />
                    {:else if dockerLiveDown}
                      <Icon name="alert" size={11} />
                    {:else}
                      <Icon name="x" size={11} />
                    {/if}
                  </div>

                  <div class="tool-body">
                    <div class="tool-head">
                      <span class="tool-label">{tool.label}</span>
                      {#if tool.installed && tool.version}
                        <span class="tool-version">{tool.version}</span>
                      {/if}
                    </div>
                    <div class="tool-purpose">
                      {#if dockerLiveDown}
                        Installed but the daemon isn't responding. Open Docker Desktop and click Refresh.
                      {:else}
                        {tool.purpose}
                      {/if}
                    </div>
                    {#if isDocker && tool.installed && !dockerLive && dockerError}
                      <code class="err-detail">{dockerError}</code>
                    {/if}
                  </div>

                  <div class="tool-action">
                    {#if !tool.installed}
                      <button
                        class="btn"
                        type="button"
                        onclick={() => openInstallPage(tool.installUrl)}
                      >
                        <Icon name="external" size={11} />
                        Install
                      </button>
                    {:else if dockerLiveDown}
                      <button
                        class="btn btn-primary"
                        type="button"
                        onclick={() => projectStore.startDockerDesktop()}
                        disabled={projectStore.startingDocker}
                      >
                        {#if projectStore.startingDocker}
                          <span class="spinner"></span>
                          Starting…
                        {:else}
                          <Icon name="play" size={11} />
                          Start Docker
                        {/if}
                      </button>
                    {/if}
                  </div>
                </div>
              {/each}

              {#if optionalTools.length > 0}
                <button
                  type="button"
                  class="optional-toggle"
                  class:open={showOptionalTools}
                  onclick={() => (showOptionalTools = !showOptionalTools)}
                  aria-expanded={showOptionalTools}
                >
                  <Icon name="chevron" size={12} />
                  <span>
                    {showOptionalTools ? 'Hide' : 'Show'} optional tools
                    <span class="optional-meta">
                      ({optionalTools.length}{optionalMissing > 0
                        ? `, ${optionalMissing} missing`
                        : ', all installed'})
                    </span>
                  </span>
                </button>

                {#if showOptionalTools}
                  <div class="optional-group" in:fade={{ duration: 150 }}>
                    {#each optionalTools as tool (tool.id)}
                      <div
                        class="tool-row optional"
                        class:ok={tool.installed}
                        class:missing={!tool.installed}
                      >
                        <div class="tool-status-dot">
                          {#if tool.installed}
                            <Icon name="check" size={11} />
                          {:else}
                            <span class="dash" aria-hidden="true">—</span>
                          {/if}
                        </div>

                        <div class="tool-body">
                          <div class="tool-head">
                            <span class="tool-label">{tool.label}</span>
                            {#if tool.installed && tool.version}
                              <span class="tool-version">{tool.version}</span>
                            {/if}
                          </div>
                          <div class="tool-purpose">{tool.purpose}</div>
                        </div>

                        <div class="tool-action">
                          {#if !tool.installed}
                            <button
                              class="btn btn-ghost"
                              type="button"
                              onclick={() => openInstallPage(tool.installUrl)}
                            >
                              <Icon name="external" size={11} />
                              Install
                            </button>
                          {/if}
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              {/if}
            {/if}
          </div>

          <div class="actions split">
            <button class="btn btn-ghost" onclick={prev}>Back</button>
            <div class="actions-inline">
              <button class="btn" onclick={retryDockerCheck} disabled={retrying || depsLoading}>
                <Icon name="refresh" size={11} />
                {retrying || depsLoading ? 'Checking…' : 'Refresh'}
              </button>
              <button
                class="btn btn-primary"
                onclick={next}
                disabled={!canPassSystemCheck}
                title={canPassSystemCheck
                  ? ''
                  : 'Docker must be installed and running before you can continue.'}
              >
                Next
              </button>
            </div>
          </div>
        </section>
      {:else if step === 3}
        <section class="step" in:fade={{ duration: 200, delay: 80 }}>
          <h2>Projects folder</h2>
          <p class="lede">
            All Laravel projects you create or import will live here. Pick a
            folder you can find later — Documents, your home directory,
            wherever feels right.
          </p>

          <div class="folder-row">
            <span class="path-icon"><Icon name="folder" size={16} /></span>
            <input
              class="folder-input"
              type="text"
              spellcheck="false"
              autocomplete="off"
              placeholder="/Users/you/SailProjects"
              bind:value={folderDraft}
              disabled={folderSaving}
            />
            <button
              type="button"
              class="btn"
              onclick={browseProjectsFolder}
              disabled={folderSaving}
            >
              <Icon name="folder" size={11} />
              Browse
            </button>
          </div>

          {#if folderError}
            <p class="hint error-hint">{folderError}</p>
          {:else}
            <p class="hint">
              You can change this later from <strong>Settings</strong>. We'll
              create the folder if it doesn't exist yet.
            </p>
          {/if}

          <div class="actions split">
            <button class="btn btn-ghost" onclick={prev} disabled={folderSaving}>Back</button>
            <button
              class="btn btn-primary"
              onclick={handleNextFromFolder}
              disabled={folderSaving || folderDraft.trim().length === 0}
            >
              {#if folderSaving}
                <span class="spinner"></span>
                Saving…
              {:else}
                Next
              {/if}
            </button>
          </div>
        </section>
      {:else if step === 4}
        <section class="step" in:fade={{ duration: 200, delay: 80 }}>
          <h2>Local URLs <span class="optional-tag">(optional)</span></h2>
          <p class="lede">
            Skip the <span class="port-num">localhost:54321</span> guessing game.
            Open your apps at a real hostname instead.
          </p>

          <div class="url-preview" aria-hidden="true">
            <div class="url-chip before">
              <span class="url-dot"></span>
              <span class="url-text">localhost:54321</span>
            </div>
            <span class="url-arrow">→</span>
            <div class="url-chip after">
              <span class="url-dot"></span>
              <span class="url-text">{enableLocalUrls && enableHttps ? 'https://' : ''}myapp.{tldDraft || 'sail'}</span>
            </div>
          </div>

          <p class="lede subtle">
            We'll set up a small proxy and a DNS resolver. One macOS admin
            prompt — once per TLD, not per project.
          </p>

          <label class="row-block">
            <div class="copy">
              <div class="title">Enable local <code>.{tldDraft || 'sail'}</code> URLs</div>
              <div class="desc">
                Turning this on prompts once for a macOS admin password to install the resolver
                file (you'll need an administrator account). If you skip it, projects still open at
                <code>localhost:&lt;port&gt;</code>. Change this any time in Settings.
              </div>
            </div>
            <button
              type="button"
              class="toggle"
              class:on={enableLocalUrls}
              onclick={() => (enableLocalUrls = !enableLocalUrls)}
              aria-pressed={enableLocalUrls}
              aria-label={enableLocalUrls ? 'Disable local URLs' : 'Enable local URLs'}
            >
              <span class="knob"></span>
            </button>
          </label>

          {#if enableLocalUrls}
            <div class="tld-row">
              <div class="copy">
                <div class="title">TLD</div>
                <div class="desc">
                  Lowercase letters, digits, hyphens. Default <code>sail</code>. Avoid
                  <code>test</code> and <code>local</code> if you also use Herd or Valet.
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
                />
              </div>
            </div>

            <label class="row-block">
              <div class="copy">
                <div class="title">Use HTTPS</div>
                <div class="desc">
                  Generates a local Certificate Authority and trusts it in your login
                  keychain — one extra dialog ("security wants to access your keychain"),
                  no admin password. Browsers stop warning about <code>https://&lt;project&gt;.{tldDraft || 'sail'}</code>.
                </div>
              </div>
              <button
                type="button"
                class="toggle"
                class:on={enableHttps}
                onclick={() => (enableHttps = !enableHttps)}
                aria-pressed={enableHttps}
                aria-label={enableHttps ? 'Disable HTTPS' : 'Enable HTTPS'}
              >
                <span class="knob"></span>
              </button>
            </label>
          {/if}

          {#if tldError}
            <p class="hint error-hint">{tldError}</p>
          {/if}

          {#if projectStore.togglingLocalUrls}
            <p class="hint working">
              <span class="spinner"></span>
              Setting up Traefik and the resolver…
            </p>
          {:else if projectStore.togglingLocalUrlsHttps}
            <p class="hint working">
              <span class="spinner"></span>
              Generating cert and trusting CA…
            </p>
          {/if}

          <div class="actions split">
            <button class="btn btn-ghost" onclick={prev}>Back</button>
            <div class="actions-inline">
              <button class="btn btn-ghost" onclick={next}>Skip</button>
              <button
                class="btn btn-primary"
                onclick={handleNextFromLocalUrls}
                disabled={projectStore.togglingLocalUrls || projectStore.togglingLocalUrlsHttps}
              >
                {enableLocalUrls ? 'Enable & continue' : 'Next'}
              </button>
            </div>
          </div>
        </section>
      {:else if step === 5}
        <section class="step star-step" in:fade={{ duration: 200, delay: 80 }}>
          <div class="star-icon-wrap">
            <Icon name="star" size={28} />
          </div>
          <h2>Show some love</h2>
          <p class="lede">
            Sail Manager is free and open source. Stars on GitHub help other Laravel devs
            find it — and they keep me motivated to ship more.
          </p>

          <div class="repo-card">
            <div class="repo-info">
              <code class="repo-path">LuukDahlmans/Laravel-Sail-Manager</code>
              <span class="repo-sub">MIT licensed · macOS · Tauri 2 + Svelte</span>
            </div>
            <button
              class="btn btn-primary star-btn"
              type="button"
              onclick={openRepoAndStar}
            >
              <Icon name="star" size={12} />
              {starred ? 'Opened on GitHub' : 'Star on GitHub'}
            </button>
          </div>

          <p class="hint">
            Already starred or not your thing? No worries — hit Skip.
          </p>

          <div class="actions split">
            <button class="btn btn-ghost" onclick={prev}>Back</button>
            <div class="actions-inline">
              <button class="btn btn-ghost" onclick={next}>Skip</button>
              <button class="btn btn-primary" onclick={next}>
                {starred ? 'Continue' : 'Maybe later'}
              </button>
            </div>
          </div>
        </section>
      {:else if step === 6}
        <section class="step hero" in:fade={{ duration: 200, delay: 80 }}>
          <div class="hero-logo-wrap small">
            <div class="hero-logo">
              <Icon name="check" size={24} />
            </div>
          </div>
          <h1>You're ready</h1>
          <p class="lede">
            Sail Manager is set up. Open the dashboard and spin up your first project
            whenever you're ready.
          </p>

          <div class="actions hero-actions">
            <button
              class="btn btn-primary btn-lg"
              onclick={() => finish(false)}
              disabled={finishing}
            >
              {#if finishing}
                <span class="spinner"></span>
                Launching…
              {:else}
                Launch application
              {/if}
            </button>
            {#if !finishing}
              <span class="hero-hint">You can create projects from the dashboard.</span>
            {/if}
          </div>

          <div class="actions split back-only">
            <button class="btn btn-ghost" onclick={prev} disabled={finishing}>Back</button>
          </div>
        </section>
      {/if}
    </main>

    <footer class="foot">
      Step {step} of {totalSteps}
    </footer>
  </div>
</div>

<style>
  .welcome {
    height: 100vh;
    width: 100vw;
    background:
      radial-gradient(1100px 600px at 78% -10%, var(--accent-soft), transparent 60%),
      radial-gradient(900px 500px at -10% 110%, rgba(16, 185, 129, 0.08), transparent 60%),
      var(--bg);
    color: var(--text);
    display: flex;
    flex-direction: column;
    align-items: center;
    overflow: hidden;
  }
  .drag-strip {
    height: 28px;
    width: 100%;
    flex-shrink: 0;
  }
  .shell {
    width: min(680px, 92vw);
    margin: auto 0;
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 0 0 32px;
  }

  .brand {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    align-self: center;
    color: var(--text-dim);
    font-size: 12px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    font-weight: 600;
  }

  .progress-wrap {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .stepper {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }
  .dot {
    width: 22px;
    height: 22px;
    border-radius: 999px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    display: grid;
    place-items: center;
    color: var(--text-faint);
    font-size: 10px;
    font-weight: 600;
    transition: background 0.18s var(--ease), color 0.18s var(--ease),
      border-color 0.18s var(--ease), transform 0.18s var(--ease);
  }
  .dot.active {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
    transform: scale(1.1);
    box-shadow: 0 0 0 4px var(--accent-soft);
  }
  .dot.done {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .dot-num {
    line-height: 1;
  }

  .progress-bar {
    width: min(360px, 80%);
    height: 3px;
    align-self: center;
    background: var(--bg-3);
    border-radius: 999px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent), var(--accent-hover));
    border-radius: 999px;
    transition: width 0.4s var(--ease);
    box-shadow: 0 0 8px var(--accent-glow);
  }

  .panel {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 28px 28px 22px;
    box-shadow: 0 30px 80px -40px rgba(0, 0, 0, 0.7);
  }

  .step h1 {
    margin: 0 0 8px;
    font-size: 26px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .step h2 {
    margin: 0 0 8px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .step .lede {
    margin: 0 0 18px;
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.55;
  }

  /* Centered hero variants for step 1 + step 6. */
  .hero {
    text-align: center;
  }
  .hero h1 {
    text-align: center;
  }
  .hero .lede {
    text-align: center;
    margin-left: auto;
    margin-right: auto;
    max-width: 460px;
  }

  .hero-logo-wrap {
    position: relative;
    width: 86px;
    height: 86px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto 18px;
  }
  .hero-logo-wrap.small {
    width: 60px;
    height: 60px;
    margin-bottom: 14px;
  }
  .hero-logo {
    width: 56px;
    height: 56px;
    border-radius: 14px;
    background: linear-gradient(135deg, var(--accent) 0%, var(--accent-hover) 100%);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.18) inset,
      0 0 0 1px rgba(255, 255, 255, 0.06),
      0 10px 32px var(--accent-glow);
    animation: bob 2.6s ease-in-out infinite;
    position: relative;
    z-index: 2;
  }
  .hero-logo-wrap.small .hero-logo {
    width: 46px;
    height: 46px;
    border-radius: 12px;
    animation: none;
  }

  .ring {
    position: absolute;
    width: 56px;
    height: 56px;
    border-radius: 50%;
    border: 1.5px solid var(--accent);
    opacity: 0;
    animation: pulse 2.6s ease-out infinite;
    z-index: 1;
  }
  .ring.r2 {
    animation-delay: 1.3s;
  }

  .optional-tag {
    color: var(--text-faint);
    font-weight: 400;
    font-size: 13px;
    margin-left: 6px;
  }

  code {
    background: var(--bg);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    color: var(--text);
  }

  .actions {
    margin-top: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .actions.split {
    justify-content: space-between;
  }
  .actions-inline {
    display: flex;
    gap: 8px;
  }
  .hero-actions {
    flex-direction: column;
    gap: 10px;
    margin-top: 6px;
  }
  .hero-hint {
    font-size: 11px;
    color: var(--text-faint);
    letter-spacing: 0.02em;
  }
  .btn-lg {
    padding: 10px 22px;
    font-size: 13px;
    font-weight: 600;
    border-radius: 999px;
    box-shadow: 0 6px 20px var(--accent-glow);
  }

  /* Inline code in prose: monospace + dim, no chunky pill background. */
  code.inline {
    background: transparent;
    padding: 0;
    color: var(--text-dim);
    font-size: 12px;
  }

  .lede.subtle {
    font-size: 12px;
    margin-top: 14px;
    margin-bottom: 8px;
    color: var(--text-faint);
  }

  .port-num {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12px;
    color: var(--text-dim);
  }

  /* Before/after URL preview on the Local URLs step. */
  .url-preview {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 18px 14px;
    margin: 4px 0 6px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .url-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
  .url-chip .url-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-faint);
    flex-shrink: 0;
  }
  .url-chip.before {
    color: var(--text-dim);
    text-decoration: line-through;
    text-decoration-color: var(--text-faint);
    text-decoration-thickness: 1px;
  }
  .url-chip.after {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-soft);
    box-shadow: 0 0 0 1px var(--accent-soft) inset;
  }
  .url-chip.after .url-dot {
    background: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .url-arrow {
    color: var(--text-faint);
    font-size: 16px;
    line-height: 1;
  }

  /* System-check tools list. */
  .tools {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 4px;
  }
  .tools-loading {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px;
    color: var(--text-dim);
    font-size: 12px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .tool-row {
    display: grid;
    grid-template-columns: 22px 1fr auto;
    align-items: start;
    gap: 12px;
    padding: 12px 14px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    transition: border-color 0.15s var(--ease-quick);
  }
  .tool-row.ok {
    border-color: var(--success);
    background: linear-gradient(180deg, var(--success-soft) 0%, var(--bg-3) 100%);
  }
  .tool-row.warn {
    border-color: var(--warning);
    background: linear-gradient(180deg, var(--warning-soft) 0%, var(--bg-3) 100%);
  }
  .tool-row.missing:not(.optional) {
    border-color: var(--error);
    background: linear-gradient(180deg, var(--error-soft) 0%, var(--bg-3) 100%);
  }

  .tool-status-dot {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    margin-top: 2px;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .tool-row.ok .tool-status-dot {
    background: var(--success);
    border-color: var(--success);
    color: white;
  }
  .tool-row.warn .tool-status-dot {
    background: var(--warning);
    border-color: var(--warning);
    color: white;
  }
  .tool-row.missing:not(.optional) .tool-status-dot {
    background: var(--error);
    border-color: var(--error);
    color: white;
  }
  .dash {
    font-size: 10px;
    line-height: 1;
  }

  .tool-body {
    min-width: 0;
  }
  .tool-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 3px;
  }
  .tool-label {
    font-size: 13px;
    font-weight: 600;
  }
  .tool-version {
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 11px;
    color: var(--text-dim);
    background: var(--bg);
    padding: 1px 6px;
    border-radius: 3px;
  }
  .tool-purpose {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.45;
  }
  .tool-action {
    align-self: center;
  }

  /* Star step. */
  .star-step {
    text-align: center;
  }
  .star-icon-wrap {
    width: 60px;
    height: 60px;
    margin: 0 auto 14px;
    border-radius: 14px;
    background: linear-gradient(135deg, var(--accent) 0%, var(--accent-hover) 100%);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.18) inset,
      0 10px 32px var(--accent-glow);
  }
  .star-step h2 {
    text-align: center;
    font-size: 22px;
  }
  .star-step .lede {
    text-align: center;
    margin-left: auto;
    margin-right: auto;
    max-width: 460px;
  }

  .repo-card {
    margin-top: 6px;
    padding: 14px 16px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    text-align: left;
  }
  .repo-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .repo-path {
    font-size: 13px;
    font-weight: 600;
    background: transparent;
    padding: 0;
    color: var(--text);
  }
  .repo-sub {
    font-size: 11px;
    color: var(--text-faint);
  }
  .star-btn {
    flex-shrink: 0;
  }

  .err-detail {
    margin-top: 6px;
    display: block;
    font-size: 11px;
    background: rgba(0, 0, 0, 0.25);
    padding: 6px 8px;
    border-radius: 4px;
    color: var(--text);
    word-break: break-word;
  }

  .path-icon {
    color: var(--text-dim);
    flex-shrink: 0;
  }

  /* Editable folder row on the projects-folder step. */
  .folder-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--bg-3);
    border: 1px solid var(--border);
    transition: border-color 0.15s var(--ease-quick);
  }
  .folder-row:focus-within {
    border-color: var(--accent);
  }
  .folder-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    padding: 4px 0;
    color: var(--text);
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
  .folder-input:focus {
    outline: none;
    border: none;
  }
  .folder-input:disabled {
    color: var(--text-faint);
  }

  /* Optional-tools toggle on the system-check step. */
  .optional-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 4px;
    margin-top: 4px;
    background: transparent;
    border: none;
    color: var(--text-dim);
    font-size: 12px;
    cursor: pointer;
    align-self: flex-start;
    transition: color 0.15s var(--ease-quick);
  }
  .optional-toggle:hover {
    color: var(--text);
  }
  .optional-toggle :global(svg) {
    transition: transform 0.18s var(--ease-quick);
  }
  .optional-toggle.open :global(svg) {
    transform: rotate(90deg);
  }
  .optional-meta {
    color: var(--text-faint);
    margin-left: 4px;
  }
  .optional-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 2px;
  }
  /* Optional rows are flatter — they're not gating Next. */
  .tool-row.optional {
    background: var(--bg-3);
    border-color: var(--border);
  }
  .tool-row.optional.ok {
    background: var(--bg-3);
    border-color: var(--border);
  }

  .hint {
    margin: 12px 0 0;
    font-size: 11px;
    color: var(--text-faint);
    line-height: 1.5;
  }
  .hint.error-hint {
    color: var(--error);
  }
  .hint.working {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--warning);
  }
  .spinner {
    width: 11px;
    height: 11px;
    border: 1.5px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .row-block {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 12px 14px;
    border-radius: var(--radius-sm);
    background: var(--bg-3);
    border: 1px solid var(--border);
    cursor: pointer;
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
    cursor: pointer;
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

  .tld-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    margin-top: 12px;
    padding: 12px 14px;
    border-radius: var(--radius-sm);
    background: var(--bg-3);
    border: 1px solid var(--border);
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

  .actions.back-only {
    margin-top: 18px;
    border-top: 1px solid var(--border);
    padding-top: 14px;
  }

  .foot {
    text-align: center;
    color: var(--text-faint);
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  @keyframes bob {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(-4px);
    }
  }

  @keyframes pulse {
    0% {
      transform: scale(0.85);
      opacity: 0.45;
    }
    100% {
      transform: scale(1.7);
      opacity: 0;
    }
  }
</style>
