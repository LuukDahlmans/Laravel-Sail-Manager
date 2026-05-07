---
title: 'How to run multiple Laravel Sail projects at the same time'
description: 'Out of the box, Sail pins every project to port 80, 3306, and 6379. Here are three ways to run multiple Laravel projects simultaneously, from one-click to full DIY.'
publishedAt: 2026-05-07
tags: ['laravel-sail', 'docker', 'how-to']
keywords:
  - laravel sail multiple projects
  - laravel sail port already in use
  - laravel sail port conflict
  - run multiple sail
  - laravel multiple projects docker
---

If you've tried to spin up two Laravel Sail projects at once, you've probably seen this:

```
Error response from daemon: driver failed programming external connectivity
on endpoint laravel.test: Bind for 0.0.0.0:80 failed: port is already allocated
```

Sail's defaults claim port 80, port 3306, and port 6379 for every project. There's no built-in handling for "I want to run my e-commerce app and my admin dashboard at the same time." So we have to fix it ourselves.

Here are three ways to do that, in order from least to most effort.

## Why Sail does this

Before we fix it, a quick sanity check on what's actually happening.

When you run `./vendor/bin/sail up`, Docker spins up a container per service in your `compose.yaml`. Each container is mapped to a host port via lines like:

```yaml
ports:
  - '${APP_PORT:-80}:80'
  - '${FORWARD_DB_PORT:-3306}:3306'
```

Two host ports, two containers. So far, fine. The problem is the **defaults** — `APP_PORT:-80` means "use APP_PORT from .env, or fall back to 80". Every fresh Sail project ships with no `APP_PORT` in `.env`, so they all default to 80. First project to start grabs port 80; the second can't.

This is one of those situations where the framework picked good defaults for the 90% case (running one project) and left the 10% case (running ten) up to you.

## Approach 1: Use Sail Manager <span class="recommended">Recommended</span>

Full disclosure: I built [Sail Manager](/) because I got tired of doing the manual approaches below for every new project.

It's a free, open-source macOS app that:

- Probes for free ports on both IPv4 and IPv6 wildcards before claiming one (more on why that matters in a second).
- Writes the `APP_PORT`, `FORWARD_*_PORT`, and `COMPOSE_PROJECT_NAME` keys into each project's `.env` automatically.
- Manages a Traefik proxy and dnsmasq container so `http://shop.test` works for every project, without you ever editing `/etc/hosts`.
- Asks for your password **once** to set up the resolver. Never again.
- Gives you "Start all" and "Stop all" buttons for when you're switching contexts.

Install, point it at your projects folder, and the port-collision problem stops being a thing you think about. The full setup story is in the [docs](/docs/getting-started).

If you'd rather understand and own every piece of this yourself, the next two approaches walk through what Sail Manager is doing under the hood.

## Approach 2: Edit each project's `.env` by hand

The official answer. For each project, set unique ports in `.env`:

```env
# Project: shop
APP_PORT=8081
FORWARD_DB_PORT=33061
FORWARD_REDIS_PORT=63791
COMPOSE_PROJECT_NAME=shop
WWWUSER=1000
WWWGROUP=1000
```

```env
# Project: admin
APP_PORT=8082
FORWARD_DB_PORT=33062
FORWARD_REDIS_PORT=63792
COMPOSE_PROJECT_NAME=admin
```

```env
# Project: api
APP_PORT=8083
FORWARD_DB_PORT=33063
FORWARD_REDIS_PORT=63793
COMPOSE_PROJECT_NAME=api
```

A few things to note:

- **`COMPOSE_PROJECT_NAME` is critical.** Without it, two projects with services named `mysql` will collide on the container name, not just the port. Setting it scopes everything.
- **`WWWUSER` and `WWWGROUP`** ensure files written by the container have the right ownership on macOS and Linux. Sail sets these automatically on first run, but it's good to be explicit.
- **Every service you've enabled needs its `FORWARD_*_PORT`.** Mailpit (`FORWARD_MAILPIT_PORT`), Meilisearch (`FORWARD_MEILISEARCH_PORT`), MinIO, and so on. The full list lives in Sail's compose file as variables.

