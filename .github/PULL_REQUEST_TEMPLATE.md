## What changed

<!-- One paragraph describing the change. -->

## Why

<!-- The motivation. Reference any issue: Closes #N. -->

## How to verify

<!-- Steps a reviewer can run locally. -->

## Checklist

- [ ] `cd src-tauri && cargo check` passes
- [ ] `cd src-tauri && cargo test` passes
- [ ] `npx svelte-check --tsconfig ./tsconfig.json` passes (0 errors)
- [ ] `npm run build` passes
- [ ] Manually verified the change in `npm run tauri dev`
- [ ] Updated [`CHANGELOG.md`](../CHANGELOG.md) under `[Unreleased]`
- [ ] No new external network calls without an explicit consent path
- [ ] No new emojis introduced into code or UI
