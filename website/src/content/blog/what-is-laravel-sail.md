---
title: 'What is Laravel Sail? A practical guide for 2026'
description: "Laravel Sail is the official Docker-based local development environment for Laravel. Here's what it does, when to use it, and how it compares to Valet and Herd."
publishedAt: 2026-05-07
tags: ['laravel-sail', 'docker', 'guide']
keywords:
  - what is laravel sail
  - laravel sail tutorial
  - laravel docker
  - laravel sail vs valet
  - laravel sail vs herd
---

If you've worked with Laravel for any amount of time, you've probably bumped into **Laravel Sail**. It's the official Docker-based local development environment that ships with every fresh Laravel install. But what does it actually do, and when should you reach for it instead of something like Valet or Herd?

This is the version of the answer I wish I'd had when I started.

## Laravel Sail in one sentence

Laravel Sail is a thin wrapper around `docker compose` that gives you a ready-to-run Laravel stack — PHP, a web server, and any extras like MySQL, Redis, Mailpit, or Meilisearch — without installing any of them on your Mac or Linux box.

You run `./vendor/bin/sail up`, and a minute later your Laravel app is serving on `http://localhost`, talking to a real MySQL container, and your machine has gained zero new global dependencies.

## What Sail actually is, under the hood

Sail is two things:

1. **A Docker compose file** at the root of your Laravel project (`compose.yaml`). It defines a `laravel.test` service plus whichever databases, caches, and tools you opted into.
2. **A small CLI** at `./vendor/bin/sail`. It's mostly a shell script that proxies commands into the running container. `sail artisan migrate` is `docker compose exec laravel.test php artisan migrate` with nicer ergonomics.

Everything else is just standard Docker. If you read the `compose.yaml`, it's not magic. It's plain services with image names like `mysql/mysql-server:8.0` and `redis:alpine`, port mappings, and volume mounts.

## What you get out of the box

When you run `php artisan sail:install`, the installer asks which services you want. Sail's defaults cover most of what a Laravel project actually needs:

- **Databases:** MySQL, PostgreSQL, MariaDB, MongoDB
- **Caches and queues:** Redis, Valkey, Memcached
- **Search:** Meilisearch, Typesense
- **Mail:** Mailpit (the spiritual successor to Mailhog)
- **Storage:** MinIO (S3-compatible)
- **Testing:** Selenium for browser tests
- **Realtime:** Soketi (Pusher-compatible WebSocket server)

Pick the ones you need, skip the rest. Want to add Redis later? Run `php artisan sail:install --with=redis` and it gets added to the compose file.

## Why Sail exists

Before Sail, the typical Laravel local-dev story on a Mac involved either:

- **Homebrew everything**: install PHP, MySQL, Redis, and Composer system-wide. Works, but every Laravel project shares the same PHP version, and upgrading anything risks breaking all your other projects.
- **Laravel Valet**: a slick CLI that runs PHP-FPM natively on macOS with a `.test` domain proxy. Fast and simple, but you still need to install the database engines yourself.
- **Vagrant/Homestead**: a full VM running Ubuntu with everything inside. Dependable but heavy. Booting takes a minute, and the disk usage is real.

Sail's pitch is: **the parity of a VM, the speed of native, no global installs**. Whatever runs in your container locally is what runs in CI, in staging, and (with the right Dockerfile) in production. No more "works on my machine" because someone has PHP 8.2 and someone else has 8.3.

## When Sail is the right call

You should reach for Sail when:

- **Your team uses Docker in CI or production.** Local parity is genuinely valuable here. The same `compose.yaml` you debug locally is one Dockerfile away from your prod runtime.
- **You're juggling multiple PHP versions.** Sail pins the PHP version per project. No more `brew unlink php@8.2 && brew link php@8.3` dance.
- **You want a real MySQL or Redis without installing them.** This alone is worth it for some people.
- **You hate config drift.** "What version of MySQL are we on?" Open `compose.yaml` and read the line. That's the answer.

## When Sail is not the right call

Sail isn't the right tool when:

- **You're doing very lightweight prototyping.** Spinning up a database container for a five-file experiment is overkill. Use Valet or just SQLite.
- **You're on a maxed-out laptop running 4+ Laravel projects at once.** Each Sail project is a database container, a Redis container, and a PHP container. Running ten of them gets warm. (More on this in a moment.)
- **You need millisecond-level filesystem performance on macOS.** Docker's bind mounts on Mac aren't free. For most apps it's fine; for hot-reloading frontends with thousands of files, the overhead shows. (OrbStack helps here.)

## The dirty secret of running multiple Sail projects

Here's the thing nobody tells you up front: **out of the box, Sail can only run one project at a time on a given machine**.

Why? Because every Sail project's `compose.yaml` claims port 80 for the web server, 3306 for MySQL, 6379 for Redis. Try to start a second project and Docker hits you with a "port is already allocated" error.

The fix is per-project port assignments in each project's `.env`:

```env
APP_PORT=8081
FORWARD_DB_PORT=33061
FORWARD_REDIS_PORT=63791
COMPOSE_PROJECT_NAME=shop
```

But doing that manually for ten projects, then remembering which ports map to which app, then dealing with `localhost:33063` instead of a real domain — it gets old fast. This is the exact problem that drove me to build [Sail Manager](/), a free macOS app that handles port allocation, `.test` domains, and bulk start/stop automatically. Worth a look if you've felt this pain.

If you'd rather DIY it, I wrote a more focused walkthrough: [How to run multiple Laravel Sail projects at the same time](/blog/run-multiple-laravel-sail-projects).

## Sail vs Valet vs Herd: the short version

| | Sail | Valet | Herd |
|---|---|---|---|
| Runtime | Docker | Native PHP | Native PHP |
| Database included | Yes | No (install yourself) | Yes (in paid tier) |
| Per-project PHP version | Yes | Sort of | Yes |
| Production parity | Best | Lower | Lower |
| Speed | Good | Excellent | Excellent |
| Multiple projects | Possible with effort | Trivial | Trivial |

**My take:** if your team's CI runs in Docker, use Sail. The parity tax is worth it. If you're a freelancer flipping between five tiny client projects a day, Valet or Herd are usually faster to live with.

## How to actually install Sail

If you're starting a new project:

```bash
laravel new myapp
cd myapp
php artisan sail:install
./vendor/bin/sail up -d
```

If you're adding it to an existing project:

```bash
composer require laravel/sail --dev
php artisan sail:install
./vendor/bin/sail up -d
```

The installer asks which services you want, writes the `compose.yaml`, and you're up.

## Where to go next

- The [official Sail docs](https://laravel.com/docs/sail) cover the CLI options well.
- For real-world multi-project workflows, see [Run multiple Laravel Sail projects at once](/blog/run-multiple-laravel-sail-projects).
- For adding more services to an existing project, see [How to add services to Laravel Sail](/blog/add-services-to-laravel-sail).

Sail is the most production-faithful local environment Laravel has ever shipped. The only catch is the bookkeeping that comes with running multiple Docker stacks, which is exactly the gap [Sail Manager](/) fills.
