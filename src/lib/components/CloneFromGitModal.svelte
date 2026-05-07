<script lang="ts">
  import { ui } from '$lib/uiState.svelte';
  import { projectStore } from '$lib/projects.svelte';
  import type { PhpVersion } from '$lib/types';
  import Icon from './Icon.svelte';
  import { goto } from '$app/navigation';

  let urlInput: HTMLInputElement | undefined = $state();
  let url = $state('');
  let name = $state('');
  let branch = $state('');
  let phpVersion = $state<PhpVersion>('8.3');
  let showAdvanced = $state(false);
  let cloning = $state(false);
  let cloneError = $state<string | null>(null);
  let logEl: HTMLElement | undefined = $state();

  const valid = $derived(isValidGitUrl(url.trim()));

  function isValidGitUrl(value: string): boolean {
    if (!value) return false;
    if (/^https?:\/\/\S+/i.test(value)) return true;
    if (/^ssh:\/\/\S+/i.test(value)) return true;
    // git@host:owner/repo(.git)
    if (/^[^\s@]+@[^\s:]+:[^\s/][^\s]*$/.test(value)) return true;
    return false;
  }

  function close() {
    if (cloning) return;
    ui.showCloneModal = false;
    reset();
  }

  function reset() {
    url = '';
    name = '';
    branch = '';
    phpVersion = '8.3';
    showAdvanced = false;
    cloneError = null;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!valid || cloning) return;
    cloning = true;
    cloneError = null;
    try {
      const project = await projectStore.cloneProject({
        url: url.trim(),
        name: name.trim() || undefined,
        branch: branch.trim() || undefined,
        phpVersion,
      });
      cloning = false;
      ui.showCloneModal = false;
      reset();
      goto(`/projects/${project.id}`);
    } catch (e) {
      cloneError = String(e);
      cloning = false;
    }
  }

  $effect(() => {
    if (cloning && logEl && projectStore.createOutput.length > 0) {
      logEl.scrollTop = logEl.scrollHeight;
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }

  $effect(() => {
    if (ui.showCloneModal && urlInput) {
      urlInput.focus();
    }
  });
</script>

{#if ui.showCloneModal}
  <div class="backdrop" onclick={close} role="presentation"></div>

  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="clone-modal-title"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header>
      <h2 id="clone-modal-title">{cloning ? 'Cloning…' : 'Clone from Git'}</h2>
      <button
        class="btn btn-ghost btn-icon"
        onclick={close}
        aria-label="Close"
        disabled={cloning}
      >
        <Icon name="x" size={14} />
      </button>
    </header>

    {#if cloning}
      <div class="creating-body">
        <p class="hint">
          Running <code>git clone</code>, then <code>composer install</code> and
          <code>sail:install</code> if needed. Allocating ports and writing <code>.env</code>.
        </p>
        <div class="log-output" bind:this={logEl}>
          {#each projectStore.createOutput as line, i (i)}
            <div class="log-line">{line}</div>
          {:else}
            <div class="log-line dim">Starting…</div>
          {/each}
        </div>
      </div>
    {:else if cloneError}
      <div class="creating-body">
        <p class="error">Could not clone project.</p>
        <pre class="error-message selectable">{cloneError}</pre>
        {#if projectStore.createOutput.length > 0}
          <p class="hint">Last output:</p>
          <div class="log-output selectable">
            {#each projectStore.createOutput as line, i (i)}
              <div class="log-line">{line}</div>
            {/each}
          </div>
        {/if}
        <footer>
          <button type="button" class="btn btn-ghost" onclick={close}>Close</button>
          <button type="button" class="btn btn-primary" onclick={() => (cloneError = null)}>
            Try again
          </button>
        </footer>
      </div>
    {:else}
      <form onsubmit={submit}>
        <div class="field">
          <label for="git-url">Git URL</label>
          <input
            id="git-url"
            type="text"
            placeholder="https://github.com/owner/repo.git"
            bind:value={url}
            bind:this={urlInput}
            autocomplete="off"
            spellcheck="false"
            required
          />
          <p class="hint">HTTPS or SSH (<code>git@host:owner/repo.git</code>).</p>
        </div>

        <div class="field">
          <label for="php-version-clone">PHP version</label>
          <select id="php-version-clone" bind:value={phpVersion}>
            <option value="8.2">8.2</option>
            <option value="8.3">8.3</option>
            <option value="8.4">8.4</option>
          </select>
        </div>

        <button
          type="button"
          class="advanced-toggle"
          onclick={() => (showAdvanced = !showAdvanced)}
          aria-expanded={showAdvanced}
        >
          {showAdvanced ? '− Hide advanced' : '+ Advanced options'}
        </button>

        {#if showAdvanced}
          <div class="row">
            <div class="field">
              <label for="proj-name-clone">Project name</label>
              <input
                id="proj-name-clone"
                type="text"
                placeholder="auto from URL"
                bind:value={name}
                autocomplete="off"
                spellcheck="false"
              />
              <p class="hint">Folder name; defaults to repo name.</p>
            </div>

            <div class="field">
              <label for="branch-clone">Branch</label>
              <input
                id="branch-clone"
                type="text"
                placeholder="default"
                bind:value={branch}
                autocomplete="off"
                spellcheck="false"
              />
              <p class="hint">Leave empty for the default branch.</p>
            </div>
          </div>
        {/if}

        <footer>
          <button type="button" class="btn btn-ghost" onclick={close}>Cancel</button>
          <button type="submit" class="btn btn-primary" disabled={!valid}>Clone &amp; set up</button>
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
  .hint code,
  .creating-body code {
    background: var(--bg);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  }
  .advanced-toggle {
    align-self: flex-start;
    background: transparent;
    border: none;
    color: var(--text-dim);
    font-size: 11.5px;
    padding: 2px 0;
    cursor: pointer;
  }
  .advanced-toggle:hover {
    color: var(--text);
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
    from { opacity: 0; }
    to { opacity: 1; }
  }
  @keyframes pop {
    from { opacity: 0; transform: translate(-50%, -48%) scale(0.97); }
    to { opacity: 1; transform: translate(-50%, -50%) scale(1); }
  }
</style>
