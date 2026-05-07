<script lang="ts">
  import { projectStore } from '$lib/projects.svelte';
  import { ui } from '$lib/uiState.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import ConfirmModal from '$lib/components/ConfirmModal.svelte';
  import type { Template, TemplateInput, ServiceKind, StarterKit, PhpVersion } from '$lib/types';

  type ServiceOption = { key: ServiceKind; label: string };
  const serviceOptions: ServiceOption[] = [
    { key: 'mysql', label: 'MySQL' },
    { key: 'redis', label: 'Redis' },
    { key: 'mailpit', label: 'Mailpit' },
    { key: 'meilisearch', label: 'Meilisearch' },
    { key: 'minio', label: 'MinIO' },
  ];

  const templates = $derived(projectStore.templates);

  // Form modal state
  let formOpen = $state(false);
  let editingId = $state<string | null>(null);
  let formName = $state('');
  let formDescription = $state('');
  let formStarterKit = $state<StarterKit>('none');
  let formPhpVersion = $state<PhpVersion>('8.3');
  let formServices = $state<Record<ServiceKind, boolean>>({
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
  let saving = $state(false);
  let formError = $state<string | null>(null);

  // Delete confirm state
  let deleteTarget = $state<Template | null>(null);
  let deleteError = $state<string | null>(null);

  const formValid = $derived(formName.trim().length > 0);

  function openCreate() {
    editingId = null;
    formName = '';
    formDescription = '';
    formStarterKit = 'none';
    formPhpVersion = '8.3';
    formServices = {
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
    formError = null;
    formOpen = true;
  }

  function openEdit(template: Template) {
    editingId = template.id;
    formName = template.name;
    formDescription = template.description;
    formStarterKit = template.starterKit;
    formPhpVersion = (template.phpVersion as PhpVersion) ?? '8.3';
    const has = (k: ServiceKind) => template.services.includes(k);
    formServices = {
      mysql: has('mysql'),
      pgsql: has('pgsql'),
      mariadb: has('mariadb'),
      redis: has('redis'),
      valkey: has('valkey'),
      memcached: has('memcached'),
      mailpit: has('mailpit'),
      meilisearch: has('meilisearch'),
      typesense: has('typesense'),
      mongodb: has('mongodb'),
      minio: has('minio'),
      selenium: has('selenium'),
      soketi: has('soketi'),
    };
    formError = null;
    formOpen = true;
  }

  function closeForm() {
    if (saving) return;
    formOpen = false;
  }

  async function submitForm(e: SubmitEvent) {
    e.preventDefault();
    if (!formValid || saving) return;
    saving = true;
    formError = null;
    const selectedServices = (Object.keys(formServices) as ServiceKind[]).filter(
      (s) => formServices[s],
    );
    const input: TemplateInput = {
      name: formName.trim(),
      description: formDescription.trim(),
      services: selectedServices,
      phpVersion: formPhpVersion,
      starterKit: formStarterKit,
    };
    try {
      if (editingId) {
        await projectStore.updateTemplate(editingId, input);
      } else {
        await projectStore.createTemplate(input);
      }
      formOpen = false;
    } catch (err) {
      formError = String(err);
    } finally {
      saving = false;
    }
  }

  function useTemplate(template: Template) {
    ui.pendingTemplate = template;
    ui.showCreateModal = true;
  }

  function askDelete(template: Template) {
    deleteTarget = template;
    deleteError = null;
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    try {
      await projectStore.deleteTemplate(deleteTarget.id);
      deleteTarget = null;
    } catch (err) {
      deleteError = String(err);
    }
  }

  function onFormKey(e: KeyboardEvent) {
    if (e.key === 'Escape') closeForm();
  }

  function starterKitLabel(kit: StarterKit): string {
    switch (kit) {
      case 'breeze':
        return 'Breeze';
      case 'jetstream':
        return 'Jetstream';
      default:
        return 'No starter kit';
    }
  }
</script>

<header class="page-header" data-tauri-drag-region>
  <div class="title-block" data-tauri-drag-region>
    <h1>Templates</h1>
    <span class="count">{templates.length}</span>
  </div>

  <div class="header-actions">
    <button class="btn btn-primary" onclick={openCreate}>
      <Icon name="plus" size={13} />
      New Template
    </button>
  </div>
</header>

<section class="grid">
  {#each templates as template (template.id)}
    <div class="card">
      <div class="head">
        <h3 class="name">{template.name}</h3>
        {#if template.description}
          <p class="description">{template.description}</p>
        {/if}
      </div>

      <div class="chips">
        {#each template.services as svc (svc)}
          <span class="chip">{svc}</span>
        {:else}
          <span class="chip muted">no services</span>
        {/each}
      </div>

      <div class="meta">
        <span class="meta-item">PHP {template.phpVersion}</span>
        <span class="meta-dot">·</span>
        <span class="meta-item">{starterKitLabel(template.starterKit)}</span>
        <span class="meta-dot">·</span>
        <span class="meta-item">{template.services.length} services</span>
      </div>

      <div class="actions">
        <button class="btn btn-primary" onclick={() => useTemplate(template)}>
          <Icon name="plus" size={12} />
          Use
        </button>
        <button class="btn btn-ghost" onclick={() => openEdit(template)} title="Edit template">
          <Icon name="settings" size={13} />
        </button>
        <button class="btn btn-ghost danger" onclick={() => askDelete(template)} title="Delete template">
          <Icon name="trash" size={13} />
        </button>
      </div>
    </div>
  {/each}

  {#if templates.length === 0}
    <div class="empty">
      <Icon name="box" size={28} />
      <p class="empty-title">No templates yet.</p>
      <p class="empty-sub">Create one to speed up new projects.</p>
      <button class="btn btn-primary" onclick={openCreate}>
        <Icon name="plus" size={13} />
        New Template
      </button>
    </div>
  {/if}
</section>

{#if formOpen}
  <div class="backdrop" onclick={closeForm} role="presentation"></div>

  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="tpl-modal-title"
    tabindex="-1"
    onkeydown={onFormKey}
  >
    <header class="modal-header">
      <h2 id="tpl-modal-title">{editingId ? 'Edit template' : 'New template'}</h2>
      <button class="btn btn-ghost btn-icon" onclick={closeForm} aria-label="Close" disabled={saving}>
        <Icon name="x" size={14} />
      </button>
    </header>

    <form onsubmit={submitForm}>
      <div class="field">
        <label for="tpl-name">Template name</label>
        <input
          id="tpl-name"
          type="text"
          placeholder="e.g. Breeze + Meilisearch"
          bind:value={formName}
          autocomplete="off"
          spellcheck="false"
        />
      </div>

      <div class="field">
        <label for="tpl-desc">Description</label>
        <input
          id="tpl-desc"
          type="text"
          placeholder="Optional — what's this preset for?"
          bind:value={formDescription}
          autocomplete="off"
        />
      </div>

      <div class="row">
        <div class="field">
          <label for="tpl-kit">Starter kit</label>
          <select id="tpl-kit" bind:value={formStarterKit}>
            <option value="none">None (plain Laravel)</option>
            <option value="breeze">Breeze</option>
            <option value="jetstream">Jetstream</option>
          </select>
        </div>

        <div class="field">
          <label for="tpl-php">PHP version</label>
          <select id="tpl-php" bind:value={formPhpVersion}>
            <option value="8.2">8.2</option>
            <option value="8.3">8.3</option>
            <option value="8.4">8.4</option>
          </select>
        </div>
      </div>

      <div class="field">
        <span class="label">Services</span>
        <div class="services">
          {#each serviceOptions as svc (svc.key)}
            <label class="service" class:checked={formServices[svc.key]}>
              <input type="checkbox" bind:checked={formServices[svc.key]} />
              <span class="service-label">{svc.label}</span>
            </label>
          {/each}
        </div>
      </div>

      {#if formError}
        <p class="form-error">{formError}</p>
      {/if}

      <footer class="modal-footer">
        <button type="button" class="btn btn-ghost" onclick={closeForm} disabled={saving}>Cancel</button>
        <button type="submit" class="btn btn-primary" disabled={!formValid || saving}>
          {#if saving}
            <span class="spinner"></span>
            Saving…
          {:else}
            {editingId ? 'Save changes' : 'Create template'}
          {/if}
        </button>
      </footer>
    </form>
  </div>
{/if}

<ConfirmModal
  open={deleteTarget !== null}
  title="Delete template"
  message={deleteTarget ? `Delete "${deleteTarget.name}"?` : ''}
  detail={deleteError ?? 'This only removes the template preset. Existing projects are not affected.'}
  confirmLabel="Delete"
  danger={true}
  onConfirm={confirmDelete}
  onCancel={() => (deleteTarget = null)}
/>

<style>
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 22px 28px 18px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: linear-gradient(180deg, var(--bg-1) 0%, var(--bg) 100%);
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
  .count {
    padding: 2px 9px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .header-actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 14px;
    padding: 22px 28px 28px;
    max-width: 1100px;
    margin: 0 auto;
    width: 100%;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-card);
    transition: border-color 0.15s var(--ease-quick), background 0.15s var(--ease-quick),
      transform 0.08s var(--ease-quick);
  }
  .card:hover {
    border-color: var(--border-strong);
    background: var(--bg-3);
    transform: translateY(-1px);
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .name {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .description {
    margin: 0;
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.45;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    font-size: 11px;
    color: var(--text);
    text-transform: capitalize;
  }
  .chip.muted {
    color: var(--text-faint);
    font-style: italic;
    text-transform: none;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-faint);
  }
  .meta-dot {
    color: var(--text-faint);
  }

  .actions {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }
  .actions .btn:first-child {
    flex: 1;
    justify-content: center;
  }
  .actions .btn.danger:hover {
    color: var(--error);
  }

  .empty {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 72px 24px;
    margin-top: 24px;
    color: var(--text-dim);
    text-align: center;
    background: linear-gradient(180deg, var(--bg-2) 0%, transparent 100%);
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
  }
  .empty :global(svg:first-of-type) {
    color: var(--accent);
    opacity: 0.7;
    margin-bottom: 4px;
  }
  .empty-title {
    margin: 8px 0 0;
    font-size: 17px;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--text);
  }
  .empty-sub {
    margin: 0 0 12px;
    font-size: 12.5px;
    max-width: 380px;
    line-height: 1.5;
  }

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
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    z-index: 51;
    animation: pop 0.18s ease;
  }
  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }
  .modal-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  form {
    padding: 18px 20px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
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
  .services {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .service {
    display: flex;
    align-items: center;
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
    accent-color: var(--accent);
  }
  .service-label {
    font-size: 12.5px;
    font-weight: 500;
  }
  .form-error {
    margin: 0;
    padding: 10px 12px;
    background: var(--error-soft);
    border: 1px solid var(--error);
    border-radius: 6px;
    color: var(--error);
    font-size: 12px;
  }
  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    border-top: 1px solid var(--border);
    margin: 4px -20px -20px;
    padding: 12px 20px;
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
