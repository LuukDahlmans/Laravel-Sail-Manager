# Contributing to Sail Manager

Thanks for stopping by. This file covers what you need to set up a dev environment and the conventions the codebase follows.

## Setup

You need macOS 12 or newer, Rust stable (`rustup install stable`), Node.js 20+, and Docker Desktop.

```sh
git clone https://github.com/LuukDahlmans/Laravel-Sail-Manager.git
cd Laravel-Sail-Manager
npm install
npm run tauri dev
```

The first `cargo` compile takes 3 to 6 minutes. After that, dev runs rebuild in seconds.

## Layout

`src-tauri/` is the Rust backend. Each module has one job: `store.rs` for SQLite, `scaffolder.rs` for `laravel new`, `proxy.rs` for Traefik, `dnsmasq.rs` for the DNS forwarder, `resolver.rs` for `/etc/resolver/<tld>`, `tls.rs` for the local CA, and so on. All Tauri command handlers live in `commands.rs`. `lib.rs` wires it together. Models go camelCase across the wire (`#[serde(rename_all = "camelCase")]`).

`src/` is the SvelteKit + Svelte 5 frontend. The singleton `projectStore` in `lib/projects.svelte.ts` owns reactive state and is the only thing that calls `invoke()`. Components subscribe to it. UI runs in dark, light, or system theme via `[data-theme]` on `<html>`.

When in doubt, follow the patterns of the existing module nearest your change.

## Tests

```sh
cd src-tauri && cargo test
```

88 unit tests covering the high-value pure logic: `transform_sail_command`, the port allocator, `.env` parsing, `apply_env_overrides`, validators, settings serde. Add tests for new pure functions you introduce. Async, Tauri-runtime, and Docker-coupled paths are intentionally not unit-tested; verifying those needs the real app running.

```sh
npx svelte-check --tsconfig ./tsconfig.json
```

Frontend type-check. Always passes with zero errors before a PR.

## Code conventions

### Rust

- Command handlers return `AppResult<T>` (defined in `error.rs`).
- Errors that surface to the user use `AppError::Other(String)` for human-readable messages.
- New shell-outs go through `tokio::process::Command` with explicit `args(...)`. Never construct shell strings from user input.
- Anything calling `osascript with administrator privileges` validates its inputs. No string concatenation of user input into the shell command body.
- New SQLite migrations go in the `SCHEMA` const in `store.rs` using `CREATE TABLE IF NOT EXISTS` so they're idempotent.

### Svelte / TypeScript

- Svelte 5 runes only: `$state`, `$derived`, `$derived.by`, `$effect`, `$props`, `$bindable`. No legacy `$:` reactive statements.
- All reactive state lives on `projectStore`. Components read from it; they don't fetch directly.
- Types for command payloads live in `lib/types.ts`. Mirror the Rust struct field names exactly (camelCase from the serde rename).
- Use `data-tauri-drag-region` on draggable chrome. The `-webkit-app-region: drag` CSS doesn't work in Tauri 2.
- For destructive confirmations, use `ConfirmModal.svelte`. The Tauri webview swallows `window.confirm/alert/prompt` unreliably.
- Errors: assigning `projectStore.error = '...'` automatically fires a toast via the bridge in `+layout.svelte`.

### General

- No emojis in code or UI unless explicitly requested.
- Comments are sparse. Add them when the behaviour would surprise a reader, not to describe what the code already says.

## What's worth working on

Concrete high-impact items:

**Linux and Windows builds.** Tauri supports both. The Mac-specific bits to swap out: PATH augmentation in `lib.rs`, the `getCurrentWindow().setTheme()` call, and the `osascript` resolver writer. PATH is fine on Linux/Windows shells. Theme on Linux uses `dbus`/desktop settings. Linux uses `/etc/hosts` or NetworkManager dispatcher rather than `/etc/resolver`.

**Code-signing and notarization** in CI for macOS. Apple Developer ID (\$99/year) and `notarytool` configuration via repo secrets. See [`RELEASE.md`](RELEASE.md) for the checklist.

**Auto-updater** via `tauri-plugin-updater` reading from GitHub Releases. The plumbing's already there, it needs the signed builds to consume.

**Database table browser.** Proxy `docker compose exec mysql mysql -e "SHOW TABLES"` and friends into a read-only viewer to reduce trips to TablePlus.

**Container shell as a tab.** Embed xterm.js plus a Tauri command that wires a PTY to `docker compose exec laravel.test bash`. Replaces "open in Terminal" for most cases.

## Before opening a PR

1. `cd src-tauri && cargo check` clean
2. `cd src-tauri && cargo test` clean
3. `npx svelte-check --tsconfig ./tsconfig.json` clean (zero errors)
4. `npm run build` clean
5. Run the app via `npm run tauri dev` and use the feature you changed

## Commit and PR style

Conventional Commits aren't required, but commit messages should describe the *why*, not just the *what*. "Fix port allocation race" is fine. "Updates" isn't.

One feature or fix per PR. Smaller PRs get reviewed faster.

Reference any related issue in the PR description.

## Reporting bugs

Open an issue using the bug-report template. Include macOS version (`sw_vers`), Docker Desktop version, and the contents of `~/Library/Application Support/com.laraveldevtool.app/settings.json`. No secrets are stored there. If you can reproduce the bug after a *Reset application*, that's gold.

## Code of conduct

Be respectful. We're all here to make a useful thing better. Abuse, harassment, or hostility gets a permanent ban. First warnings exist for non-malicious miscommunications.
