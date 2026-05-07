import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toast } from './toast.svelte';
import type {
  Project,
  ProjectStatus,
  ServiceKind,
  StarterKit,
  PhpVersion,
  Settings,
  Template,
  TemplateInput,
  CloneProjectInput,
  HistoryEntry,
  AutoCommand,
  AutoCommandInput,
  LocalUrlsHealth,
  ContainerStat,
  GitStatus,
  OrphanCandidate,
  ProjectStatsSummary,
  DockerSystemInfo,
  DependencyCheck,
  ToolStatus,
} from './types';

interface CreateProjectInput {
  name: string;
  starterKit: StarterKit;
  phpVersion: PhpVersion;
  services: ServiceKind[];
  customServices: string[];
}

interface StatusChange {
  projectId: string;
  status: ProjectStatus;
}

interface ProcessOutput {
  projectId: string;
  stream: 'stdout' | 'stderr';
  line: string;
}

interface LogLine {
  projectId: string;
  stream: 'stdout' | 'stderr';
  line: string;
}

interface AutoCommandOutputLine {
  projectId: string;
  commandId: string;
  label: string;
  stream: 'stdout' | 'stderr';
  line: string;
}

interface OneShotOutputLine {
  projectId: string;
  stream: 'stdout' | 'stderr';
  line: string;
}

export interface OneShotEntry {
  stream: 'stdout' | 'stderr';
  line: string;
  at: number;
}

export interface AutoLogEntry {
  stream: 'stdout' | 'stderr';
  line: string;
  at: number;
}

export interface AutoLogStream {
  commandId: string;
  label: string;
  entries: AutoLogEntry[];
  /** Bumps any time the entries list grows so consumers can react. */
  updatedAt: number;
}

interface EnvCheck {
  dockerOk: boolean;
  dockerError: string | null;
  projectsRoot: string;
}

class ProjectStore {
  projects = $state<Project[]>([]);
  search = $state('');
  loading = $state(false);
  /** Set true once `init()` has resolved (success or error). Splash uses this. */
  booted = $state(false);
  /** Coarse-grained boot phase for the splash screen status line. */
  bootPhase = $state<'docker' | 'projects' | 'ready' | null>(null);
  error = $state<string | null>(null);
  envCheck = $state<EnvCheck | null>(null);
  createOutput = $state<string[]>([]);
  settings = $state<Settings | null>(null);
  togglingLocalUrls = $state(false);
  localUrlsHealth = $state<LocalUrlsHealth | null>(null);
  liveLogs = $state<{ id: string; lines: string[]; service: string | null } | null>(null);
  autoLogs = $state<{ projectId: string; streams: AutoLogStream[] } | null>(null);
  oneShotLogs = $state<{ projectId: string; entries: OneShotEntry[]; running: boolean } | null>(null);
  templates = $state<Template[]>([]);
  /** Per-project git status, keyed by project id. `null` means "not a git repo". */
  gitStatuses = $state<Record<string, GitStatus | null>>({});
  /** Per-Compose-project live stats (CPU, RAM). Keyed by composeProjectName. */
  liveStats = $state<Record<string, ProjectStatsSummary>>({});
  /** Daemon-wide stats — drives the system panel at the top of the list. */
  dockerSystem = $state<DockerSystemInfo | null>(null);

  filtered = $derived(
    this.search.trim() === ''
      ? this.projects
      : this.projects.filter((p) => p.name.toLowerCase().includes(this.search.toLowerCase())),
  );

  runningCount = $derived(this.projects.filter((p) => p.status === 'running').length);

  private statusUnlisten: UnlistenFn | null = null;
  private outputUnlisten: UnlistenFn | null = null;
  private logUnlisten: UnlistenFn | null = null;
  private autoOutputUnlisten: UnlistenFn | null = null;
  private oneShotUnlisten: UnlistenFn | null = null;

  async init() {
    this.loading = true;
    this.error = null;
    this.bootPhase = 'docker';
    try {
      this.envCheck = await invoke<EnvCheck>('check_environment');
      this.bootPhase = 'projects';
      this.projects = await invoke<Project[]>('list_projects');
      this.settings = await invoke<Settings>('get_settings');
      this.templates = await invoke<Template[]>('list_templates');
      await this.subscribe();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
      this.bootPhase = 'ready';
      this.booted = true;
    }
  }

  localUrlFor(project: Project): string | null {
    if (!this.settings?.localUrlsEnabled) return null;
    const scheme = this.settings.localUrlsHttps ? 'https' : 'http';
    return `${scheme}://${project.name}.${this.settings.localUrlTld}`;
  }

