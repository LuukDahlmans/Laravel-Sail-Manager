---
title: 'Running Horizon, queue:work, and schedule:run in Laravel Sail'
description: 'Long-running workers without juggling terminal tabs. How to add Horizon, queue workers, and the scheduler to your Laravel Sail setup, and how to make them start automatically.'
publishedAt: 2026-05-07
tags: ['laravel-sail', 'docker', 'horizon', 'queues']
keywords:
  - laravel sail horizon
  - laravel sail queue worker
  - laravel sail schedule
  - laravel sail queue work
  - sail horizon docker
  - laravel queue local
---

Real Laravel apps have background workers. Horizon for queues. `php artisan queue:work` if you skipped Horizon. The scheduler running every minute. Maybe Reverb or Pulse, depending on your stack.

In a typical Sail setup, you start these manually with one terminal tab per process:

```bash
# Tab 1
sail up

# Tab 2
sail artisan horizon

# Tab 3
sail artisan schedule:work

# Tab 4
sail artisan reverb:start
```

It works. It also gets old. And the moment you forget one ("why aren't my emails sending?"), you spend ten minutes troubleshooting before remembering you didn't start the queue worker. Here are the better options, in order from least to most effort.

## Option 1: Sail Manager auto-commands <span class="recommended">Recommended</span>

This is the option I built [Sail Manager](/)'s auto-commands feature for. Each project has a list of commands that fire when you start it.

For a typical setup:

| Label | Command | Mode |
|---|---|---|
| Horizon | `sail artisan horizon` | Service |
| Schedule | `sail artisan schedule:work` | Service |
| Vite | `sail npm run dev` | Service |

"Service" mode means it runs detached and stays alive until containers stop. "Once" mode means it runs blocking and exits, useful for things like an extra `php artisan migrate` or `storage:link`.

Each command gets its own log tab in the project detail view, so when you wonder why something isn't working, you can see Horizon's output without grepping through `sail logs`.

The presets are pre-configured for the obvious ones: Horizon, Queue worker, Schedule worker, Reverb, Pulse worker, Vite dev server, Bun dev, Pail, Telescope, Storage link, Cache clear, Optimize clear. Click to add, edit if you want.

This is what I use day to day. The other options below are what to do if you'd rather not install another app — they're all valid, just more bookkeeping. Full setup in the [auto-commands docs](/docs/auto-commands).

## Option 2: One-off, manual

If you're doing focused work on a feature and you'll remember to restart whatever you killed, manual is fine. One terminal per process. The shortcuts:

```bash
sail artisan horizon
sail artisan queue:work --tries=3
sail artisan schedule:work
sail artisan pail        # tail logs in human-friendly format
sail artisan reverb:start
sail artisan pulse:check
```

Each blocks the terminal. Use `Ctrl+C` to stop. If you're on a Mac with a tabbed terminal, this scales to two or three workers before it gets unwieldy.

## Option 3: Add the workers to compose.yaml

If you want workers to start with `sail up` automatically and you'd rather own the config in your repo, add them as services in `compose.yaml`. Each worker becomes a sibling container that mounts the same code and runs its own command:

```yaml
services:
  laravel.test:
    # ... existing config

  horizon:
    build:
      context: ./vendor/laravel/sail/runtimes/8.3
      dockerfile: Dockerfile
      args:
        WWWGROUP: '${WWWGROUP}'
    image: 'sail-8.3/app'
    extra_hosts:
      - 'host.docker.internal:host-gateway'
    environment:
      WWWUSER: '${WWWUSER}'
      LARAVEL_SAIL: 1
    volumes:
      - '.:/var/www/html'
    networks:
      - sail
    depends_on:
      - redis
    command: ['php', '/var/www/html/artisan', 'horizon']
    restart: unless-stopped

  scheduler:
    image: 'sail-8.3/app'
    volumes:
      - '.:/var/www/html'
    networks:
      - sail
    depends_on:
      - mysql
    command: ['php', '/var/www/html/artisan', 'schedule:work']
    restart: unless-stopped
```

This is the most "production-like" option. The compose file becomes a complete description of what runs. `sail up -d` starts everything; `sail down` stops everything.

The downsides:

- **Editing `compose.yaml` is heavy.** Sail Manager and other tools assume the compose file is mostly stock so they can read it without surprise. Diverging from the default makes upgrades painful.
- **Each worker is a separate container.** That's an extra Docker process, extra memory, extra boot time. For a single project it's fine; for ten Sail projects each running Horizon as its own container, your fan starts spinning.
- **Hot reload after code changes** still requires `sail restart horizon`. Workers don't notice file changes the way `php artisan serve` does.

The third one is mitigated by `php-fpm` reloading or by tools like `composer require spatie/laravel-horizon-watcher`, but it's a real friction point.

## Option 4: Run workers inside the existing laravel.test container

A lighter alternative: instead of adding new containers, exec workers inside the `laravel.test` container that's already running.

```bash
sail exec -d laravel.test php artisan horizon
sail exec -d laravel.test php artisan schedule:work
```

The `-d` flag detaches them so they keep running in the background. Output goes to the container's stdout, which you can tail with `sail logs laravel.test --follow`.

This is lighter than option 3 (no extra containers) but you're back to remembering to start them after every `sail up`. And `sail down` doesn't kill them gracefully because they're not in the compose lifecycle. Option 1 is essentially this approach with the ergonomics fixed.

There's a small subtlety with `docker compose exec` v2: it doesn't accept the legacy `-i` flag from `docker exec`. Sail's CLI handles the difference, but if you're running raw `docker compose exec`, drop `-i` and use `-T` if you specifically want to disable TTY.

## What about the scheduler specifically?

The scheduler has its own gotcha. There are two ways to run it:

- **`schedule:work`** keeps a long-running process that fires due tasks every minute. This is what you want in dev.
- **`schedule:run`** is the one-shot command that picks up due tasks and exits. This is what you'd put in a real cron, but in dev it's useless on its own because nothing fires it every minute.

Use `schedule:work` in development. If you put `schedule:run` in your auto-commands, it'll fire once at start and then never again. People hit this more often than I'd expect.

## What about the queue without Horizon?

Plenty of projects don't need Horizon. A simple `queue:work` is enough.

```bash
sail artisan queue:work --tries=3 --timeout=60 --memory=128
```

Watch out for two things:

- **The worker doesn't reload code.** After a code change to a job, `sail artisan queue:restart` tells running workers to exit gracefully so a supervisor restarts them. Without a supervisor, you're killing and restarting manually. Or use `--once` and rerun in a loop.
- **Failed jobs need a table.** Run `sail artisan queue:failed-table` and `sail artisan migrate` once, then `sail artisan queue:failed` to see what's broken.

If your queues are growing fast or you want metrics, switch to Horizon. The extra setup is a one-time cost.

## Picking an option

- **Want it to just work:** option 1 (Sail Manager auto-commands). Set once, forget.
- **One project, infrequent worker use, like the terminal:** option 2 (manual) is fine.
- **Production parity matters more than dev speed:** option 3 (compose). Your `compose.yaml` becomes the source of truth, which is closer to what your prod setup looks like.
- **You want option 1's behavior without an extra app:** option 4 (exec into laravel.test). You'll just be doing the same thing manually each time.

That's the full picture. Workers in Sail aren't hard. They're just one of those things that adds friction every day until you decide to fix it once.

For more on managing multiple Sail projects (where this gets really painful), see [How to run multiple Laravel Sail projects at the same time](/blog/run-multiple-laravel-sail-projects).
