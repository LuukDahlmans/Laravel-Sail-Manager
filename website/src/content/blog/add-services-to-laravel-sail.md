---
title: 'How to add services to Laravel Sail (MySQL, Redis, Mailpit, and more)'
description: 'Adding databases, caches, search engines, and other services to a Laravel Sail project. The official sail:install --with command, plus how to add custom services later.'
publishedAt: 2026-05-07
tags: ['laravel-sail', 'docker', 'how-to']
keywords:
  - laravel sail add service
  - laravel sail install with
  - laravel sail mysql
  - laravel sail redis
  - laravel sail mailpit
  - laravel sail meilisearch
---

Laravel Sail ships with a curated list of services you can opt into when you install it. MySQL, Redis, Mailpit, Meilisearch, Mongo, and a few others. But the docs are a little terse on **how to add a service after you've already installed Sail**, or how to bolt on a service that isn't in the official list at all.

Here's the full picture.

## The fastest way (if you don't want to read 1500 words) <span class="recommended">Recommended</span>

If you'd rather click checkboxes than edit YAML, [Sail Manager](/) gives you a service picker on the create-project form. All 13 official services are there, plus a free-form field for custom ones. Pick what you want, click Create, and the right `--with=` flags get passed to `sail:install` for you. No `compose.yaml` editing, no `.env` patching, no port collisions. Free, MIT, [docs here](/docs/services).

For everyone who wants to know what's actually happening under the hood — read on.

## The 13 services Sail knows about

When you run `php artisan sail:install`, the installer offers these:

| Service | Category | Default port |
|---|---|---|
| MySQL | Database | 3306 |
| PostgreSQL | Database | 5432 |
| MariaDB | Database | 3306 |
| MongoDB | Database | 27017 |
| Redis | Cache | 6379 |
| Valkey | Cache | 6379 |
| Memcached | Cache | 11211 |
| Meilisearch | Search | 7700 |
| Typesense | Search | 8108 |
| Mailpit | Mail | 1025 / 8025 |
| MinIO | S3 storage | 9000 / 8900 |
| Selenium | Browser tests | 4444 |
| Soketi | WebSockets | 6001 |

Pick the ones you need. The installer writes them into your `compose.yaml` and adds the right env defaults to your `.env`.

## Adding a service to a fresh project

For a new install, just include the services in the `--with=` flag:

```bash
php artisan sail:install --with=mysql,redis,mailpit
```

That's the cleanest path. The installer is idempotent in a single run, but rerunning it later doesn't merge — it overwrites. So pick what you need up front.

## Adding a service to an existing project

This is where most people get stuck. Sail's `--with=` flag rewrites `compose.yaml` from scratch, which clobbers any custom changes you've made. Two safer paths:

### Option A: rerun sail:install if you haven't customized compose.yaml

If your `compose.yaml` is still vanilla, rerunning the installer is fine:

```bash
php artisan sail:install --with=mysql,redis,mailpit,meilisearch
```

It'll regenerate the file with everything in the new list. Backup first, just in case.

### Option B: manually add the service block

Open `compose.yaml` and add the service. The official Sail compose snippets for each service are in the [Laravel Sail repo](https://github.com/laravel/sail/tree/1.x/stubs). Copy the relevant block.

For example, to add Redis to a project that doesn't have it:

```yaml
services:
  laravel.test:
    # ... existing config
    depends_on:
      - mysql
      - redis    # add this line

  redis:
    image: 'redis:alpine'
    ports:
      - '${FORWARD_REDIS_PORT:-6379}:6379'
    volumes:
      - 'sail-redis:/data'
    networks:
      - sail
    healthcheck:
      test: ['CMD', 'redis-cli', 'ping']
      retries: 3
      timeout: 5s

volumes:
  sail-redis:
    driver: local
```

Then update your `.env`:

```env
REDIS_HOST=redis
REDIS_PORT=6379
CACHE_STORE=redis
SESSION_DRIVER=redis
QUEUE_CONNECTION=redis
```

And restart:

```bash
sail down
sail up -d
```

Same pattern works for any other service. Pull the snippet from Sail's source, drop it in, update `.env`, restart.

## What about services Sail doesn't know about?

Plenty of real-world projects need things outside Sail's default list. ClickHouse for analytics. RabbitMQ for legacy queues. A dedicated Elasticsearch when Meilisearch isn't enough. Postgres + TimescaleDB. The list goes on.

Adding any of these is just a `compose.yaml` edit. Pick an image, give it a name, expose the port, point your app at it via env vars.

```yaml
services:
  clickhouse:
    image: 'clickhouse/clickhouse-server:23'
    ports:
      - '${FORWARD_CLICKHOUSE_PORT:-8123}:8123'
    volumes:
      - 'sail-clickhouse:/var/lib/clickhouse'
    networks:
      - sail
    ulimits:
      nofile:
        soft: 262144
        hard: 262144

volumes:
  sail-clickhouse:
    driver: local
```

The convention I'd suggest:

- Use **`FORWARD_<NAME>_PORT`** as the env var name for the host port. It matches Sail's pattern and stays out of the way of your app's own env.
- Use **`sail-<name>`** for the volume name. Same reasoning. If you ever run `docker volume ls`, you can spot which volumes belong to which project at a glance.
- Add the service to the `sail` network. Otherwise Laravel can't reach it by container name.

## The port collision problem

Once you start adding services across multiple Sail projects, you'll trip over the port-collision problem. Two projects with Mailpit both want port 1025; Docker says no.

The fix is per-project port assignments in each `.env`:

```env
FORWARD_DB_PORT=33061
FORWARD_REDIS_PORT=63791
FORWARD_MAILPIT_PORT=10251
```

I've written about this in more detail in [How to run multiple Laravel Sail projects at the same time](/blog/run-multiple-laravel-sail-projects), but the short version is: every service has a `FORWARD_*_PORT` variable, and you need to set them per-project to avoid conflicts.

[Sail Manager](/) handles this automatically — it allocates free ports per service per project and writes them to `.env` so you don't have to think about it.

## Choosing services for common stacks

A few stacks I keep coming back to, in case it saves you the decision:

- **Plain Laravel app:** MySQL + Redis + Mailpit. The minimum that gives you sessions, cache, queues, and email testing.
- **API only:** MySQL + Redis. Skip Mailpit if the API doesn't send mail.
- **Full-text search app:** Add Meilisearch. Or Typesense if you prefer it; both are fine.
- **Photo or file uploads:** Add MinIO. Lets you point your S3-driven code at a local S3-compatible store.
- **Realtime app:** Add Soketi (Pusher-compatible) or run your own WebSocket service.
- **Browser tests:** Selenium. Run it once, set up Dusk, you're done.

## After adding a service

Two things to remember every time:

1. **Update `.env`.** New services often need new keys (`MEILISEARCH_HOST`, `MAIL_MAILER`, etc.). Sail's installer does this for you on first install but not when you add manually.
2. **Run migrations or setup commands.** New databases need migrations. New caches don't, but you might want to flush them. New search engines need their indexes built.

For Laravel-specific cleanups:

```bash
sail artisan config:clear
sail artisan cache:clear
sail artisan migrate
```

That's it. Adding services to Sail is mostly mechanical once you know the convention. The hard part is knowing which services to add, not how to add them.
