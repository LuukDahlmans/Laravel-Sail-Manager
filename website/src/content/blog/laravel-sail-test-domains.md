---
title: 'How to use .test domains with Laravel Sail (without editing /etc/hosts)'
description: "Real local URLs like http://shop.test for Laravel Sail. Three approaches: a managed setup, dnsmasq + Traefik, and /etc/hosts. Which one to pick and why."
publishedAt: 2026-05-07
tags: ['laravel-sail', 'docker', 'how-to', 'dns']
keywords:
  - laravel sail test domain
  - laravel sail local url
  - laravel sail .test
  - laravel sail dnsmasq
  - laravel sail traefik
  - laravel sail without etc hosts
---

When you start with Sail, your app lives at `http://localhost`. That's fine for one project. The moment you have two or three, you're either juggling port numbers (`localhost:8081`, `localhost:8082`...) or wishing your projects had real names like `http://shop.test` and `http://api.test`.

There are three ways to get there, listed in order from one-click to full DIY.

## Why `.test` and not `.local`?

A quick aside before we get into setup. The `.test` TLD is reserved by IANA for testing and won't ever be a real domain. `.local` is taken by mDNS / Bonjour and conflicts with how macOS does network discovery. `.dev` was a community favorite for years until Google bought it as a real TLD and broke half the world's local setups.

The safe options are `.test` (the standard now) or `.localhost`. Most Laravel devs use `.test`. Valet defaults to it. Herd does too. Sail Manager defaults to `.sail` to stay out of the way of an existing Valet or Herd setup, but you can change that in settings.

## Approach 1: Use Sail Manager <span class="recommended">Recommended</span>

This is the path of least resistance. [Sail Manager](/) is a free, open-source macOS app that handles the whole `.test` setup for every project automatically:

- Runs the Traefik and dnsmasq containers for you. They start when the app starts.
- Writes the `/etc/resolver/<tld>` file once at first setup. **Single sudo prompt, ever.**
- Regenerates the proxy config whenever you create, rename, or delete a project. You don't think about it.
- Notices when the proxy or DNS containers go missing (after a Docker reset, say) and brings them back automatically on next launch.
- Lets you change the TLD from `.test` to anything else by typing it in settings. Resolver file gets rewritten correctly.
- Adds an HTTPS toggle that generates a local CA, installs it in your keychain, and serves your apps over `https://shop.test`.

Install, enable local URLs in settings, and every existing or new project you have gets `http://<name>.<tld>` working. The full setup story is in the [docs](/docs/local-urls).

If you'd rather understand every piece — or build your own version — the next two approaches walk through what's happening under the hood.

## Approach 2: dnsmasq + Traefik (DIY)

This is what Sail Manager does internally. Three pieces.

### Piece 1: dnsmasq for wildcard DNS

dnsmasq is a tiny DNS resolver. Run it locally and tell it: "for anything ending in `.test`, return `127.0.0.1`."

You can install it via Homebrew:

```bash
brew install dnsmasq
echo 'address=/.test/127.0.0.1' > $(brew --prefix)/etc/dnsmasq.conf
sudo brew services start dnsmasq
```

Or run it as a Docker container, which is what I'd actually recommend so it's easy to clean up:

```bash
docker run -d --name sail-dns \
  -p 127.0.0.1:5354:53/udp \
  -p 127.0.0.1:5354:53/tcp \
  4km3/dnsmasq \
  --address=/.test/127.0.0.1
```

Note the **port 5354** instead of 53. macOS reserves port 53 for the system resolver, and Valet/Herd are usually already running on 53 if they're installed. Using 5354 keeps you out of trouble.

### Piece 2: tell macOS to use it

dnsmasq is running but macOS doesn't know to ask it. Drop a file at `/etc/resolver/test`:

```
nameserver 127.0.0.1
port 5354
```

This file is the only piece that needs sudo. **Once.** From now on, any DNS lookup ending in `.test` goes to your local dnsmasq, which returns `127.0.0.1`.

```bash
sudo mkdir -p /etc/resolver
echo 'nameserver 127.0.0.1
port 5354' | sudo tee /etc/resolver/test
```

Verify with `scutil --dns | grep -A2 test` — you should see your custom resolver listed.

