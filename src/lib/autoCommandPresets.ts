import type { AutoCommandRunMode } from './types';

export interface AutoCommandPreset {
  label: string;
  command: string;
  runMode: AutoCommandRunMode;
  description: string;
  /** Group label for grouping presets in the UI. */
  group: 'workers' | 'frontend' | 'tooling' | 'maintenance';
}

export const AUTO_COMMAND_PRESETS: AutoCommandPreset[] = [
  // Long-running workers
  {
    label: 'Horizon',
    command: 'sail artisan horizon',
    runMode: 'service',
    description: 'Redis queue dashboard + workers',
    group: 'workers',
  },
  {
    label: 'Queue worker',
    command: 'sail artisan queue:work --tries=3',
    runMode: 'service',
    description: 'Process the default queue',
    group: 'workers',
  },
  {
    label: 'Schedule worker',
    command: 'sail artisan schedule:work',
    runMode: 'service',
    description: 'Run scheduled tasks every minute',
    group: 'workers',
  },
  {
    label: 'Reverb',
    command: 'sail artisan reverb:start',
    runMode: 'service',
    description: 'Laravel Reverb WebSocket server',
    group: 'workers',
  },
  {
    label: 'Pulse worker',
    command: 'sail artisan pulse:work',
    runMode: 'service',
    description: 'Background workers for Laravel Pulse',
    group: 'workers',
  },

  // Frontend tooling
  {
    label: 'Vite dev server',
    command: 'sail npm run dev',
    runMode: 'service',
    description: 'HMR for frontend assets (npm run dev)',
    group: 'frontend',
  },
  {
    label: 'Bun dev',
    command: 'sail bun run dev',
    runMode: 'service',
    description: 'Vite via Bun if your project uses it',
    group: 'frontend',
  },

  // Dev tooling
  {
    label: 'Pail (live logs)',
    command: 'sail artisan pail',
    runMode: 'service',
    description: 'Tail Laravel logs in real time',
    group: 'tooling',
  },
  {
    label: 'Telescope publish',
    command: 'sail artisan telescope:publish',
    runMode: 'once',
    description: 'Publish Telescope assets after install',
    group: 'tooling',
  },

  // Run-on-every-start maintenance
  {
    label: 'Storage link',
    command: 'sail artisan storage:link',
    runMode: 'once',
    description: 'Symlink storage/app/public → public/storage',
    group: 'maintenance',
  },
  {
    label: 'Cache clear',
    command: 'sail artisan cache:clear',
    runMode: 'once',
    description: 'Wipe the application cache on every start',
    group: 'maintenance',
  },
  {
    label: 'Optimize clear',
    command: 'sail artisan optimize:clear',
    runMode: 'once',
    description: 'Clear all bootstrap caches',
    group: 'maintenance',
  },
];

export const PRESET_GROUP_LABELS: Record<AutoCommandPreset['group'], string> = {
  workers: 'Long-running workers',
  frontend: 'Frontend',
  tooling: 'Dev tooling',
  maintenance: 'Run-on-start maintenance',
};
