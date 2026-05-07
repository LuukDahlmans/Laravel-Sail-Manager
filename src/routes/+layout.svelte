<script lang="ts">
  import '../app.css';
  import { onMount, untrack } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import CreateProjectModal from '$lib/components/CreateProjectModal.svelte';
  import ImportProjectModal from '$lib/components/ImportProjectModal.svelte';
  import CloneFromGitModal from '$lib/components/CloneFromGitModal.svelte';
  import Toaster from '$lib/components/Toaster.svelte';
  import SplashScreen from '$lib/components/SplashScreen.svelte';
  import { projectStore } from '$lib/projects.svelte';
  import { toast } from '$lib/toast.svelte';

  // Splash gate. Show until init() resolves AND a minimum display time has
  // elapsed, so the splash doesn't strobe in/out on fast machines.
  let minDisplayElapsed = $state(false);
  $effect(() => {
    const t = window.setTimeout(() => {
      minDisplayElapsed = true;
    }, 600);
    return () => window.clearTimeout(t);
  });
  const showSplash = $derived(!(projectStore.booted && minDisplayElapsed));

  let { children } = $props();

  const onWelcome = $derived(page.url.pathname === '/welcome');

  // Bridge legacy projectStore.error → toast. Whenever the field changes to a
  // non-null value, fire a toast and immediately clear it so the next assignment
  // re-fires.
  let lastErrorSeen: string | null = $state(null);
  $effect(() => {
    const e = projectStore.error;
    if (e && e !== lastErrorSeen) {
      lastErrorSeen = e;
      toast.error(e);
      untrack(() => projectStore.clearError());
    }
    if (!e) lastErrorSeen = null;
  });

  onMount(() => {
    (async () => {
      await projectStore.init();
      // Self-heal local URLs: if the user enabled them previously but the
      // resolver, dnsmasq, or proxy is missing/down, attempt a silent repair
      // (containers only — no sudo). If still broken, surface a sticky toast
      // with a "Fix it" action that runs the full resync (which prompts for
      // admin password to write /etc/resolver/<tld>).
      try {
        const h = await projectStore.tryQuietRepair();
        if (h.enabled && !h.overallOk) {
          const detail = h.issues[0] ?? `.${h.tld} URLs aren't fully working.`;
          toast.show({
            type: 'warning',
            title: 'Local URLs need repair',
            message: detail,
            duration: 0,
            action: {
              label: 'Fix it',
              handler: async () => {
                try {
                  await projectStore.resyncLocalUrls();
                  toast.success('Local URLs are working');
                } catch (e) {
                  toast.error(`Repair failed: ${e}`);
                }
              },
            },
          });
        }
      } catch {
        // Health check failure is non-fatal.
      }

      // Orphan-discovery toast is gated on `firstRunCompleted` and lives in a
      // $effect below — that way it doesn't pile onto the welcome wizard, and
      // it still fires once the user finishes onboarding without needing a
      // relaunch.

      // Read the bundled app version (for the sidebar pill) and check whether
      // a newer release is available. Both write into projectStore so the
      // sidebar can render them. Silent failures inside checkForUpdate handle
      // "no release yet / no network / signature mismatch".
      await projectStore.loadAppVersion();
      projectStore.checkForUpdate();
    })();

    const unlistenPromise = listen<{ path: string }>('tray-navigate', (e) => {
      goto(e.payload.path);
    });
    // Poll Docker status so the sidebar reflects real-time changes (user
    // pausing/quitting Docker Desktop, etc.).
    const dockerInterval = window.setInterval(() => {
      projectStore.refreshEnvCheck();
    }, 5000);

    // Poll daemon-wide docker stats (CPU / RAM / disk) so the sidebar can
    // show them at all times, regardless of route. The list page used to
    // own this; now layout owns it so the sidebar is always live.
    const refreshStats = () => {
      if (projectStore.envCheck?.dockerOk) {
        projectStore.refreshDockerSystem();
        projectStore.refreshLiveStats();
      }
    };
    refreshStats();
    const statsInterval = window.setInterval(refreshStats, 8000);

    return () => {
      unlistenPromise.then((fn) => fn());
      window.clearInterval(dockerInterval);
      window.clearInterval(statsInterval);
    };
  });

  let redirected = $state(false);
  $effect(() => {
    if (redirected) return;
    const s = projectStore.settings;
    if (s && !s.firstRunCompleted && page.url.pathname !== '/welcome') {
      redirected = true;
      goto('/welcome');
    }
  });

  // Run the orphan-discovery toast at most once per session, AFTER first-run
  // is complete. Returning users hit this immediately on boot; new users hit
  // it the moment they finish the welcome wizard. Skipping during onboarding
  // keeps the wizard quiet.
  let orphanScanDone = $state(false);
  $effect(() => {
    if (orphanScanDone) return;
    if (!projectStore.settings?.firstRunCompleted) return;
    orphanScanDone = true;
    void (async () => {
      try {
        const orphans = await projectStore.discoverOrphans();
        if (orphans.length === 0) return;
        const noun = orphans.length === 1 ? 'project' : 'projects';
        toast.show({
          type: 'info',
          title: `Found ${orphans.length} untracked ${noun}`,
          message:
            orphans.map((o) => o.name).slice(0, 5).join(', ') +
            (orphans.length > 5 ? `, … (${orphans.length - 5} more)` : ''),
          duration: 0,
          action: {
            label: 'Import all',
            handler: async () => {
              const { imported, failed } = await projectStore.importOrphans(orphans);
              if (failed === 0) {
                toast.success(`Imported ${imported} ${noun}`);
              } else {
                toast.warning(
                  `Imported ${imported} of ${orphans.length}; ${failed} failed`,
                );
              }
            },
          },
        });
      } catch {
        // Best-effort: discovery failures are silent.
      }
    })();
  });

  // Apply theme: 'dark' / 'light' literal, or 'system' resolved at runtime.
  // Also tells the macOS native window to switch appearance so the traffic
  // light buttons stay visible (otherwise on light mode they can fade against
  // a near-white sidebar).
  $effect(() => {
    const choice = projectStore.settings?.theme ?? 'system';
    const apply = (effective: 'dark' | 'light') => {
      document.documentElement.dataset.theme = effective;
      // Best-effort: sync native window appearance with our CSS theme.
      import('@tauri-apps/api/window')
        .then((m) => m.getCurrentWindow().setTheme(effective))
        .catch(() => {});
    };
    if (choice === 'dark' || choice === 'light') {
      apply(choice);
      return;
    }
    const mql = window.matchMedia('(prefers-color-scheme: light)');
    apply(mql.matches ? 'light' : 'dark');
    const onChange = (e: MediaQueryListEvent) => apply(e.matches ? 'light' : 'dark');
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  });
</script>

{#if onWelcome}
  {@render children?.()}
{:else}
<div class="app">
  <Sidebar />
  <main class="content">
    {@render children?.()}
  </main>
</div>

<CreateProjectModal />
<ImportProjectModal />
<CloneFromGitModal />
{/if}

<Toaster />

{#if showSplash}
  <SplashScreen phase={projectStore.bootPhase} />
{/if}

<style>
  .app {
    display: flex;
    height: 100vh;
    width: 100vw;
    background: var(--bg);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
  }

</style>
