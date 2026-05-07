<script lang="ts">
  import { ui } from '$lib/uiState.svelte';
  import { projectStore } from '$lib/projects.svelte';
  import Icon from './Icon.svelte';
  import { goto } from '$app/navigation';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';

  let pathInput: HTMLInputElement | undefined = $state();
  let path = $state('');
  let importing = $state(false);
  let importError = $state<string | null>(null);

  async function browseForFolder() {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: 'Select Sail project folder',
        defaultPath: projectStore.envCheck?.projectsRoot ?? undefined,
      });
      if (typeof selected === 'string' && selected.length > 0) {
        path = selected;
      }
    } catch (e) {
      importError = `Could not open folder picker: ${e}`;
    }
  }

  const valid = $derived(path.trim().length > 0);

  function close() {
    if (importing) return;
    ui.showImportModal = false;
    reset();
  }

  function reset() {
    path = '';
    importError = null;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!valid || importing) return;
    importing = true;
    importError = null;
    try {
      const project = await projectStore.importProject(path.trim());
      importing = false;
      ui.showImportModal = false;
      reset();
      goto(`/projects/${project.id}`);
    } catch (e) {
      importError = String(e);
      importing = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }

  $effect(() => {
    if (ui.showImportModal && pathInput) {
      pathInput.focus();
    }
  });
</script>

{#if ui.showImportModal}
  <div class="backdrop" onclick={close} role="presentation"></div>

  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="import-modal-title"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header>
      <h2 id="import-modal-title">
        {importing ? `Importing ${path}…` : 'Import existing Sail project'}
      </h2>
      <button class="btn btn-ghost btn-icon" onclick={close} aria-label="Close" disabled={importing}>
        <Icon name="x" size={14} />
      </button>
    </header>

    {#if importError}
      <div class="creating-body">
        <p class="error">Could not import project.</p>
        <pre class="error-message selectable">{importError}</pre>
        <footer>
          <button type="button" class="btn btn-ghost" onclick={close}>Close</button>
          <button type="button" class="btn btn-primary" onclick={() => (importError = null)}>
            Try again
          </button>
        </footer>
      </div>
    {:else}
      <form onsubmit={submit}>
        <div class="field">
          <label for="proj-path">Project folder</label>
          <div class="path-row">
            <input
              id="proj-path"
              type="text"
              placeholder="/Users/you/path/to/sail-project"
              bind:value={path}
              bind:this={pathInput}
              autocomplete="off"
              spellcheck="false"
              disabled={importing}
            />
            <button type="button" class="btn" onclick={browseForFolder} disabled={importing}>
              <Icon name="folder" size={13} />
              Browse…
            </button>
          </div>
          <p class="hint">
            Must contain a <code>compose.yaml</code> (or <code>docker-compose.yml</code>) and
            <code>.env</code>. Ports are read from <code>.env</code>.
          </p>
        </div>

        <footer>
          <button type="button" class="btn btn-ghost" onclick={close} disabled={importing}>
            Cancel
          </button>
          <button type="submit" class="btn btn-primary" disabled={!valid || importing}>
            {importing ? 'Importing…' : 'Import'}
          </button>
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
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    z-index: 51;
    animation: pop 0.18s ease;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }
  header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .path-row {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .path-row input {
    flex: 1;
    min-width: 0;
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
  .field label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-dim);
  }
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-faint);
  }
  .hint code {
    background: var(--bg);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 10.5px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
    margin: 4px -20px -20px;
    padding: 12px 20px;
  }

  .creating-body {
    padding: 18px 20px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .creating-body .error {
    margin: 0;
    color: var(--error);
    font-size: 13px;
    font-weight: 500;
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
