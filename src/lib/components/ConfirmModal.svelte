<script lang="ts">
  import Icon from './Icon.svelte';

  interface Props {
    open: boolean;
    title: string;
    message: string;
    detail?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onConfirm: () => void | Promise<void>;
    onCancel: () => void;
  }

  let {
    open,
    title,
    message,
    detail,
    confirmLabel = 'Confirm',
    cancelLabel = 'Cancel',
    danger = false,
    onConfirm,
    onCancel,
  }: Props = $props();

  let busy = $state(false);
  let modalEl = $state<HTMLElement | null>(null);
  let cancelBtn = $state<HTMLButtonElement | null>(null);

  // Focus the SAFE (Cancel) control when the dialog opens: Escape/Tab work
  // immediately, and a reflexive Enter lands on Cancel rather than confirming a
  // destructive action. (The old code bound Enter to confirm globally.)
  $effect(() => {
    if (open && cancelBtn) cancelBtn.focus();
  });

  async function handleConfirm() {
    if (busy) return;
    busy = true;
    try {
      await onConfirm();
    } finally {
      busy = false;
    }
  }

  function handleCancel() {
    if (busy) return;
    onCancel();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      handleCancel();
      return;
    }
    // Trap focus inside the dialog so Tab can't reach the obscured background.
    if (e.key === 'Tab' && modalEl) {
      const focusable = Array.from(
        modalEl.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

{#if open}
  <div class="backdrop" onclick={handleCancel} role="presentation"></div>

  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title" tabindex="-1" onkeydown={onKey} bind:this={modalEl}>
    <header>
      <h2 id="confirm-title">{title}</h2>
      <button class="btn btn-ghost btn-icon" onclick={handleCancel} aria-label="Close" disabled={busy}>
        <Icon name="x" size={14} />
      </button>
    </header>

    <div class="body">
      <p class="message">{message}</p>
      {#if detail}
        <p class="detail">{detail}</p>
      {/if}
    </div>

    <footer>
      <button type="button" class="btn btn-ghost" onclick={handleCancel} disabled={busy} bind:this={cancelBtn}>{cancelLabel}</button>
      <button
        type="button"
        class="btn"
        class:btn-primary={!danger}
        class:btn-confirm-danger={danger}
        onclick={handleConfirm}
        disabled={busy}
      >
        {#if busy}
          <span class="spinner"></span>
          Working…
        {:else}
          {confirmLabel}
        {/if}
      </button>
    </footer>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(2px);
    z-index: 60;
    animation: fade 0.15s ease;
  }
  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(440px, calc(100vw - 32px));
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    z-index: 61;
    animation: pop 0.18s ease;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }
  header h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .body {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .message {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
  }
  .detail {
    margin: 0;
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.5;
    word-break: break-all;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 18px;
    border-top: 1px solid var(--border);
  }
  .btn-confirm-danger {
    background: var(--error);
    border-color: var(--error);
    color: white;
  }
  .btn-confirm-danger:hover {
    background: #dc2626;
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
