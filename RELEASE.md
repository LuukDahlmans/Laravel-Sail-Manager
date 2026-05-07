# Release plan

Living checklist for shipping signed/notarized macOS builds. Pick up here once
the Apple Developer Program enrollment is approved.

## Status

- [x] DMG bundle config + retina background
- [x] Updater plugin wired in `lib.rs` + frontend check
- [x] Universal binary target in workflow
- [x] Tag-based release trigger
- [ ] **Apple Developer Program enrollment** — applied, waiting on Apple
- [ ] Developer ID Application certificate generated + exported as `.p12`
- [ ] App-specific password created at appleid.apple.com
- [ ] Tauri updater signing keypair generated
- [ ] `pubkey` placeholder in `tauri.conf.json` replaced with real key
- [ ] Eight GitHub repo secrets added (see below)
- [ ] `release.yml` rewritten to use `tauri-action` (signs, notarizes, generates `latest.json`)
- [ ] `src-tauri/entitlements.plist` created (hardened runtime requirements)
- [ ] First signed release tagged + published

## Once Apple approves

### 1. Apple Developer setup

1. **Certificates, Identifiers & Profiles** → create a **Developer ID Application** certificate.
2. Download the `.cer`, double-click into Keychain, then export from Keychain as a `.p12` (right-click the cert → Export → set a password — save it).
3. Note the **Team ID** (10-char alphanumeric, top-right of the dev portal).
4. At appleid.apple.com → Sign-In and Security → **App-Specific Passwords** → create one named "tauri-notarize". Save it; cannot be viewed again.

### 2. Tauri updater signing keys

```sh
npm run tauri signer generate -- -w ~/.tauri/sailmanager.key
```

Paste the printed pubkey into `src-tauri/tauri.conf.json`:

```json
"plugins": {
  "updater": {
    "endpoints": ["https://github.com/LuukDahlmans/Laravel-Sail-Manager/releases/latest/download/latest.json"],
    "pubkey": "<pubkey from output>",
    ...
  }
}
```

The private key file `~/.tauri/sailmanager.key` is for the workflow — **never commit it**.

### 3. GitHub repo secrets

Settings → Secrets and variables → Actions. Add all eight:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | The password set when exporting the .p12 |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` — full string from Keychain |
| `APPLE_ID` | Apple ID email |
| `APPLE_PASSWORD` | App-specific password from step 1.4 |
| `APPLE_TEAM_ID` | The 10-char Team ID |
| `TAURI_SIGNING_PRIVATE_KEY` | `cat ~/.tauri/sailmanager.key \| base64 \| pbcopy` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password set when generating the signer key (empty string if none) |

### 4. Rewrite `.github/workflows/release.yml`

Switch from `softprops/action-gh-release` to `tauri-apps/tauri-action` — it
builds, signs, notarizes, generates `latest.json`, and uploads to a draft
release in one step. About 30 lines. Also create `src-tauri/entitlements.plist`
with hardened-runtime entitlements (`com.apple.security.cs.allow-jit`,
`com.apple.security.cs.allow-unsigned-executable-memory`, etc. — the WKWebView
needs them).

### 5. The actual release flow

```sh
# bump versions in lockstep:
#   src-tauri/tauri.conf.json  -> "version"
#   src-tauri/Cargo.toml        -> version
#   package.json                -> version
# update CHANGELOG.md
git commit -am "release: v0.2.0"
git tag v0.2.0
git push origin main --tags
```

The tag triggers the workflow → universal DMG → Apple-signed → notarized →
ticket stapled → DMG + `latest.json` uploaded → draft GitHub Release. Edit
release notes, click **Publish**. Existing installs auto-update on next launch.

## Notes

- Notarization can take 1–15 minutes — the workflow blocks on it.
- If notarization fails, common causes: missing entitlements, an unsigned binary somewhere in `Frameworks/`, or `--deep` not applied. Check the Apple notary log via `xcrun notarytool log <submission-id>`.
- For a quick local-only signed build (no notarization), set the `APPLE_SIGNING_IDENTITY` env var locally and run `npm run tauri build`. Skips Apple's servers but Gatekeeper will still warn first-run users.
