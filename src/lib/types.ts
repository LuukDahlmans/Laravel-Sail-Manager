export type ServiceKind =
  | 'mysql'
  | 'pgsql'
  | 'mariadb'
  | 'redis'
  | 'valkey'
  | 'memcached'
  | 'mailpit'
  | 'meilisearch'
  | 'typesense'
  | 'mongodb'
  | 'minio'
  | 'selenium'
  | 'soketi';

export type ProjectStatus = 'running' | 'stopped' | 'starting' | 'stopping' | 'error';

export type PortService =
  | 'app'
  | 'vite'
  | 'mysql'
  | 'redis'
  | 'mailpit_smtp'
  | 'mailpit_ui'
  | 'meilisearch'
  | 'minio';

export interface Port {
  service: PortService;
  label: string;
  host: number;
}

export type StarterKit = 'none' | 'breeze' | 'jetstream';
export type PhpVersion = '8.2' | '8.3' | '8.4';

export interface Project {
  id: string;
  name: string;
  composeProjectName: string;
  path: string;
  status: ProjectStatus;
  starterKit: StarterKit;
  phpVersion: PhpVersion;
  services: ServiceKind[];
  ports: Port[];
  createdAt: string;
  lastStarted?: string;
}

export type EditorChoice = '' | 'phpstorm' | 'vscode' | 'cursor' | 'zed';

export type ThemeChoice = 'system' | 'dark' | 'light';

export interface Settings {
  localUrlsEnabled: boolean;
  localUrlTld: string;
  proxyPort: number;
  editor: EditorChoice;
  firstRunCompleted: boolean;
  theme: ThemeChoice;
  projectsRoot: string;
  localUrlsHttps: boolean;
  dismissedSailImports: string[];
}

export type HistoryKind =
  | 'created'
  | 'started'
  | 'stopped'
  | 'errored'
  | 'imported'
  | 'cloned';

export interface HistoryEntry {
  id: number;
  projectId: string;
  kind: HistoryKind;
  detail: string | null;
  at: string;
}

export type AutoCommandRunMode = 'once' | 'service';

export interface AutoCommand {
  id: string;
  projectId: string;
  label: string;
  command: string;
  runMode: AutoCommandRunMode;
  enabled: boolean;
  sortOrder: number;
}

export interface AutoCommandInput {
  id?: string;
  projectId: string;
  label: string;
  command: string;
  runMode: AutoCommandRunMode;
  enabled: boolean;
  sortOrder: number;
}

export interface Template {
  id: string;
  name: string;
  description: string;
  services: ServiceKind[];
  phpVersion: string;
  starterKit: StarterKit;
  createdAt: string;
}

export interface TemplateInput {
  name: string;
  description: string;
  services: ServiceKind[];
  phpVersion: string;
  starterKit: StarterKit;
}

export interface CloneProjectInput {
  url: string;
  name?: string;
  branch?: string;
  phpVersion: PhpVersion;
}

export interface LocalUrlsHealth {
  enabled: boolean;
  tld: string;
  resolverOk: boolean;
  dnsmasqRunning: boolean;
  proxyRunning: boolean;
  proxyPortBound: boolean;
  dnsResolves: boolean;
  overallOk: boolean;
  issues: string[];
}

export interface ContainerStat {
  name: string;
  cpuPercent: string;
  memUsage: string;
  memPercent: string;
  netIo: string;
  blockIo: string;
  pids: number;
}

export interface GitStatus {
  branch: string;
  dirty: boolean;
  ahead: number;
  behind: number;
}

export interface OrphanCandidate {
  name: string;
  path: string;
  composeFile: string;
}

/** A Sail stack Docker knows about — running or stopped — that isn't tracked. */
export interface UntrackedSailProject {
  composeProject: string;
  name: string;
  path: string;
  services: string[];
  running: boolean;
  appPort: number | null;
  phpVersion: string | null;
  importable: boolean;
  blockedReason: string | null;
}

export interface AdoptOutcome {
  project: Project;
  /** .env keys we added because the project relied on compose defaults. */
  pinnedKeys: string[];
  /** Ports moved to dodge a conflict — the containers need a restart to match. */
  needsRestart: boolean;
}

export interface ProjectStatsSummary {
  composeProjectName: string;
  containerCount: number;
  cpuPercent: number;
  memUsedBytes: number;
  memLimitBytes: number;
}

export interface DockerSystemInfo {
  containersRunning: number;
  containersStopped: number;
  images: number;
  totalCpuPercent: number;
  memUsedBytes: number;
  memTotalBytes: number;
  diskImagesBytes: number;
  diskContainersBytes: number;
  diskVolumesBytes: number;
  diskCacheBytes: number;
}

export interface ToolStatus {
  id: string;
  label: string;
  purpose: string;
  required: boolean;
  installed: boolean;
  version: string | null;
  installUrl: string;
}

export interface DependencyCheck {
  tools: ToolStatus[];
}