After you set this up, your projects live at `localhost:8081`, `localhost:8082`, etc. Functional, ugly, easy to forget which port is which.

### The IPv6 gotcha on macOS

Here's a trap I hit. You probe for a free port with `lsof -i :8081` or `nc -z localhost 8081`, get nothing, and assume the port is free. You write it to `.env`, run `sail up`, and Docker says "port already allocated."

What happened: Docker on macOS binds both the IPv4 wildcard (`0.0.0.0:port`) **and** the IPv6 wildcard (`[::]:port`) when it claims a port. A previous Docker container is holding `[::]:8081` invisibly. `lsof -i :8081` doesn't always show it.

The fix in code is to attempt a TCP bind on **both** `0.0.0.0:port` AND `[::]:port` before trusting that a port is free:

```rust
// pseudo-Rust, but the logic is the same in any language
fn is_port_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
        && TcpListener::bind(("::", port)).is_ok()
}
```

If you're picking ports manually, just leave gaps. Going `8081, 8082, 8083` straight is fine; going `80, 81, 82` gets you into trouble with system ports.

## Approach 3: Local domains via reverse proxy

Once you've got unique ports, the next problem is that nobody wants to remember `localhost:8083`. A reverse proxy on port 80 plus some DNS magic gets you to real domains like `http://shop.test` for every project.

The pieces you need:

1. A **Traefik** or **Nginx** container running on host port 80, with rules like "if the Host header is `shop.test`, send to `host.docker.internal:8081`."
2. A way to resolve `*.test` to `127.0.0.1`. Either edit `/etc/hosts` per project (annoying) or run a wildcard DNS resolver. **dnsmasq** is the standard pick: it answers `address=/.test/127.0.0.1` for any subdomain of `.test`.
3. On macOS, a `/etc/resolver/test` file telling the system to use that dnsmasq for any lookup ending in `.test`. This is the only step that needs sudo, and you only need it once per TLD.

Here's the rough Traefik config you'd write to a dynamic config file:

```yaml
http:
  routers:
    shop:
      rule: 'Host(`shop.test`)'
      service: shop
  services:
    shop:
      loadBalancer:
        servers:
          - url: 'http://host.docker.internal:8081'
```

Add a router and service per project, restart Traefik, and `http://shop.test` works.

This is genuinely the right approach. It's also a lot of moving pieces to keep in sync. If you delete a project, you remember to remove its router. If you add a new one, you remember to add it. And so on, across ten projects, forever.

## Picking an approach

If you want it to **just work**, go with approach 1. Sail Manager exists specifically for this and the rest of the article is what it does behind the scenes anyway.

If you have **2 projects** and you like understanding every line of config, approach 2 (manual `.env` edits) is fine. You'll spend an hour up front and trip over the IPv6 gotcha at least once.

If you want **real `.test` domains** without the management overhead of doing it by hand for ten projects, approach 1 again. If you want to wire up your own version of approach 3 because you like Traefik, that's a respectable hobby and you'll learn things along the way.

## Common pitfalls

A few mistakes I've seen (and made):

- **Forgetting `COMPOSE_PROJECT_NAME`.** Two projects can have unique ports but still fight over container names. Set the project name explicitly.
- **Leaving Vite on its default port.** If you've got Vite running for HMR, set `VITE_PORT` in `.env` per project too.
- **Killing one project's containers and assuming the ports are immediately free.** Docker on macOS sometimes holds the IPv6 wildcard for 30 to 60 seconds after `down`. Wait, or run `docker system prune` if you're impatient.
- **Hardcoding ports in your code.** If you reference `localhost:8081` anywhere in your app code or tests, those break the moment the port changes. Read from `APP_URL` instead.

That's the full picture. Pick the approach that matches your project count and time budget.
