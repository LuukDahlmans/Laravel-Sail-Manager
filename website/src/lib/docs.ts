// Single source of truth for the docs site. Used by:
//   • DocsLayout — to render the sidebar nav and previous/next links
//   • The client-side search — to filter pages and section anchors
//
// Each page lists its sections so a user typing "auto-command" can jump
// straight to /docs/auto-commands#service-mode without having to read the
// whole page first.

export interface DocSection {
  id: string;
  label: string;
}

export interface DocPage {
  slug: string;
  title: string;
  description: string;
  sections: DocSection[];
}

export interface DocGroup {
  title: string;
  pages: DocPage[];
}

export const docGroups: DocGroup[] = [
  {
    title: 'Getting started',
    pages: [
      {
        slug: 'getting-started',
        title: 'Getting started',
        description: 'Install Sail Manager, walk through the first-run wizard, and learn where the app keeps its data.',
        sections: [
          { id: 'requirements', label: 'Requirements' },
          { id: 'install', label: 'Install' },
          { id: 'first-run', label: 'First-run wizard' },
          { id: 'data-locations', label: 'Where data lives' },
        ],
      },
    ],
  },
  {
    title: 'Projects',
    pages: [
      {
        slug: 'projects',
        title: 'Projects',
        description: 'Create new Sail projects, clone from Git, import existing folders, and manage starts and stops.',
        sections: [
          { id: 'create-new', label: 'Create a new project' },
          { id: 'clone-from-git', label: 'Clone from Git' },
          { id: 'import-existing', label: 'Import an existing folder' },
          { id: 'starting-stopping', label: 'Start, stop, delete' },
          { id: 'port-allocation', label: 'How ports get assigned' },
        ],
      },
      {
        slug: 'local-urls',
        title: 'Local URLs',
        description: 'How the .test domain system works, how to enable it, and what to do when it breaks.',
        sections: [
          { id: 'how-local-urls-work', label: 'How it works' },
          { id: 'enable-local-urls', label: 'Enable / change TLD' },
          { id: 'https', label: 'HTTPS' },
          { id: 'local-urls-troubleshooting', label: 'When things break' },
        ],
      },
    ],
  },
  {
    title: 'Per-project tools',
    pages: [
      {
        slug: 'tools',
        title: 'Project detail tabs',
        description: 'Overview, logs, shell, environment, database, run command, and history tabs explained.',
        sections: [
          { id: 'overview-tab', label: 'Overview' },
          { id: 'logs-tab', label: 'Logs' },
          { id: 'shell-tab', label: 'Shell' },
          { id: 'environment-tab', label: 'Environment' },
          { id: 'database-tab', label: 'Database' },
          { id: 'run-command', label: 'Run command' },
          { id: 'history-tab', label: 'History' },
        ],
      },
      {
        slug: 'auto-commands',
        title: 'Auto-commands',
        description: 'Run Horizon, queues, schedulers, npm dev, and any other command automatically when a project starts.',
        sections: [
          { id: 'modes', label: 'Service vs once mode' },
          { id: 'presets', label: 'Built-in presets' },
          { id: 'custom', label: 'Custom commands' },
          { id: 'run-now', label: 'Run all now' },
        ],
      },
      {
        slug: 'templates',
        title: 'Templates',
        description: 'Save your stack and reuse it on every new project. Includes the three built-in defaults.',
        sections: [
          { id: 'what-templates', label: 'What templates do' },
          { id: 'create-template', label: 'Creating one' },
          { id: 'built-in-templates', label: 'Built-in templates' },
        ],
      },
    ],
  },
  {
    title: 'App-wide',
    pages: [
      {
        slug: 'tray',
        title: 'Menu bar tray',
        description: 'Per-project submenus, status glyphs, and hide vs quit behavior.',
        sections: [
          { id: 'submenus', label: 'Per-project submenus' },
          { id: 'status-glyphs', label: 'Status glyphs' },
          { id: 'window-behavior', label: 'Closing vs quitting' },
        ],
      },
      {
        slug: 'settings',
        title: 'Settings',
        description: 'Theme, projects folder, editor pick, local URLs, and the danger zone.',
        sections: [
          { id: 'appearance', label: 'Appearance' },
          { id: 'local-urls', label: 'Local URLs' },
          { id: 'projects-folder', label: 'Projects folder' },
          { id: 'editor', label: 'Editor' },
          { id: 'updates', label: 'Updates' },
          { id: 'danger-zone', label: 'Danger zone' },
        ],
      },
    ],
  },
  {
    title: 'Reference',
    pages: [
      {
        slug: 'services',
        title: 'Built-in services',
        description: 'The 13 Sail services Sail Manager wires into the create form, plus custom services.',
        sections: [
          { id: 'service-list', label: 'Full service list' },
          { id: 'custom-services', label: 'Custom services' },
        ],
      },
      {
        slug: 'editors',
        title: 'Editor integrations',
        description: 'Open in editor: PhpStorm, VS Code, Cursor, and Zed.',
        sections: [
          { id: 'supported-editors', label: 'Supported editors' },
          { id: 'how-it-opens', label: 'How "Open in editor" works' },
        ],
      },
      {
        slug: 'troubleshooting',
        title: 'Troubleshooting',
        description: 'Fixes for the common issues: Docker not responding, port conflicts, broken local URLs, fresh-project 500s, and more.',
        sections: [
          { id: 'docker-down', label: 'Docker not responding' },
          { id: 'fresh-500s', label: '500s on a fresh project' },
          { id: 'port-conflicts', label: 'Port conflicts' },
          { id: 'local-urls-broken', label: 'Local URLs not loading' },
          { id: 'orphans', label: 'Untracked projects' },
          { id: 'shell-echo', label: 'Shell echoes weird characters' },
        ],
      },
      {
        slug: 'reset',
        title: 'Reset application',
        description: 'How to wipe Sail Manager state without losing your code or Docker volumes.',
        sections: [
          { id: 'what-it-does', label: 'What it does' },
          { id: 'what-it-keeps', label: 'What it keeps' },
        ],
      },
    ],
  },
];

// Flat lookup table — used for prev/next navigation and search.
export const allPages: DocPage[] = docGroups.flatMap((g) => g.pages);

export function findAdjacent(currentSlug: string): { prev?: DocPage; next?: DocPage } {
  const i = allPages.findIndex((p) => p.slug === currentSlug);
  return {
    prev: i > 0 ? allPages[i - 1] : undefined,
    next: i >= 0 && i < allPages.length - 1 ? allPages[i + 1] : undefined,
  };
}
