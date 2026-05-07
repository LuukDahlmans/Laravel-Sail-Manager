<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onDestroy } from 'svelte';

  type Props = {
    projectId: string;
    active: boolean;
  };

  const { projectId, active }: Props = $props();

  let host: HTMLDivElement | null = $state(null);
  let term: import('@xterm/xterm').Terminal | null = null;
  let fit: import('@xterm/addon-fit').FitAddon | null = null;
  let unlisten: UnlistenFn | null = null;
  let resizeObs: ResizeObserver | null = null;
  let mounted = false;
  let mountToken = 0;

  // Track theme value so we can re-apply when it changes.
  let themeValue = $state(
    typeof document !== 'undefined' ? document.documentElement.dataset.theme ?? 'system' : 'system',
  );

  function readTheme() {
    if (typeof document === 'undefined') return null;
    const css = getComputedStyle(document.documentElement);
    const get = (name: string, fallback: string) => {
      const v = css.getPropertyValue(name).trim();
      return v.length > 0 ? v : fallback;
    };
    return {
      background: get('--code-bg', '#0b0e14'),
      foreground: get('--code-text', '#d6deeb'),
      cursor: get('--accent', '#5ccfe6'),
      cursorAccent: get('--code-bg', '#0b0e14'),
      selectionBackground: get('--accent-soft', 'rgba(92,207,230,0.3)'),
    };
  }

  async function mountTerminal() {
    if (mounted || !host) return;
    mounted = true;
    const token = ++mountToken;

    const [{ Terminal }, { FitAddon }] = await Promise.all([
      import('@xterm/xterm'),
      import('@xterm/addon-fit'),
    ]);
    // Bail if the component was torn down between awaits.
    if (token !== mountToken || !host) return;

    const t = new Terminal({
      cursorBlink: true,
      fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
      fontSize: 13,
      lineHeight: 1.25,
      scrollback: 5000,
      allowProposedApi: true,
      theme: readTheme() ?? undefined,
    });
    const f = new FitAddon();
    t.loadAddon(f);
    t.open(host);
    try {
      f.fit();
    } catch {
      // host may not have layout yet — fall back to defaults; ResizeObserver
      // will fit() once we get a real size.
    }
    term = t;
    fit = f;

    t.onData((data) => {
      invoke('send_shell_input', { id: projectId, data }).catch(() => {
        // Backend gone or session torn down — ignore. The component will
        // surface real failures via toast on start_shell.
      });
    });

    unlisten = await listen<{ projectId: string; data: string }>('shell-output', (event) => {
      if (event.payload.projectId !== projectId) return;
      term?.write(event.payload.data);
    });

    try {
      await invoke('start_shell', { id: projectId, cols: t.cols, rows: t.rows });
    } catch (e) {
      t.write(`\r\n\x1b[31m[shell] failed to start: ${String(e)}\x1b[0m\r\n`);
    }

    resizeObs = new ResizeObserver(() => {
      if (!fit || !term) return;
      try {
        fit.fit();
      } catch {
        return;
      }
      invoke('shell_resize', { id: projectId, cols: term.cols, rows: term.rows }).catch(() => {});
    });
    resizeObs.observe(host);
  }

  async function teardown() {
    mountToken++;
    mounted = false;
    if (resizeObs) {
      resizeObs.disconnect();
      resizeObs = null;
    }
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
    if (term) {
      term.dispose();
      term = null;
    }
    fit = null;
    try {
      await invoke('stop_shell', { id: projectId });
    } catch {
      // ignore
    }
  }

  $effect(() => {
    if (active && host) {
      mountTerminal();
    } else if (!active && mounted) {
      teardown();
    }
  });

  // Watch the documentElement's data-theme attribute so we can re-apply colors.
  $effect(() => {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    const obs = new MutationObserver(() => {
      themeValue = root.dataset.theme ?? 'system';
    });
    obs.observe(root, { attributes: true, attributeFilter: ['data-theme', 'class'] });
    return () => obs.disconnect();
  });

  $effect(() => {
    void themeValue;
    if (!term) return;
    const next = readTheme();
    if (next) term.options.theme = next;
  });

  onDestroy(() => {
    teardown();
  });
</script>

<div class="shell-wrap">
  <div class="shell-host" bind:this={host}></div>
  {#if !active}
    <div class="shell-idle">Switch to this tab to attach a shell.</div>
  {/if}
</div>

<style>
  .shell-wrap {
    position: relative;
    background: var(--code-bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    min-height: 360px;
    height: calc(100vh - 240px);
    max-height: 720px;
  }
  .shell-host {
    width: 100%;
    height: 100%;
    padding: 8px 10px;
    box-sizing: border-box;
  }
  :global(.shell-host .xterm) {
    height: 100%;
  }
  :global(.shell-host .xterm-viewport) {
    background: transparent !important;
  }
  .shell-idle {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-faint);
    font-size: 12px;
    pointer-events: none;
  }
</style>
