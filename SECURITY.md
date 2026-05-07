# Security

## Threat model

Sail Manager is a local developer tool that orchestrates the user's own Docker daemon. There's no remote server, no auth boundary, no network calls beyond `docker pull` and `git clone` operations the user explicitly initiates. The threat model is a hostile workstation environment, not a multi-tenant service.

## What's protected

**No shell injection from user input.** Every shell-out uses `tokio::process::Command::args(...)` with explicit argv. No string concatenation into a shell command line. Project names, paths, URLs, branches, etc. all pass as separate arguments.

**Parameterized SQL throughout.** SQLite queries use `params![...]`. No string concatenation.

**Validated input where it matters.** TLDs (lowercase alphanumeric and hyphens, 2 to 32 chars), project names (must start with a letter, alphanumeric and hyphens, 40 chars max), and path arguments are validated before use in commands that elevate privileges.

**Minimal Tauri capabilities.** `core:default` plus window allow-start-dragging, window allow-set-theme, opener:default, and dialog:default. The webview can't reach into the filesystem or run arbitrary commands.

**Admin-elevated operations are tightly scoped.** The only `osascript … with administrator privileges` invocations write `/etc/resolver/<tld>` (via `/bin/cp` to a fixed path) and update `/etc/hosts` (via a fixed `sed` pattern). No user-supplied data goes into the shell body unsanitized. The local CA install uses `security add-trusted-cert` against the user's own login keychain, no admin elevation.

## What to know about

**Auto-commands run user-typed shell inside the user's Laravel container.** This is a power-user feature. Anyone who can write to the `auto_commands` table (the user, or anything that can modify `~/Library/Application Support/com.laraveldevtool.app/state.db`) can run arbitrary commands as the container's `sail` user the next time the project starts. There's no template-sharing or remote-templates feature today. If one's ever added, it'll need sandboxing or explicit per-command consent.

**DMG distribution is currently unsigned.** Until the Apple Developer Program enrollment lands and `notarytool` is wired into CI, downloaded DMGs trigger Gatekeeper warnings and require right-click → Open. Don't accept a Sail Manager DMG from anywhere other than this repo's official Releases page.

**Secrets in Laravel's `.env`** (API keys, DB passwords, etc.) are read by the import flow and shown in the Environment tab. They don't get transmitted anywhere; they live on disk in the project folder as Laravel manages them. The app's own settings and SQLite files don't store anything sensitive.

**HTTPS for `.sail` URLs** generates a local CA stored in `~/Library/Application Support/com.laraveldevtool.app/tls/`. The CA private key is on disk with the same permissions as your home directory (user-readable). Anyone with read access to your account can sign certs that browsers on your machine trust. Standard threat model for a local dev cert authority. If your account is compromised, the local CA is the least of your worries.

**PATH augmentation on macOS** prepends `/opt/homebrew/bin`, `/usr/local/bin`, and a few related paths so a packaged `.app` can find `docker`, `git`, and friends. If a malicious binary lives at one of those paths, we'd run it. At that point a much bigger compromise has already happened. These paths are the standard contract Homebrew users opt into.

## Reporting a vulnerability

If you find something that lets the app do more than intended on the host (command injection through a project name, escape from the resolver-write sandbox, anything that elevates beyond what was authorized, etc.), please **email the maintainer privately** rather than opening a public issue. Contact via [GitHub profile](https://github.com/luukdahlmans).

I'll acknowledge receipt within 7 days, fix and ship within 30 days for anything verified, and credit you in the changelog if you'd like.

## Known limitations

These aren't security holes per se, but they're worth knowing:

- The auto-commands log buffer caps at 800 lines per command in memory. A chatty crash loop won't OOM the app.
- Docker engine state is polled every 5 seconds; pause and resume are reflected within that window.
- The `laravelsail/php-composer` and `4km3/dnsmasq` images we pull aren't signature-verified by Sail Manager. Docker handles signature verification when you've configured it for your daemon.
