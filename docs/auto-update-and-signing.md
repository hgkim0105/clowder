# Auto-update + Code signing — implementation plan

Status: **planned, not implemented**. Owner: hgkim0105. Target version: `0.2.0`.

## Goals

1. **Silent auto-update (UX option A)** — Clowder checks for new releases at startup and every ~6 h, downloads in the background, installs on next launch. No prompts, no UI nag.
2. **Code-signed builds on both platforms** — eliminate macOS Gatekeeper "unidentified developer" warning and Windows SmartScreen reputation prompt for end users.

Both ship together because (a) auto-update without signing is fragile on macOS (Gatekeeper may quarantine updates), and (b) we want one cutover release rather than two trust resets.

## Non-goals (for this iteration)

- Linux support / AppImage updater. Decide separately before 1.0.
- In-app "update available" UI / changelog viewer. Silent install means we don't need it for v0.2.0; can add later if user feedback wants it.
- Roll-back / staged rollouts. GitHub Releases is fine for our scale.

## Cutover plan

| Version       | Purpose                                                                             |
|---------------|-------------------------------------------------------------------------------------|
| `0.2.0-beta.1` | First build with updater enabled + signed. Distribute manually.                     |
| `0.2.0-beta.2` | Bump-only release. Verify `beta.1 → beta.2` auto-update works on a real install. **Do not skip.** |
| `0.2.0`        | GA, only after the beta round-trip is confirmed.                                    |

**Why the round-trip matters:** the updater's public key in `0.2.0-beta.1` permanently determines what signatures it will accept. If the key is wrong / lost / mismatched, every install of `0.2.0-beta.1+` becomes a dead end and users have to reinstall manually. Verify with a second beta before any GA.

Users on `0.1.21` and earlier have no updater code, so they will need a one-time manual download of `0.2.0`. After that they're on the auto-update channel.

---

## Part 1: Auto-update (Tauri updater plugin)

### Architecture

```
GitHub Releases (existing)
    ├── Clowder_0.2.0_x64-setup.nsis.zip       (Windows installer + .sig)
    ├── Clowder_0.2.0_aarch64.app.tar.gz       (macOS bundle + .sig)
    └── latest.json                            (manifest pointing at the above, signed)
            ↑
            │ HTTPS GET on startup + every 6 h
            ↓
tauri-plugin-updater  →  if newer:  download .zip/.tar.gz, verify sig with embedded
                                    pubkey, stage for next launch
```

`latest.json` and the per-platform signed update artifacts are produced automatically by `tauri-action` in CI when `createUpdaterArtifacts: true` is set and signing env vars are present.

### Steps

1. **Generate updater keypair** (one-time, locally, never commit):
   ```bash
   npx @tauri-apps/cli signer generate -w ~/.tauri/clowder.key
   ```
   Output: a base64-encoded private key (`clowder.key`, password-protected) and matching public key (`clowder.key.pub`).
   - Store `clowder.key` + the password in 1Password under `Clowder updater signing key`. **Losing this key permanently bricks the auto-update channel** — every existing install would have to be replaced manually.

2. **Add public key to `src-tauri/tauri.conf.json`:**
   ```json
   "plugins": {
     "updater": {
       "active": true,
       "endpoints": [
         "https://github.com/hgkim0105/clowder/releases/latest/download/latest.json"
       ],
       "dialog": false,
       "pubkey": "<contents of clowder.key.pub>",
       "windows": {
         "installMode": "passive"
       }
     }
   },
   "bundle": {
     "createUpdaterArtifacts": true,
     ...
   }
   ```
   - `dialog: false` — we manage UX in Rust, no built-in prompt.
   - `installMode: "passive"` — Windows NSIS shows a small progress bar but no clicks needed. `"quiet"` is fully silent but more error-prone. Start with passive.

3. **Add Cargo dep** in `src-tauri/Cargo.toml`:
   ```toml
   tauri-plugin-updater = "2"
   ```

