---
title: 'Laravel Sail vs Valet vs Herd: which one should you use in 2026?'
description: "An honest comparison of the three main local Laravel dev environments on macOS. What each one does well, where each one struggles, and how to pick."
publishedAt: 2026-05-07
tags: ['laravel-sail', 'laravel-valet', 'laravel-herd', 'comparison']
keywords:
  - laravel sail vs valet
  - laravel sail vs herd
  - laravel valet vs herd
  - best laravel local environment
  - laravel local development
---

There are three serious options for running Laravel locally on a Mac in 2026: **Sail**, **Valet**, and **Herd**. They all let you build apps. They make very different bets on how to get there.

Here's how I think about choosing between them, after using all three for real work.

## The 30-second version

| | Sail | Valet | Herd |
|---|---|---|---|
| What it is | Docker compose stack | Native PHP-FPM + nginx | Native PHP + GUI |
| Runtime | Docker | Native macOS | Native macOS |
| Database included | Yes | No | Yes (paid) |
| Setup time | Medium | Fast | Fastest |
| Speed (HTTP) | Good | Excellent | Excellent |
| Speed (filesystem) | OK to slow | Native | Native |
| Production parity | Best | Lower | Lower |
| Cost | Free | Free | Free with paid tier |
| Multiple projects | Possible with effort | Trivial | Trivial |
| PHP version per project | Yes, in Docker | Sort of | Yes |

If you want me to skip the analysis: **Sail if you ship to Docker in production, Herd if you want the easy life and don't mind paying, Valet if you've been doing this since 2017 and you're happy.**

## Laravel Sail

Sail is Laravel's official Docker setup. Every fresh `laravel new` ships with `vendor/bin/sail` and a `compose.yaml`. Pick services with `php artisan sail:install`, run `sail up`, and you've got a Laravel app talking to a real MySQL container.

### What Sail does well

- **Production parity.** The MySQL version your dev container is running is the same as the one in CI and the same one (with the right Dockerfile) in prod. If it works locally, you're not surprised in staging.
- **No global installs.** No `brew install php@8.3`. No `brew install mysql`. The first developer onboarding to your project doesn't need to install half the internet first.
- **Per-project PHP versions.** Two projects on PHP 8.2 and 8.4 coexist without `brew unlink && brew link` dancing.
- **Real services, not approximations.** Mailpit, Meilisearch, MinIO, MongoDB are containers with their actual official images. SQLite-as-DB-substitute or a fake S3 lets you skip these in dev, but you pay for it later.

### Where Sail hurts

- **Slow filesystem on macOS.** Bind mounts on Mac are not free. For most apps it's fine; for hot-reloading frontends with thousands of files, the overhead shows. OrbStack helps a lot here. Mutagen syncs help even more.
- **Memory and battery.** Each project is a database container, a Redis container, a PHP container. Running ten of them gets warm.
- **Multiple projects don't just work.** Out of the box, every Sail project claims port 80, 3306, and 6379. The second one doesn't start. (See [How to run multiple Laravel Sail projects at the same time](/blog/run-multiple-laravel-sail-projects).)
- **`compose.yaml` is yours to maintain.** Add a service, edit the file. Upgrade Sail, hope your changes don't conflict.

## Laravel Valet

Valet runs PHP-FPM natively on your Mac through Homebrew. It uses dnsmasq + nginx to give you `.test` domains automatically. No containers, just native processes.

### What Valet does well

- **Speed.** There's nothing between your code and your CPU. PHP runs as fast as your hardware lets it. No mount overhead.
- **Multiple projects are trivial.** Park a directory, every subfolder becomes `<name>.test`. Done.
- **Battery life.** Valet idles at near zero. Sail with ten projects active doesn't.
- **Free, simple, works.**

### Where Valet hurts

- **Databases are your problem.** You install MySQL via Homebrew, and now you've got system-wide MySQL with whatever version Brew has today. Want MySQL 5.7 for one project and 8.0 for another? Solve that yourself.
- **Per-project PHP version.** Valet's `valet isolate` works, but it's clunkier than Sail's "the version is in the compose file."
- **Lower production parity.** If your prod runs Docker, your local doesn't match. Most of the time it doesn't matter; the times it does, it really does.
- **macOS-only.** Linux Valet exists but isn't first-class.

## Laravel Herd

Herd is a newer entrant from the Laravel team. Native PHP via a polished GUI app. Free tier; paid tier ("Herd Pro") adds databases, mail testing, dump/log inspection, and so on.

### What Herd does well

- **Onboarding speed.** Install the app, point it at a folder, and you have a working Laravel dev environment in two minutes. No CLI configuration.
- **GUI for everything.** PHP version per site, log viewer, database management, dump catcher. If you don't like terminal-driven workflows, this is the one.
- **Free tier is generous.** Free Herd is essentially "Valet, but with a UI you don't have to think about."
- **Paid tier is genuinely good.** The dump and log inspectors save real time.

### Where Herd hurts

- **Same parity story as Valet.** Native PHP. If your prod is Docker, you're still drifting.
- **Paid features locked behind subscription.** Fair enough — software costs money. Worth knowing up front.
- **Less flexible than Sail or Valet for unusual setups.** If you need a service Herd doesn't ship, you're back to running it yourself anyway.
- **Newer and less battle-tested** than Valet (which has been around since 2016). This gap shrinks every release.

## How I'd actually choose

**Pick Sail if:**

- Your team's CI runs in Docker.
- You ship to Docker in production.
- You work on a team where "works on my machine" matters to more than just you.
- You're running Laravel as part of a polyglot stack (a Go service, a Python ML pipeline) that's already in compose.

**Pick Valet if:**

- You're a freelancer who flips between five tiny client projects a day.
- Battery life matters (writing on a plane, off-grid coding, etc.).
- You want the fastest possible dev loop and you're comfortable installing services yourself.
- You already have a setup that works and you're not interested in changing it.

**Pick Herd if:**

- You'd rather not learn the CLI.
- You like polished GUIs and don't mind a small monthly subscription.
- You're new to Laravel and want the lowest-friction start.
- You've outgrown Valet but don't need the Docker firepower of Sail.

## A note on running them together

You can mix. I've seen teams run Herd for personal projects and Sail for the work codebase. It's fine, with two caveats:

- **Don't fight over port 80.** Herd takes 80 by default; Sail's reverse proxy wants 80 too. Pick one for a TLD and let the other use a different one.
- **Don't fight over a TLD.** If Herd has `.test`, give Sail Manager `.sail` (or vice versa). Both let you change the TLD in settings.

## A note on Docker on macOS

If you go the Sail route, **install OrbStack.** It's a drop-in replacement for Docker Desktop with much better filesystem performance and idle resource usage on Mac. Download is free for personal use.

I switched a year ago and the dev experience improved across every Sail project I have. No config changes; it just works.

## The honest answer

There's no winner. There's only the right tool for your situation. If you ship to Docker, Sail's parity is worth real money in saved debugging time. If you ship to a managed PHP host or care about battery life, Valet or Herd will probably make you happier.

If you're using Sail and feeling the multi-project pain, [Sail Manager](/) is the gap-filler I built for that exact problem. It doesn't replace Sail; it just handles the bookkeeping (ports, `.test` domains, auto-commands) so you can run ten projects at once without thinking.