  async setLocalUrlsEnabled(enabled: boolean) {
    this.togglingLocalUrls = true;
    this.error = null;
    try {
      this.settings = await invoke<Settings>('set_local_urls_enabled', { enabled });
      await this.checkLocalUrlsHealth();
    } catch (e) {
      this.error = String(e);
      throw e;
    } finally {
      this.togglingLocalUrls = false;
    }
  }

  togglingLocalUrlsHttps = $state(false);
  async setLocalUrlsHttps(enabled: boolean) {
    this.togglingLocalUrlsHttps = true;
    this.error = null;
    try {
      this.settings = await invoke<Settings>('set_local_urls_https', { enabled });
      await this.checkLocalUrlsHealth();
    } catch (e) {
      this.error = String(e);
      throw e;
    } finally {
      this.togglingLocalUrlsHttps = false;
    }
  }

  async setLocalUrlTld(tld: string) {
    this.togglingLocalUrls = true;
    this.error = null;
    try {
      this.settings = await invoke<Settings>('set_local_url_tld', { tld });
      await this.checkLocalUrlsHealth();
    } catch (e) {
      this.error = String(e);
      throw e;
    } finally {
      this.togglingLocalUrls = false;
    }
  }

  async resyncLocalUrls() {
    try {
      await invoke('resync_local_urls');
      await this.checkLocalUrlsHealth();
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async checkLocalUrlsHealth(): Promise<LocalUrlsHealth> {
    const h = await invoke<LocalUrlsHealth>('check_local_urls');
    this.localUrlsHealth = h;
    return h;
  }

  /**
   * Best-effort silent recovery on app boot. Brings up Traefik + dnsmasq if
   * they're not running. Returns the post-recovery health so the layout can
   * decide whether to surface a "Fix it" toast.
   */
  async tryQuietRepair(): Promise<LocalUrlsHealth> {
    try {
      const h = await invoke<LocalUrlsHealth>('repair_local_urls_quiet');
      this.localUrlsHealth = h;
      return h;
    } catch (e) {
      // Don't blow up startup — surface in health if possible.
      const fallback = await this.checkLocalUrlsHealth();
      return fallback;
    }
  }

  async setEditor(editor: string) {
    try {
      this.settings = await invoke<Settings>('set_editor', { editor });
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async subscribe() {
    this.statusUnlisten?.();
    this.outputUnlisten?.();
    this.logUnlisten?.();

    this.statusUnlisten = await listen<StatusChange>('project-status-changed', (event) => {
      const { projectId, status } = event.payload;
      this.projects = this.projects.map((p) => (p.id === projectId ? { ...p, status } : p));
    });

    this.outputUnlisten = await listen<ProcessOutput>('process-output', (event) => {
      const line = event.payload.line;
      this.createOutput = [...this.createOutput.slice(-200), line];
    });

    this.logUnlisten = await listen<LogLine>('project-log', (event) => {
      const { projectId, line } = event.payload;
      const current = this.liveLogs;
      if (!current || current.id !== projectId) return;
      const next =
        current.lines.length >= 2000
          ? [...current.lines.slice(-1999), line]
          : [...current.lines, line];
      this.liveLogs = { id: current.id, lines: next, service: current.service };
    });

    this.autoOutputUnlisten = await listen<AutoCommandOutputLine>(
      'auto-command-output',
      (event) => {
        const p = event.payload;
        const now = Date.now();
        const entry: AutoLogEntry = { stream: p.stream, line: p.line, at: now };

        const current = this.autoLogs;
        // New project or first event since reset → start a fresh log group.
        if (!current || current.projectId !== p.projectId) {
          this.autoLogs = {
            projectId: p.projectId,
            streams: [
              { commandId: p.commandId, label: p.label, entries: [entry], updatedAt: now },
            ],
          };
          return;
        }

        const existing = current.streams.find((s) => s.commandId === p.commandId);
        if (existing) {
          const entries =
            existing.entries.length >= 800
              ? [...existing.entries.slice(-799), entry]
              : [...existing.entries, entry];
          const updated: AutoLogStream = {
            commandId: existing.commandId,
            label: existing.label,
            entries,
            updatedAt: now,
          };
          this.autoLogs = {
            projectId: current.projectId,
            streams: current.streams.map((s) => (s.commandId === p.commandId ? updated : s)),
          };
        } else {
          this.autoLogs = {
            projectId: current.projectId,
            streams: [
              ...current.streams,
              { commandId: p.commandId, label: p.label, entries: [entry], updatedAt: now },
            ],
          };
        }
      },
    );

    this.oneShotUnlisten = await listen<OneShotOutputLine>('one-shot-output', (event) => {
      const p = event.payload;
      const now = Date.now();
      const entry: OneShotEntry = { stream: p.stream, line: p.line, at: now };
      const current = this.oneShotLogs;

      // Synthetic terminator emitted by the backend when the child exits or
      // the wait fails. Mark `running: false` and append the line.
      const isTerminator =
        /^\[exit\b/.test(p.line) || /^\[wait failed\b/.test(p.line);

      if (!current || current.projectId !== p.projectId) {
        this.oneShotLogs = {
          projectId: p.projectId,
          entries: [entry],
          running: !isTerminator,
        };
        return;
      }

      const entries =
        current.entries.length >= 1000
          ? [...current.entries.slice(-999), entry]
          : [...current.entries, entry];
      this.oneShotLogs = {
        projectId: current.projectId,
        entries,
        running: isTerminator ? false : current.running,
      };
    });
  }

  resetAutoLogs(projectId: string) {
    this.autoLogs = { projectId, streams: [] };
  }

  async startLogStream(id: string, service: string | null = null) {
    this.liveLogs = { id, lines: [], service };
    try {
      await invoke('start_log_stream', { id, service });
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async stopLogStream(id: string) {
    try {
      await invoke('stop_log_stream', { id });
    } catch (e) {
      this.error = String(e);
    } finally {
      if (this.liveLogs?.id === id) this.liveLogs = null;
    }
  }

  /** Stop, then re-start the log stream filtered to a specific service (or all). */
  async setLogFilter(id: string, service: string | null) {
    await this.stopLogStream(id);
    await this.startLogStream(id, service);
  }

  async listComposeServices(id: string): Promise<string[]> {
    try {
      return await invoke<string[]>('list_compose_services', { id });
    } catch (e) {
      // Soft fail: if compose isn't usable yet, fall back to an empty list so
      // the dropdown still renders with just "All services".
      return [];
    }
  }

  async runOneShot(id: string, command: string) {
    // Reset and mark running before invoke so the UI updates immediately, even
    // if the first event lands quickly.
    this.oneShotLogs = { projectId: id, entries: [], running: true };
    try {
      await invoke('run_one_shot', { id, command });
    } catch (e) {
      this.oneShotLogs = {
        projectId: id,
        entries: [
          {
            stream: 'stderr',
            line: String(e),
            at: Date.now(),
          },
        ],
        running: false,
      };
      this.error = String(e);
      throw e;
    }
  }

  async stopOneShot(id: string) {
    try {
      await invoke('stop_one_shot', { id });
    } catch (e) {
      this.error = String(e);
    }
  }

  clearOneShotLog(id: string) {
    this.oneShotLogs = { projectId: id, entries: [], running: false };
  }

  async discoverOrphans(): Promise<OrphanCandidate[]> {
    try {
      return await invoke<OrphanCandidate[]>('discover_orphans');
    } catch {
      return [];
    }
  }

  async importOrphans(orphans: OrphanCandidate[]): Promise<{ imported: number; failed: number }> {
    let imported = 0;
    let failed = 0;
    for (const o of orphans) {
      try {
        await this.importProject(o.path);
        imported++;
      } catch {
        failed++;
      }
    }
    return { imported, failed };
  }

  async importProject(path: string): Promise<Project> {
    try {
      const project = await invoke<Project>('import_project', { path });
      this.projects = [project, ...this.projects];
      return project;
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async cloneProject(input: CloneProjectInput): Promise<Project> {
    this.createOutput = [];
    try {
      const project = await invoke<Project>('clone_project', { input });
      this.projects = [project, ...this.projects];
      return project;
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async listTemplates() {
    this.templates = await invoke<Template[]>('list_templates');
  }

  async createTemplate(input: TemplateInput): Promise<Template> {
    const t = await invoke<Template>('create_template', { input });
    this.templates = [...this.templates, t];
    return t;
  }

  async updateTemplate(id: string, input: TemplateInput): Promise<Template> {
    const t = await invoke<Template>('update_template', { id, input });
    this.templates = this.templates.map((x) => (x.id === id ? t : x));
    return t;
  }

  async deleteTemplate(id: string): Promise<void> {
    await invoke('delete_template', { id });
    this.templates = this.templates.filter((x) => x.id !== id);
  }

  async completeFirstRun() {
    try {
      this.settings = await invoke<Settings>('complete_first_run');
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async setTheme(theme: 'system' | 'dark' | 'light') {
    try {
      this.settings = await invoke<Settings>('set_theme', { theme });
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async setProjectsRoot(path: string): Promise<void> {
    try {
      this.envCheck = await invoke<EnvCheck>('set_projects_root', { path });
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async checkDependencies(): Promise<ToolStatus[]> {
    try {
      const r = await invoke<DependencyCheck>('check_dependencies');
      return r.tools;
    } catch (e) {
      this.error = String(e);
      return [];
    }
  }

  async refreshEnvCheck() {
    try {
      this.envCheck = await invoke<EnvCheck>('check_environment');
    } catch {
      // Soft failure — keep last known state.
    }
  }

  startingDocker = $state(false);
  async startDockerDesktop() {
    if (this.startingDocker) return;
    this.startingDocker = true;
    try {
      await invoke('start_docker_desktop');
      // Poll faster for ~60s after the launch attempt.
      const t0 = Date.now();
      while (Date.now() - t0 < 60_000) {
        await new Promise((r) => setTimeout(r, 1500));
        await this.refreshEnvCheck();
        if (this.envCheck?.dockerOk) break;
      }
    } catch (e) {
      this.error = String(e);
    } finally {
      this.startingDocker = false;
    }
  }

  async listHistory(id: string, limit?: number): Promise<HistoryEntry[]> {
    return await invoke<HistoryEntry[]>('list_history', { id, limit });
  }

  async listAutoCommands(id: string): Promise<AutoCommand[]> {
    return await invoke<AutoCommand[]>('list_auto_commands', { id });
  }

  async upsertAutoCommand(input: AutoCommandInput): Promise<AutoCommand> {
    return await invoke<AutoCommand>('upsert_auto_command', { input });
  }

  async deleteAutoCommand(id: string): Promise<void> {
    await invoke('delete_auto_command', { id });
  }

  async runAutoCommandsNow(id: string): Promise<void> {
    await invoke('run_auto_commands_now', { id });
  }

  async getProjectStats(id: string): Promise<ContainerStat[]> {
    return await invoke<ContainerStat[]>('get_project_stats', { id });
  }

  async refreshLiveStats() {
    try {
      const map = await invoke<Record<string, ProjectStatsSummary>>(
        'get_all_running_stats',
      );
      this.liveStats = map;
    } catch {
      // Silent — likely Docker is down. Sidebar already conveys that.
    }
  }

  async refreshDockerSystem() {
    try {
      this.dockerSystem = await invoke<DockerSystemInfo>('get_docker_system_info');
    } catch {
      this.dockerSystem = null;
    }
  }

  /** Helper: lookup live stats for a project by Compose project name. */
  statsFor(project: Project): ProjectStatsSummary | null {
    return this.liveStats[project.composeProjectName] ?? null;
  }

  async loadGitStatus(id: string): Promise<GitStatus | null> {
    const project = this.byId(id);
    if (!project) return null;
    try {
      const result = await invoke<GitStatus | null>('get_git_status', {
        path: project.path,
      });
      this.gitStatuses = { ...this.gitStatuses, [id]: result };
      return result;
    } catch {
      // Treat any failure as "not a repo" so we don't toast on every refresh.
      this.gitStatuses = { ...this.gitStatuses, [id]: null };
      return null;
    }
  }

  async loadAllGitStatuses(): Promise<void> {
    await Promise.all(this.projects.map((p) => this.loadGitStatus(p.id)));
  }

  async resetApplication() {
    await invoke('reset_application');
    // Re-fetch everything from disk so the UI mirrors the wiped state.
    this.projects = [];
    this.templates = [];
    this.liveLogs = null;
    this.autoLogs = null;
    this.localUrlsHealth = null;
    this.settings = await invoke<Settings>('get_settings');
    this.envCheck = await invoke<EnvCheck>('check_environment');
  }

  async startAll() {
    const targets = this.projects.filter(
      (p) => p.status === 'stopped' || p.status === 'error',
    );
    for (const p of targets) {
      try {
        await this.start(p.id);
      } catch {
        // continue with the next one
      }
    }
  }

  async stopAll() {
    const targets = this.projects.filter(
      (p) => p.status === 'running' || p.status === 'error',
    );
    for (const p of targets) {
      try {
        await this.stop(p.id);
      } catch {
        // continue
      }
    }
  }

  byId(id: string): Project | undefined {
    return this.projects.find((p) => p.id === id);
  }

  async refresh() {
    this.projects = await invoke<Project[]>('list_projects');
  }

  async start(id: string) {
    try {
      await invoke('start_project', { id });
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async stop(id: string) {
    try {
      await invoke('stop_project', { id });
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async create(input: CreateProjectInput): Promise<Project> {
    this.createOutput = [];
    try {
      const project = await invoke<Project>('create_project', { input });
      this.projects = [project, ...this.projects];
      return project;
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async remove(id: string, alsoRemoveFiles = false) {
    try {
      await invoke('delete_project', { id, alsoRemoveFiles });
      this.projects = this.projects.filter((p) => p.id !== id);
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  clearError() {
    this.error = null;
  }

  /**
   * Set the error field AND surface it as a toast. Call sites can keep using
   * `projectStore.error = '...'` without losing toast feedback because the
   * setter on the auto-property still runs reactively, but using this helper
   * is preferred so the toast title stays consistent.
   */
  reportError(message: string, title = 'Something went wrong') {
    this.error = message;
    toast.error(message, title);
  }
}

export const projectStore = new ProjectStore();