4. **Register plugin and run check** in `src-tauri/src/lib.rs` (sketch, in the `tauri::Builder` setup):
   ```rust
   .plugin(tauri_plugin_updater::Builder::new().build())
   ```
   And in `setup`, spawn a tokio task:
   ```rust
   let app_for_updater = app.handle().clone();
   tauri::async_runtime::spawn(async move {
       // First check 60 s after startup so it doesn't compete with hook install / sweep
       tokio::time::sleep(Duration::from_secs(60)).await;
       loop {
           if let Err(e) = check_and_install(&app_for_updater).await {
               eprintln!("clowder: updater error: {e}");
           }
           tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
       }
   });
   ```
   `check_and_install` calls `app.updater().check()`, and on `Some(update)`, `update.download_and_install(...)`. The installer is staged for next launch — we do not force-quit.

5. **Capability** (`src-tauri/capabilities/default.json`):
   ```json
   "updater:default"
   ```
   (The plugin's default ACL grants `check`, `download`, `install`. No window-specific scoping needed since we drive it from Rust.)

6. **CI secrets** (GitHub repo → Settings → Secrets):
   - `TAURI_SIGNING_PRIVATE_KEY` — paste full contents of `clowder.key`.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the passphrase set during `signer generate`.

7. **CI workflow** (`.github/workflows/release.yml`) — add to the `Build & release` step's `env:`:
   ```yaml
   TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
   TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
   ```
   `tauri-action` detects these and emits `*.sig` + `latest.json` automatically; no further changes needed.

### Verification (do not skip)

After `0.2.0-beta.1` is published:

1. Install `0.2.0-beta.1` on a clean Windows VM and a clean macOS box.
2. Confirm the cat appears in tray and `~/.claude/clowder/state/` is populated as normal.
3. Tag and release `0.2.0-beta.2` (just bump versions, no other changes).
4. Wait 6 h or restart Clowder; confirm in logs (`eprintln!` output) that `check()` finds the update, downloads it, and the next launch is on `0.2.0-beta.2`.
5. Only then promote to `0.2.0`.

If beta.1→beta.2 fails: do **not** ship `0.2.0`. Roll back the public key, regenerate, re-cut beta.

---

## Part 2: Code signing

### macOS

**What you need:**
- Apple Developer Program membership ($99/yr).
- A "Developer ID Application" certificate (download from developer.apple.com after enrollment, export as `.p12` with a password).
- Apple ID + app-specific password (created at appleid.apple.com → "App-Specific Passwords").
- Team ID (10-char string from developer.apple.com → membership).

**CI secrets** to add:
- `APPLE_CERTIFICATE` — base64-encoded `.p12` file (`base64 -i developer-id.p12 | pbcopy`).
- `APPLE_CERTIFICATE_PASSWORD` — password used when exporting `.p12`.
- `APPLE_SIGNING_IDENTITY` — e.g. `"Developer ID Application: HG Kim (XXXXXXXXXX)"`.
- `APPLE_ID` — your Apple ID email.
- `APPLE_PASSWORD` — the app-specific password (NOT your Apple ID password).
- `APPLE_TEAM_ID` — 10-char team ID.

`tauri-action` reads all of these automatically. The action will sign `.app`, package into `.dmg`, and run `notarytool submit` for notarization (~3-15 min wait per build). After notarization succeeds, Gatekeeper accepts the app with no warning.

**Tauri config** (`tauri.conf.json` `bundle.macOS`):
```json
"macOS": {
  "minimumSystemVersion": "10.13",
  "signingIdentity": null,           // CI provides via env
  "providerShortName": null,
  "entitlements": null
}
```

### Windows

**What you need:** a code signing certificate. Three tiers, picking matters:

| Tier            | Cost (rough) | SmartScreen behavior                              | Issuance time   |
|-----------------|--------------|---------------------------------------------------|-----------------|
| OV (standard)   | $200–400/yr  | Warning until ~3 000 downloads build reputation   | 1–3 days        |
| EV (HSM)        | $400–700/yr  | No warning from day 1                             | 3–10 days, ID verification |
| Self-signed     | $0           | Always warns; not viable for distribution         | n/a             |

**Recommendation:** OV from a budget vendor (Sectigo via SSL.com or Certum) for the first year. Cheap, gets us out of "Unknown publisher" hell. Upgrade to EV later if SmartScreen warnings are still bothering users after a few hundred downloads.

EV certificates ship on a hardware token (USB) — they cannot be exported as a file, which means they need a remote signing setup (Azure Key Vault, DigiCert KeyLocker, etc.) for CI. OV certs ship as `.pfx` and Just Work in CI.

**CI secrets** (assuming OV `.pfx`):
- `WINDOWS_CERTIFICATE` — base64-encoded `.pfx` (`certutil -encode codesign.pfx tmp.b64`).
- `WINDOWS_CERTIFICATE_PASSWORD` — password set when exporting.

**Tauri config** (`tauri.conf.json` `bundle.windows`):
```json
"windows": {
  "certificateThumbprint": null,   // CI uses signtool with the .pfx instead
  "digestAlgorithm": "sha256",
  "timestampUrl": "http://timestamp.digicert.com",
  "nsis": {
    "installerHooks": "installer.nsh"
  }
}
```

**CI workflow** — add a signing step before the `tauri-action` step on the Windows leg, OR use `tauri-action`'s built-in support by passing the right env vars (`TAURI_PRIVATE_KEY` etc. — confirm latest tauri-action docs at implementation time, the API has shifted).

### Verification

- macOS: download the `.dmg` from the draft release, run `spctl -a -vvv -t install Clowder.dmg`. Should report `accepted source=Notarized Developer ID`.
- Windows: download the installer, run it on a clean Win11 VM. SmartScreen should not show "Unknown publisher". `signtool verify /pa Clowder_0.2.0_x64-setup.exe` should report success with a chain to a trusted root.

---

## Risks and how we mitigate

| Risk                                                 | Mitigation                                                                |
|------------------------------------------------------|---------------------------------------------------------------------------|
| Updater signing key lost                             | 1Password backup + a second copy on a personal encrypted drive. Treat as critical creds. |
| First public-key release is wrong → bricks channel   | Mandatory beta.1→beta.2 round-trip before GA.                             |
| Apple cert expires (1-year cycle)                    | Calendar reminder 30 days before expiry. Renewal is straightforward; existing notarized binaries keep working. |
| Windows cert expires                                 | Same. Existing signed binaries with timestamped sigs stay valid forever; only future builds need a new cert. |
| Auto-update during a Claude Code session            | NSIS `installMode: "passive"` waits for app exit. macOS tar update applies on next launch. We never force-quit. |
| User explicitly disabled autostart, missed updates  | Check on every manual launch too — currently the design. Fine.            |
| `latest.json` URL changes (GitHub-side)              | Pin to versioned redirect: `releases/latest/download/...`. GitHub commits to that path.   |

---

## Pre-implementation checklist (do before opening the PR)

- [ ] Confirm `started_at` timestamp format on macOS by reading a real `~/.claude/sessions/<pid>.json` (the boot-time PID-reuse guard depends on it being epoch ms; only verified on Windows so far).
- [ ] Decide Linux: skip, or add AppImage + updater target now.
- [ ] Decide Windows cert vendor (OV vs EV) and budget approval.
- [ ] Apple Developer Program enrolled.
- [ ] Updater keypair generated, backed up.
- [ ] Decide whether to keep `0.2.0-beta.*` releases as draft or pre-release (pre-release is more discoverable for testers; draft is private to the maintainer).

## Out of scope but worth tracking for 1.0

- Side-mounted Windows taskbar handling for popup/bubble anchoring (currently falls back to "above", which is wrong but on-screen).
- First-run onboarding flow (where to look for the tray icon, "pin me to the visible tray" hint on Windows).
- Telemetry / opt-in error reporting so we hear about edge cases without users having to file issues.
