# Changelog

All notable changes to Sail Manager. Format roughly follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- HTTPS for `.sail` URLs. Sail Manager generates a local Certificate Authority, trusts it in your login keychain, and issues a wildcard cert that includes every project hostname as an explicit SAN. Auto-regenerates when you add or remove a project. Toggle in Settings or during the welcome wizard.
- Editable projects folder during the welcome wizard. Choose where Sail Manager scaffolds new projects up front instead of accepting `~/SailProjects` and changing it later. Persists across restarts.
- "Show some love" step in the welcome wizard with a one-click *Star on GitHub* button. Skippable.
- System check step in the welcome wizard. Probes Docker, Git, PHP, Composer, Node, and the Laravel installer with versions, and offers an *Install* link for anything missing. Optional tools collapsed under a toggle so the step focuses on Docker.
- Splash screen on app launch. Branded, animated, dismisses once `init()` completes plus a 600 ms minimum-display so it doesn't strobe on fast machines.
- Auto-start newly created projects. After scaffolding finishes, the project starts in the background and the user lands on the detail view already in *starting* state.
- Website and Docs links in the sidebar footer.

### Changed

- Welcome wizard step 1 stripped down. The bulleted feature list is gone; the hero is logo, title, one-line subtitle, and a single primary CTA.
- Welcome wizard step 4 (Local URLs) replaces the inline `<code>` URL examples with a visual before/after URL chip preview that updates live as you type the TLD.
- Final welcome step ("You're ready") simplified. One *Launch application* button instead of two ready cards.
- Orphan-import toast no longer fires during onboarding. Gated on `firstRunCompleted`. Re-runs once the wizard is dismissed.
- Create-project modal layout fixed so the Create button stays visible regardless of how tall the services list grows. Header pinned, form scrolls, footer sticks to the bottom of the modal.
- HTTPS toggle in the welcome wizard defaults to *on*; users can flip it off before pressing Enable & continue.

### Fixed

- Orphan import failed with `host port 8000 already used by another project in this app` when two projects shared the default `APP_PORT=8000`. Import now reallocates the colliding ports automatically and rewrites the orphan's `.env` so Sail picks them up.
- Wildcard cert validity dates were set to rcgen 0.13's defaults (1975 to 4096). Both Chrome and Safari rejected the cert as suspicious. Now uses sensible 1-year validity for the wildcard and 10 years for the CA.
- Wildcard cert relied on a `*.sail` SAN, which Chrome silently rejects as too broad for single-label TLDs. Cert now also includes each project's full hostname as an explicit SAN, regenerated on project create/delete.

## [0.1.0] — 2026-05-07

Initial public release.

### Project lifecycle

- Scaffold a fresh Laravel + Sail project via `laravel new` running inside the `laravelsail/php-composer` image. No local PHP required.
- Clone from Git with auto-Sail-install if missing.
- Import existing Sail folder (validates compose and `.env`, allocates ports).
- Delete with optional folder removal.

### Per-project features

- Auto-allocated host ports for all 13 Sail services plus a custom-services free-text field.
- Start and Stop with auto-migrate on first boot.
- Auto-commands with `service` (background) and `once` (blocking) modes; tabbed per-command output.
- Curated preset library (Horizon, Queue, Schedule, Reverb, Pail, Pulse, Vite dev).
- Live `docker compose logs -f` with per-service filter.
- One-shot command runner with quick-add chips.
- Resource usage panel (CPU, RAM, network per container).
- Project history (created, started, stopped, errored, imported, cloned).
- Quick-open: browser, Mailpit, TablePlus, Terminal, Finder, configured editor (PhpStorm, VS Code, Cursor, Zed).

### App-level

- Local `*.sail` URLs via dnsmasq, `/etc/resolver/sail`, and Traefik on `:80`. One admin prompt total.
- macOS menu-bar tray with per-project submenu.
- First-run welcome wizard.
- Light, dark, and system theme syncing with macOS native window appearance.
- Live Docker engine status and *Start Docker* button (uses `docker desktop start --detach`).
- Cmd+K command palette.
- Project Templates with seeded defaults.
- Toast notifications with action buttons.
- Self-healing local URLs on launch with a sticky "Fix it" repair toast.
- *Reset application* danger zone in Settings (wipes app state, keeps project folders).
- Bulk Start all / Stop all on the project list.
- Search and git status indicator (branch, dirty/clean, ahead/behind) on each card.

### Developer infrastructure

- Tauri 2 + SvelteKit + Svelte 5 (runes).
- 88 Rust unit tests covering pure logic.
- macOS DMG bundle with custom branded background and rounded waves icon.

### Notes

- macOS only at this release. Linux and Windows builds are technically supported by Tauri but not validated.
- DMG is unsigned. First-run requires right-click → Open.
- No telemetry, no analytics, no crash reporting.
