<script lang="ts">
  import { ui } from '$lib/uiState.svelte';
  import Icon from './Icon.svelte';

  let open = $state(false);
  let containerEl: HTMLDivElement | undefined = $state();

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
  }

  function close() {
    open = false;
  }

  function pick(action: 'create' | 'import' | 'clone') {
    close();
    if (action === 'create') ui.showCreateModal = true;
    else if (action === 'import') ui.showImportModal = true;
    else ui.showCloneModal = true;
  }

  $effect(() => {
    if (!open) return;
    const onDoc = (ev: MouseEvent) => {
      if (containerEl && !containerEl.contains(ev.target as Node)) {
        close();
      }
    };
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') close();
    };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDoc);
      document.removeEventListener('keydown', onKey);
    };
  });
</script>

<div class="menu-wrap" bind:this={containerEl}>
  <button class="trigger btn btn-primary" onclick={toggle} aria-haspopup="menu" aria-expanded={open}>
    <Icon name="plus" size={14} />
    New Project
    <span class="caret" class:open></span>
  </button>

  {#if open}
    <div class="menu" role="menu">
      <button class="item" role="menuitem" onclick={() => pick('create')}>
        <Icon name="plus" size={13} />
        <div class="item-text">
          <div class="title">Create new</div>
          <div class="sub">Scaffold a fresh Laravel + Sail project</div>
        </div>
      </button>
      <button class="item" role="menuitem" onclick={() => pick('clone')}>
        <Icon name="external" size={13} />
        <div class="item-text">
          <div class="title">Clone from Git</div>
          <div class="sub">Clone a repo, install Sail if missing</div>
        </div>
      </button>
      <button class="item" role="menuitem" onclick={() => pick('import')}>
        <Icon name="folder" size={13} />
        <div class="item-text">
          <div class="title">Import existing</div>
          <div class="sub">Add a Sail project that's already on disk</div>
        </div>
      </button>
    </div>
  {/if}
</div>

<style>
  .menu-wrap {
    position: relative;
  }
  .trigger {
    width: 100%;
    justify-content: center;
    padding: 8px 10px;
    font-size: 12px;
    font-weight: 600;
    gap: 6px;
  }
  .caret {
    width: 0;
    height: 0;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-top: 5px solid currentColor;
    margin-left: 4px;
    transition: transform 0.15s;
  }
  .caret.open {
    transform: rotate(180deg);
  }
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    background: var(--bg-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    padding: 4px;
    z-index: 30;
    animation: pop 0.14s ease;
  }
  .item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text);
    transition: background 0.1s;
  }
  .item:hover {
    background: var(--bg-4);
  }
  .item :global(svg) {
    margin-top: 2px;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .item-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    line-height: 1.3;
  }
  .title {
    font-size: 12.5px;
    font-weight: 500;
  }
  .sub {
    font-size: 11px;
    color: var(--text-dim);
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