### Piece 3: Traefik on port 80 to route by hostname

Now `shop.test` resolves to `127.0.0.1`. We need something on port 80 to read the `Host` header and route to the right container. Traefik is the standard pick:

```bash
docker run -d --name sail-proxy \
  -p 80:80 \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  -v $HOME/.sail-manager/proxy:/etc/traefik:ro \
  traefik:v3.0
```

Configure it via a static config file (`traefik.yml`) and a dynamic config file (`dynamic.yml`). The dynamic file is where you list the per-project routes:

```yaml
http:
  routers:
    shop:
      rule: 'Host(`shop.test`)'
      service: shop
    api:
      rule: 'Host(`api.test`)'
      service: api

  services:
    shop:
      loadBalancer:
        servers:
          - url: 'http://host.docker.internal:8081'
    api:
      loadBalancer:
        servers:
          - url: 'http://host.docker.internal:8082'
```

Add a router/service per project. Traefik watches the file and reloads automatically.

The result: visit `http://shop.test`, dnsmasq says "that's 127.0.0.1", browser hits `127.0.0.1:80`, Traefik reads the Host header and forwards to `host.docker.internal:8081`, which is your `shop` project's `laravel.test` container. No port numbers, no `/etc/hosts` edits per project, no sudo after the first time.

### The maintenance reality

The setup above works. But if you've ever maintained one, you know the small annoyances:

- **You add a new project: remember to add a route to `dynamic.yml`.**
- **You delete a project: remember to remove the route.**
- **Docker Desktop resets and your containers vanish: rebuild Traefik and dnsmasq.**
- **You upgrade macOS and the resolver file gets touched: re-add it.**
- **Port 80 is taken by something (Apache, Nginx, Herd): figure out what and decide who wins.**

It's not bad. It's also not zero. After about the third "where did my `.test` domains go" debugging session you start wondering if there's a better way (which is why approach 1 exists).

## Approach 3: `/etc/hosts`

The lowest-tech option, included for completeness. Open `/etc/hosts` and add a line per project:

```
127.0.0.1   shop.test
127.0.0.1   api.test
127.0.0.1   admin.test
```

Now `http://shop.test` resolves to `127.0.0.1`. **But it doesn't know which port to hit.** Each project is on a different host port, so you'd visit `http://shop.test:8081`, `http://api.test:8082`, etc.

That's barely better than `localhost:8081`. The whole point of `.test` domains is to drop the port number. So `/etc/hosts` alone doesn't solve the problem; you also need a reverse proxy on port 80 that routes by hostname (which puts you back in approach 2 territory anyway).

The other downside: editing `/etc/hosts` needs sudo every time you add a project. After the fifth `sudo vim /etc/hosts` you start to resent the workflow.

## A word on HTTPS

For HTTPS to work locally, the cert needs to be signed by a CA your browser trusts. Three approaches:

1. **Don't bother.** `http://` works fine for local dev. Reserve HTTPS for staging.
2. **Use mkcert.** It's a CLI tool that creates a local CA, installs it in your trust stores, and issues certs you can hand to Traefik. Reliable, requires a one-time `mkcert -install`.
3. **Have Sail Manager do it.** It uses the same CA pattern under the hood: generate a CA on first HTTPS-enable, install it, issue a wildcard for `*.test`, configure Traefik with the cert, switch the proxy to port 443. One toggle.

Whichever you pick, the gotcha is keychain trust. The CA needs to be in your **login keychain** with **always trust** for SSL. Browsers like Chrome and Safari read from the system keychain at launch, so a cert added later might not be trusted until you restart the browser.

## Picking an approach

If you have **one project**, just use `localhost`. You don't need any of this.

If you have **two or more projects** and you don't want to babysit infrastructure, approach 1 (Sail Manager) saves real time. It's free and the model under the hood is exactly the right one anyway.

If you have **a strong preference for owning your stack**, approach 2. Plan an afternoon for the initial setup and an hour every few months when something gets out of sync.

For more on running many Sail projects at once (which is the prerequisite for `.test` domains being interesting), see [How to run multiple Laravel Sail projects at the same time](/blog/run-multiple-laravel-sail-projects).
