<script lang="ts">
  import { ui } from '$lib/uiState.svelte';
  import { projectStore } from '$lib/projects.svelte';
  import type { ServiceKind, StarterKit, PhpVersion } from '$lib/types';
  import Icon from './Icon.svelte';
  import { goto } from '$app/navigation';

  let nameInput: HTMLInputElement | undefined = $state();
  let name = $state('');
  let starterKit = $state<StarterKit>('none');
  let phpVersion = $state<PhpVersion>('8.3');
  let services = $state<Record<ServiceKind, boolean>>({
    mysql: true,
    pgsql: false,
    mariadb: false,
    redis: true,
    valkey: false,
    memcached: false,
    mailpit: true,
    meilisearch: false,
    typesense: false,
    mongodb: false,
    minio: false,
    selenium: false,
    soketi: false,
  });
  let customServicesText = $state('');
  let creating = $state(false);
  let createError = $state<string | null>(null);
  let logEl: HTMLElement | undefined = $state();

  type Category = 'database' | 'cache' | 'search' | 'mail' | 'storage' | 'tools';

  const serviceList: {
    key: ServiceKind;
    label: string;
    description: string;
    initial: string;
    category: Category;
  }[] = [
    { key: 'mysql', label: 'MySQL', description: 'Default database', initial: 'My', category: 'database' },
    { key: 'pgsql', label: 'PostgreSQL', description: 'Postgres database', initial: 'Pg', category: 'database' },
    { key: 'mariadb', label: 'MariaDB', description: 'Drop-in for MySQL', initial: 'Ma', category: 'database' },
    { key: 'mongodb', label: 'MongoDB', description: 'Document database', initial: 'Mo', category: 'database' },
    { key: 'redis', label: 'Redis', description: 'Cache + queue driver', initial: 'Rd', category: 'cache' },
    { key: 'valkey', label: 'Valkey', description: 'Redis-compatible fork', initial: 'Vk', category: 'cache' },
    { key: 'memcached', label: 'Memcached', description: 'In-memory KV store', initial: 'Mc', category: 'cache' },
    { key: 'meilisearch', label: 'Meilisearch', description: 'Full-text search', initial: 'Me', category: 'search' },
    { key: 'typesense', label: 'Typesense', description: 'Search engine', initial: 'Ty', category: 'search' },
    { key: 'mailpit', label: 'Mailpit', description: 'Local SMTP catcher', initial: 'Mp', category: 'mail' },
    { key: 'minio', label: 'MinIO', description: 'S3-compatible storage', initial: 'Mn', category: 'storage' },
    { key: 'selenium', label: 'Selenium', description: 'Browser tests (Dusk)', initial: 'Se', category: 'tools' },
    { key: 'soketi', label: 'Soketi', description: 'Pusher-compatible WS', initial: 'So', category: 'tools' },
  ];

  const categoryLabels: Record<Category, string> = {
    database: 'Databases',
    cache: 'Cache & queue',
    search: 'Search',
    mail: 'Mail',
    storage: 'Storage',
    tools: 'Tools',
  };

  const grouped = $derived.by(() => {
    const order: Category[] = ['database', 'cache', 'search', 'mail', 'storage', 'tools'];
    return order
      .map((cat) => ({ cat, items: serviceList.filter((s) => s.category === cat) }))
      .filter((g) => g.items.length > 0);
  });

  const selectedCount = $derived(
    (Object.keys(services) as ServiceKind[]).filter((s) => services[s]).length,
  );

  // Specific, actionable name errors. Returning null means "OK or not yet
  // started" — we only show the error UI when there's actually something
  // wrong with what the user has typed.
  const nameError = $derived.by<string | null>(() => {
    if (name.length === 0) return null;
    if (/\s/.test(name)) return 'Spaces aren\'t allowed. Use hyphens instead.';
    if (name.length === 1) return 'Name must be at least 2 characters.';
    if (name.length > 41) return 'Name is too long (41 characters max).';
    if (!/^[a-zA-Z]/.test(name)) return 'Name must start with a letter.';
    const bad = name.match(/[^a-zA-Z0-9-]/);
    if (bad) return `"${bad[0]}" isn't allowed. Use letters, digits, and hyphens only.`;
    return null;
  });
  const valid = $derived(nameError === null && name.length > 0);

  function close() {
    if (creating) return;
    ui.showCreateModal = false;
    reset();
  }

  function reset() {
    name = '';
    starterKit = 'none';
    phpVersion = '8.3';
    services = {
      mysql: true,
      pgsql: false,
      mariadb: false,
      redis: true,
      valkey: false,
      memcached: false,
      mailpit: true,
      meilisearch: false,
      typesense: false,
      mongodb: false,
      minio: false,
      selenium: false,
      soketi: false,
    };
    customServicesText = '';
    createError = null;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!valid || creating) return;
    creating = true;
    createError = null;
    const selected = (Object.keys(services) as ServiceKind[]).filter((s) => services[s]);
    const custom = customServicesText
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    try {
      const project = await projectStore.create({
        name: name.trim(),
        starterKit,
        phpVersion,
        services: selected,
        customServices: custom,
      });
      creating = false;
      ui.showCreateModal = false;
      reset();
      // Kick off start in the background so the user lands on the detail
      // page already seeing the "starting…" state. Errors surface via the
      // store → toast bridge so we don't need to block the navigation.
      void projectStore.start(project.id).catch(() => {});
      goto(`/projects/${project.id}`);
    } catch (e) {
      createError = String(e);
      creating = false;
    }
  }

  $effect(() => {
    if (creating && logEl && projectStore.createOutput.length > 0) {
      logEl.scrollTop = logEl.scrollHeight;
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }

  $effect(() => {
    if (!ui.showCreateModal) return;
    const tpl = ui.pendingTemplate;
    if (tpl) {
      starterKit = tpl.starterKit;
      phpVersion = (tpl.phpVersion as PhpVersion) ?? '8.3';
      services = {
        mysql: tpl.services.includes('mysql'),
        pgsql: tpl.services.includes('pgsql'),
        mariadb: tpl.services.includes('mariadb'),
        redis: tpl.services.includes('redis'),
        valkey: tpl.services.includes('valkey'),
        memcached: tpl.services.includes('memcached'),
        mailpit: tpl.services.includes('mailpit'),
        meilisearch: tpl.services.includes('meilisearch'),
        typesense: tpl.services.includes('typesense'),
        mongodb: tpl.services.includes('mongodb'),
        minio: tpl.services.includes('minio'),
        selenium: tpl.services.includes('selenium'),
        soketi: tpl.services.includes('soketi'),
      };
      ui.pendingTemplate = null;
    }
    nameInput?.focus();
  });
</script>

{#if ui.showCreateModal}
  <div class="backdrop" onclick={close} role="presentation"></div>

  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title" tabindex="-1" onkeydown={onKey}>
    <header>
      <h2 id="modal-title">{creating ? `Creating ${name}…` : 'New Laravel project'}</h2>
      <button class="btn btn-ghost btn-icon" onclick={close} aria-label="Close" disabled={creating}>
        <Icon name="x" size={14} />
      </button>
    </header>

    {#if creating}
      <div class="creating-body">
        <p class="hint">
          Pulling the Laravel Sail composer image and running <code>laravel new</code> +
          <code>sail:install</code>. This usually takes 1–3 minutes on first use.
        </p>
        <div class="log-output" bind:this={logEl}>
          {#each projectStore.createOutput as line (line)}
            <div class="log-line">{line}</div>
          {:else}
            <div class="log-line dim">Starting…</div>
          {/each}
        </div>
      </div>
    {:else if createError}
      <div class="creating-body">
        <p class="error">Could not create project.</p>
        <pre class="error-message selectable">{createError}</pre>
        {#if projectStore.createOutput.length > 0}
          <p class="hint">Last output from the scaffolder:</p>
          <div class="log-output selectable">
            {#each projectStore.createOutput as line, i (i)}
              <div class="log-line">{line}</div>
            {/each}
          </div>
        {/if}
        <footer>
          <button type="button" class="btn btn-ghost" onclick={close}>Close</button>
          <button type="button" class="btn btn-primary" onclick={() => (createError = null)}>Try again</button>
        </footer>
      </div>
    {:else}
    <form onsubmit={submit}>
      <div class="field">
        <label for="proj-name">Project name</label>
        <input
          id="proj-name"
          type="text"
          placeholder="acme-shop"
          bind:value={name}
          bind:this={nameInput}
          autocomplete="off"
          spellcheck="false"
          class:invalid={nameError !== null}
          aria-invalid={nameError !== null}
          aria-describedby="proj-name-hint"
        />
        {#if nameError}
          <p id="proj-name-hint" class="hint name-error" role="alert">{nameError}</p>
        {:else}
          <p id="proj-name-hint" class="hint">Used as folder name and Compose project name. Letters, digits, hyphens.</p>
        {/if}
      </div>

      <div class="row">
        <div class="field">
          <label for="starter-kit">Starter kit</label>
          <select id="starter-kit" bind:value={starterKit}>
            <option value="none">None (plain Laravel)</option>
            <option value="breeze">Breeze</option>
            <option value="jetstream">Jetstream</option>
          </select>
        </div>

        <div class="field">
          <label for="php-version">PHP version</label>
          <select id="php-version" bind:value={phpVersion}>
            <option value="8.2">8.2</option>
            <option value="8.3">8.3</option>
            <option value="8.4">8.4</option>
          </select>
        </div>
      </div>

      <div class="field">
        <div class="label-row">
          <span class="label">Services</span>
          <span class="label-meta">{selectedCount} selected</span>
        </div>
        <div class="services-scroll">
          {#each grouped as group (group.cat)}
            <div class="service-group">
              <div class="group-head">{categoryLabels[group.cat]}</div>
              <div class="services">
                {#each group.items as svc (svc.key)}
                  <label class="service-card cat-{svc.category}" class:checked={services[svc.key]}>
                    <input type="checkbox" bind:checked={services[svc.key]} />
                    <span class="service-badge">{svc.initial}</span>
                    <span class="service-info">
                      <span class="service-name">{svc.label}</span>
                      <span class="service-desc">{svc.description}</span>
                    </span>
                    <span class="service-check">
                      {#if services[svc.key]}
                        <Icon name="play" size={9} />
                      {/if}
                    </span>
                  </label>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      </div>

      <div class="field">
        <label for="proj-custom-services">Custom services <span class="optional">optional</span></label>
        <input
          id="proj-custom-services"
          type="text"
          placeholder="comma-separated, e.g. mariadb,opensearch"
          bind:value={customServicesText}
          autocomplete="off"
          spellcheck="false"
        />
        <p class="hint">
          Passed verbatim to <code>sail:install --with</code> after the checkboxes above. Useful for
          services Sail supports that aren't surfaced as checkboxes yet.
        </p>
      </div>

      <footer>
        <button type="button" class="btn btn-ghost" onclick={close}>Cancel</button>
        <button type="submit" class="btn btn-primary" disabled={!valid}>Create project</button>
      </footer>
    </form>
    {/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(2px);
    z-index: 50;
    animation: fade 0.15s ease;
  }
  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    z-index: 51;
    animation: pop 0.18s ease;
    /* Flex column so the form is the scrollable middle and the footer can
       sit pinned at the bottom of the modal — stops the Create button
       scrolling out of view on short viewports. */
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  form {
    padding: 18px 20px 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field label,
  .field .label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-dim);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-faint);
  }
  .hint.name-error {
    color: var(--error);
  }
  .field input.invalid {
    border-color: var(--error);
  }
  .field input.invalid:focus {
    border-color: var(--error);
    box-shadow: 0 0 0 3px var(--error-soft);
  }

  .label-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 2px;
  }
  .label-meta {
    font-size: 11px;
    color: var(--text-faint);
  }

  .services-scroll {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: 320px;
    overflow-y: scroll;
    padding-right: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 10px 8px 10px 10px;
    background: var(--bg);
    scrollbar-gutter: stable;
  }
  /* Always-visible scrollbar so users can see there's more to scroll. */
  .services-scroll::-webkit-scrollbar {
    width: 10px;
  }
  .services-scroll::-webkit-scrollbar-track {
    background: var(--bg-3);
    border-radius: 6px;
    margin: 4px 0;
  }
  .services-scroll::-webkit-scrollbar-thumb {
    background: var(--border-strong);
    border-radius: 6px;
    border: 2px solid var(--bg-3);
    min-height: 30px;
  }
  .services-scroll::-webkit-scrollbar-thumb:hover {
    background: var(--text-faint);
  }
  .service-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .group-head {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    font-weight: 600;
    color: var(--text-faint);
    padding-left: 2px;
  }
  .services {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .service-card {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px 9px 9px;
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    background: var(--bg);
    transition: background 0.15s var(--ease-quick), border-color 0.15s var(--ease-quick),
      transform 0.08s var(--ease-quick);
  }
  .service-card:hover {
    background: var(--bg-3);
    border-color: var(--border-strong);
  }
  .service-card.checked {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .service-card input[type="checkbox"] {
    position: absolute;
    opacity: 0;
    pointer-events: none;
    width: 0;
    height: 0;
  }
  .service-badge {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text);
    background: var(--bg-3);
    border: 1px solid var(--border-strong);
    flex-shrink: 0;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }
  .cat-database .service-badge {
    background: rgba(56, 132, 255, 0.15);
    color: #6aa9ff;
    border-color: rgba(56, 132, 255, 0.35);
  }
  .cat-cache .service-badge {
    background: rgba(245, 158, 11, 0.18);
    color: #f5a623;
    border-color: rgba(245, 158, 11, 0.4);
  }
  .cat-search .service-badge {
    background: rgba(168, 85, 247, 0.18);
    color: #c084fc;
    border-color: rgba(168, 85, 247, 0.4);
  }
  .cat-mail .service-badge {
    background: rgba(20, 184, 166, 0.18);
    color: #2dd4bf;
    border-color: rgba(20, 184, 166, 0.4);
  }
  .cat-storage .service-badge {
    background: rgba(16, 185, 129, 0.18);
    color: #34d399;
    border-color: rgba(16, 185, 129, 0.4);
  }
  .cat-tools .service-badge {
    background: var(--bg-4);
    color: var(--text-dim);
  }
  .service-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  .service-name {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text);
    line-height: 1.25;
  }
  .service-desc {
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .service-check {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1.5px solid var(--border-strong);
    background: var(--bg-2);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: white;
    transition: background 0.15s, border-color 0.15s;
  }
  .checked .service-check {
    background: var(--accent);
    border-color: var(--accent);
  }
  .checked .service-check :global(svg) {
    transform: rotate(90deg);
  }
  .optional {
    color: var(--text-faint);
    font-weight: normal;
    font-size: 11px;
  }
  .service {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    transition: background 0.1s, border-color 0.1s;
    background: var(--bg);
  }
  .service:hover {
    background: var(--bg-3);
  }
  .service.checked {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .service input {
    margin-top: 2px;
    accent-color: var(--accent);
  }
  .service-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .service-label {
    font-size: 12.5px;
    font-weight: 500;
  }
  .service-desc {
    font-size: 11px;
    color: var(--text-dim);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    border-top: 1px solid var(--border);
    background: var(--bg-2);
    margin: 16px -20px 0;
    padding: 12px 20px;
    /* Pin to the bottom of the form's scroll viewport so the Create button
       is always reachable even when the services list pushes content past
       the viewport. */
    position: sticky;
    bottom: 0;
    z-index: 1;
  }

  .creating-body {
    padding: 18px 20px 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
  }
  .creating-body footer {
    margin: 12px -20px 0;
  }
  .creating-body code {
    background: var(--bg);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  }
  .creating-body .error {
    margin: 0;
    color: var(--error);
    font-size: 13px;
    font-weight: 500;
  }
  .log-output {
    background: var(--code-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--code-text);
    height: 220px;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .error-message {
    margin: 0;
    padding: 10px 12px;
    background: var(--error-soft);
    border: 1px solid var(--error);
    border-radius: 6px;
    color: var(--error);
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .log-line {
    white-space: pre-wrap;
    word-break: break-all;
  }
  .log-line.dim {
    color: var(--text-faint);
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translate(-50%, -48%) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translate(-50%, -50%) scale(1);
    }
  }
</style>
