<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { toast, type Toast } from '$lib/toast.svelte';
  import Icon from './Icon.svelte';

  function iconFor(t: Toast['type']): string {
    switch (t) {
      case 'error':
        return 'x';
      case 'success':
        return 'play';
      case 'warning':
        return 'logs';
      default:
        return 'logs';
    }
  }
</script>

<div class="toaster" aria-live="polite" aria-atomic="false">
  {#each toast.toasts as t (t.id)}
    <div
      class="toast toast-{t.type}"
      role={t.type === 'error' ? 'alert' : 'status'}
      in:fly={{ x: 20, duration: 220, easing: cubicOut }}
      out:fade={{ duration: 160 }}
    >
      <div class="ic">
        <Icon name={iconFor(t.type)} size={13} />
      </div>
      <div class="body">
        {#if t.title}
          <div class="title">{t.title}</div>
        {/if}
        <div class="message selectable">{t.message}</div>
        {#if t.action}
          <button
            class="action"
            onclick={async () => {
              try {
                await t.action!.handler();
              } finally {
                toast.dismiss(t.id);
              }
            }}
          >
            {t.action.label}
          </button>
        {/if}
      </div>
      <button class="close" onclick={() => toast.dismiss(t.id)} aria-label="Dismiss">
        <Icon name="x" size={12} />
      </button>
      {#if t.duration > 0}
        <span class="bar" style="--duration: {t.duration}ms"></span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toaster {
    position: fixed;
    bottom: 16px;
    right: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 100;
    pointer-events: none;
    max-width: min(380px, calc(100vw - 32px));
  }

  .toast {
    pointer-events: auto;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: flex-start;
    gap: 10px;
    padding: 11px 12px 11px 14px;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-left-width: 3px;
    border-radius: var(--radius);
    box-shadow:
      0 10px 32px rgba(0, 0, 0, 0.22),
      0 1px 0 rgba(255, 255, 255, 0.05) inset;
    backdrop-filter: blur(12px) saturate(140%);
    -webkit-backdrop-filter: blur(12px) saturate(140%);
    overflow: hidden;
    position: relative;
  }

  .toast-error {
    border-left-color: var(--error);
  }
  .toast-error .ic {
    background: var(--error-soft);
    color: var(--error);
  }

  .toast-success {
    border-left-color: var(--success);
  }
  .toast-success .ic {
    background: var(--success-soft);
    color: var(--success);
  }

  .toast-warning {
    border-left-color: var(--warning);
  }
  .toast-warning .ic {
    background: var(--warning-soft);
    color: var(--warning);
  }

  .toast-info {
    border-left-color: var(--accent);
  }
  .toast-info .ic {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .ic {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    line-height: 1.4;
  }
  .title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text);
  }
  .message {
    font-size: 12px;
    color: var(--text-dim);
    word-break: break-word;
    max-height: 7em;
    overflow-y: auto;
  }

  .action {
    margin-top: 6px;
    align-self: flex-start;
    padding: 4px 10px;
    font-size: 11.5px;
    font-weight: 500;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--bg-3);
    color: var(--text);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .action:hover {
    background: var(--bg-4);
    border-color: var(--accent);
    color: var(--accent);
  }

  .close {
    background: transparent;
    border: none;
    color: var(--text-faint);
    width: 22px;
    height: 22px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
    flex-shrink: 0;
  }
  .close:hover {
    background: var(--bg-3);
    color: var(--text);
  }

  .bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    width: 100%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--text-faint) 50%,
      transparent 100%
    );
    opacity: 0.35;
    transform-origin: left;
    animation: shrink var(--duration) linear forwards;
  }
  .toast-error .bar {
    background: var(--error);
    opacity: 0.55;
  }
  .toast-success .bar {
    background: var(--success);
    opacity: 0.55;
  }
  .toast-warning .bar {
    background: var(--warning);
    opacity: 0.55;
  }
  .toast-info .bar {
    background: var(--accent);
    opacity: 0.55;
  }

  @keyframes shrink {
    from {
      transform: scaleX(1);
    }
    to {
      transform: scaleX(0);
    }
  }
</style>
