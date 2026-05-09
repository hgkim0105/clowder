# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Clowder

Clowder is a cross-platform menu/tray bar app (Tauri + React) for macOS and Windows that displays an animated pixel-art cat in the system tray / notification area. Left-clicking the tray icon opens a popup panel showing all active Claude Code sessions with stats. Right-clicking shows an "About Clowder" / "Quit Clowder" menu. The cat animates based on session activity.

## Commands

```bash
# Frontend dev server only (Vite, port 5174)
npm run dev

# Run the full Tauri app in dev mode (preferred)
npm run tauri dev

# Build production app
npm run tauri build

# TypeScript check + Vite build (frontend only)
npm run build
```

```bash
# Rust unit tests (covers hook_install: fresh install, idempotency, BOM, foreign-entry preservation)
cd src-tauri && cargo test
```

No frontend test or lint commands are configured.

## Architecture

**Data flow:**

```
~/.claude/sessions/*.json           (Claude session metadata)
~/.claude/clowder/state/*.json      (per-session state: idle/thinking/working/done)
~/.claude/projects/**/<id>.jsonl    (conversation history — model, tokens, speed, permissionMode)
        ↓
watcher.rs  — watches both state dirs via notify crate (300–500ms poll)
        ↓
lib.rs      — updates SessionMap + StateMap (Arc<Mutex>), emits "sessions-update" Tauri event
        ↓
animation_loop (tokio task)  — reads maps every 50ms, computes dominant state,
                               advances sprite frame, calls tray.set_icon()
                               detects idle/working → done transition → trigger_bubble()
        ↓
macOS NSStatusItem / Windows notification area  — displays animated cat in tray

left-click on tray icon
        ↓
webview popup "main" (320×480)  — shows session list via get_sessions IPC command
                                   reads stats from JSONL (last 64KB) on each call

session completes (done transition)
        ↓
webview bubble "bubble" (260×80–116)  — speech bubble below (macOS) / above (Windows) tray icon
                                        auto-hides after 4s, suppressed if popup is open

right-click on tray icon → "About Clowder"
        ↓
webview about "about" (360×360, centered)  — frosted-glass dialog with version, repo link
                                              dismissed on Esc or × button
```

**Tray icon states:**

| State   | Trigger                                                    | Duration         |
|---------|------------------------------------------------------------|------------------|
| idle    | all sessions idle (or only stale done)                     | persistent       |
| working | any session non-idle and non-done                          | while active     |
| done    | any fresh done (within `DONE_FRESHNESS_SECS`), no working  | 4 s then → idle  |

**Key design decisions:**
- Three Tauri webview windows share the same JS bundle; differentiated by `getCurrentWebviewWindow().label` in `main.tsx` (`"main"` → `<App>`, `"bubble"` → `<Bubble>`, `"about"` → `<About>`; default also routes to `<App>`)
- All three windows are hidden at startup; "main" shown/hidden on tray left-click, "bubble" shown on done transition, "about" shown from the tray right-click menu
- `main.tsx` tags `<html data-platform="windows|macos">` from the user agent so CSS can adjust popup anchoring (Windows tray sits at the bottom of the screen, so the popup card sticks to the bottom of its fixed-height transparent window — see `src/App.css`)
- `ActivationPolicy::Accessory` (macOS only) removes Dock icon and app switcher entry
- Windows hide automatically via `WindowEvent::Focused(false)` in Rust (more reliable than JS blur)
- Bubble uses `orderFrontRegardless` at `NSStatusBarWindowLevel` (25) on macOS via `objc`, and `ShowWindow(SW_SHOWNOACTIVATE)` + `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE)` on Windows via `windows` crate, so it appears without stealing keyboard focus from the active app. The shared `show_without_focus` helper in `lib.rs` dispatches per-OS.
- Working FPS scales with active session count: 1→12fps, 2→16fps, 3+→20fps
- `make_icon` auto-crops transparent padding then scales to `TRAY_ICON_SIZE` (64 on macOS, 72 on Windows — Windows taskbar draws at 16/24/32 logical px and the slightly oversized source keeps pixel art crisp under nearest-neighbor downscaling)
- Stats (model, tokens, speed, permissionMode) are read from JSONL on each `get_sessions` call; not cached to avoid stale data
- `emit_update` events carry no stats (lightweight); popup always calls `get_sessions` on events
- Session ID mismatch handled via `find_active_session_id`: `~/.claude/sessions/<pid>.json` stores initial UUID, but hooks write state with current conversation UUID; the most recently modified JSONL in the project dir resolves the active ID
- `find_recent_state` picks the freshest state between direct UUID lookup and JSONL-based fallback
- **Ghost-session pruning**: `~/.claude/sessions/<pid>.json` is not cleaned up when Claude Code dies abnormally (system reboot, kill -9, crash), so stale files would surface as "valid sessions" in Clowder until the next Claude launch wiped them. `is_session_dead()` combines two checks: `pid_alive()` (per-OS — `OpenProcess` + `GetExitCodeProcess` on Windows, `kill(pid, 0)` on Unix) AND `info.started_at < boot_time_ms()` to defeat PID reuse (after a reboot the OS may reassign a dead Claude PID to an unrelated live process; the boot-time check rejects any session that started before this boot regardless). `load_sessions` filters dead entries out of the in-memory map, `prune_dead_session_files()` physically deletes the JSON files at startup, and a 30 s tokio sweep task in `run()` re-runs both — necessary because the file watcher only fires on actual file events, so a Claude crash that doesn't touch `~/.claude/sessions/` would leave a ghost forever. `boot_time_ms()` uses `GetTickCount64` on Windows, `/proc/uptime` on Linux, `sysctl kern.boottime` on macOS; returns `None` elsewhere and the boot check is skipped.
- **Orphan state pruning**: a crashed Claude Code can leave its `~/.claude/clowder/state/<id>.json` showing `"working"` forever, which would keep `animation_loop` reading it as a live worker and the cat would animate working indefinitely. Whenever `session_map` changes (watcher reload + periodic sweep), `compute_live_state_ids()` builds the set of state ids legitimately owned by live sessions (each session's own id + the JSONL-resolved active id for sole-occupant cwds, mirroring `find_recent_state`'s policy), and `state_map.retain()` drops the rest. `handle_state_event` also rejects writes for ids outside that set, so a stale state file touch can't re-pollute the map.
- **Stale-done decay (`DONE_FRESHNESS_SECS = 60`)**: a session's `done` is meaningful only briefly. After the threshold, `effective_state()` reports it as `idle` for the popup, `trigger_bubble` excludes it, and `animation_loop`'s `raw_display_from()` ignores it for tray icon and bubble triggering. This avoids a stale "green dot" on long-idle Claude processes still sitting at their last Stop hook.
- **Bubble cwd dedupe**: `trigger_bubble` collapses multiple done sessions sharing the same `cwd` to a single row (the most recently updated), so two Claude instances in the same project don't render as duplicate-looking rows.
- **Dynamic context window**: `context_window_for_model()` returns 1 000 000 for Claude 4.x families (`opus-4*`, `sonnet-4*`, `haiku-4*`) and 200 000 otherwise; falls back to 1M if observed input already exceeds the picked value. The context bar in the popup uses this so the percentage stays meaningful past 200k.
- **Position anchoring**: `anchor_y()` flips the popup/bubble between *below* (macOS, menu bar at top) and *above* (Windows, taskbar at bottom) the tray rect. Side-mounted Windows taskbars aren't yet handled — they'll fall back to "above", which keeps the window on-screen but mis-aligned.
- **Hooks auto-install on every app start (`hook_install::ensure_hooks_installed`)**: writes `~/.claude/clowder/hooks/{thinking,working,done}.py` (idempotent — only rewrites if body changed; chmods 755 on Unix) and merges `UserPromptSubmit`/`PreToolUse`/`PostToolUse`/`Stop` entries into `~/.claude/settings.json`. Pre-existing non-clowder hook entries are preserved; old clowder entries (matched by path containing `clowder/hooks` or `clowder\hooks`) are stripped before re-adding so reinstalls and path-format migrations stay idempotent. The Windows command is wrapped as `python "<path>"` (Windows ignores shebangs); macOS/Linux use the raw path. **The settings reader strips a leading UTF-8 BOM** because PowerShell 5.1's `Set-Content -Encoding utf8` writes one, and `serde_json` rejects it as a JSON prefix — without this the merge silently no-ops on Windows-edited settings files.
- **Autostart on first run**: `tauri-plugin-autostart` is initialized with `MacosLauncher::LaunchAgent`, and `setup` calls `autolaunch().enable()` if not already enabled. macOS gets a LaunchAgent plist; Windows gets a `HKCU\…\Run` registry entry. Re-enable is a no-op so users who manually disable it stay disabled.
- **Tauri 2 capability file (`src-tauri/capabilities/default.json`) is required.** Without `core:event:allow-listen` granted to all three windows (`main`, `bubble`, `about`), frontend `listen()` calls silently reject and webviews never receive their events. The capability also grants `opener:allow-open-url` scoped to `https://github.com/*` (the About dialog's GitHub links use `tauri-plugin-opener`'s `openUrl` to escape the webview into the OS browser; widening the allowlist is a security decision). Treat this file as load-bearing.
- **Tray right-click menu** (`tauri::menu::Menu`) holds `About Clowder` → `show_about_dialog()` (which calls `win.show()` + `set_focus()` on the "about" webview) and `Quit Clowder` → `app.exit(0)`. There is no Quit button inside the popup itself — quitting is exclusively from the tray menu.

**Sprite rows:**

| State   | Row | Frames | FPS          |
|---------|-----|--------|--------------|
| idle    | 0   | 4      | 6            |
| working | 4   | 8      | 12 / 16 / 20 |
| done    | 6   | 4      | 8            |

**Source layout:**
- `src-tauri/src/lib.rs` — tray setup, popup/bubble/about toggle, tray menu, session/state management, JSONL stats parsing, animation loop, `find_active_session_id`, `find_recent_state`, `effective_state`, `context_window_for_model`, `trigger_bubble`, `show_without_focus` (per-OS), `anchor_y`, `show_about_dialog`
- `src-tauri/src/watcher.rs` — file system watching for session and state directories
- `src-tauri/src/hook_install.rs` — idempotent on-startup install of Claude Code hooks into `~/.claude/clowder/hooks/` + merge into `~/.claude/settings.json` (handles BOM, preserves unrelated entries, replaces stale clowder paths). Has unit tests covering fresh install, idempotency, foreign-entry preservation, path replacement, and BOM handling.
- `src-tauri/src/notify_icon_cleanup.rs` — Windows-only: prunes orphan `HKCU\Control Panel\NotifyIconSettings\<id>` subkeys whose `clowder.exe` `ExecutablePath` no longer resolves; resolves Known-Folder GUID prefixes via `SHGetKnownFolderPath`; self-protects via `current_exe()`. No-op on non-Windows.
- `src-tauri/installer.nsh` — NSIS `NSIS_HOOK_POSTUNINSTALL` that wipes any `NotifyIconSettings` subkey ending with `\clowder.exe` during uninstall.
- `src-tauri/capabilities/default.json` — Tauri 2 capability granting `core:event:*`, autostart, `opener:allow-open-url` (scoped to github.com), and window/app permissions to `main`/`bubble`/`about` (mandatory; see Key design decisions)
- `src-tauri/tauri.conf.json` — three windows declared: `main` (320×480, default label), `bubble` (260×80, transparent, `alwaysOnTop`), `about` (360×360, centered); NSIS `installerHooks: "installer.nsh"` for the registry cleanup
- `src/App.tsx` — popup UI: session list, stats badges, context bar (no quit button — quit lives in tray right-click menu)
- `src/About.tsx` + `src/About.css` — frosted-glass About dialog: app icon, version (via `@tauri-apps/api/app#getVersion`), credit, GitHub repo + author links opened via `@tauri-apps/plugin-opener`'s `openUrl` (so navigation goes to the OS browser, not into the webview); Esc + × button dismiss
- `src/Bubble.tsx` — speech bubble UI: completion notification with cwd, model, token count
- `src/Bubble.css` — bubble styles: frosted glass, CSS triangle arrow
- `src/hooks/useSessions.ts` — IPC bridge: calls `get_sessions` on load, events, and every 10s
- `src/main.tsx` — entry point: routes to `<App>` / `<Bubble>` / `<About>` by window label, sets `data-platform` on `<html>` for CSS branching
- `src/types.ts` — shared TypeScript types

## Popup Panel

Left-clicking the tray icon opens a 320×480 frosted-glass popup, anchored below the tray icon on macOS and above it on Windows. Each session row shows:
- Working directory (truncated to last 2 components if deep)
- State indicator: orange pulse (working) / green dot (done · elapsed) / gray (idle). A `done` older than `DONE_FRESHNESS_SECS` (60 s) renders as `idle`.
- Badges: model name, speed (if non-standard), "Thinking" (if extended thinking), permission mode
- Context bar: token usage vs the model's context window (1M for Claude 4.x, 200k otherwise), color-coded by fill level

The popup closes when it loses focus. There is no in-popup Quit button — Quit and About both live in the tray right-click menu.

## About Dialog

Right-clicking the tray icon and selecting "About Clowder" shows a 360×360 centered frosted-glass dialog. It displays the app icon, name, version (read from `tauri.conf.json` via `@tauri-apps/api/app#getVersion`), tagline, build credit, and a GitHub repo link. Links are intercepted (`e.preventDefault()`) and opened in the OS default browser via `tauri-plugin-opener`'s `openUrl` — this works only because the capability allows `https://github.com/*`. The dialog dismisses on Esc, the × button, or by losing focus is *not* used here (the window is `alwaysOnTop: false` and intentionally persistent until explicitly closed). Drag region is the top strip; the rest of the card is interactive.

## Speech Bubble

When a session transitions to done, a 260-wide frosted-glass speech bubble appears next to the tray icon (below on macOS, above on Windows) for 4 seconds. It shows up to 2 freshly completed sessions (one row per `cwd` — duplicates collapsed to most recent) with a ✓ checkmark, working directory, model name, and total token count. The bubble is suppressed if the popup panel is already open. Height adjusts: 80 px (1 row) or 116 px (2 rows). The bubble window never steals keyboard focus.

Done states older than `DONE_FRESHNESS_SECS` (60 s) are excluded from the bubble payload and from the `working → done` transition detector, so bringing up a stale done state doesn't replay an old notification.

## Runtime File Paths

| Path | Purpose |
|------|---------|
| `~/.claude/sessions/*.json` | Claude Code session metadata (read-only) |
| `~/.claude/clowder/state/*.json` | Per-session cat state written by Claude hooks |
| `~/.claude/clowder/hooks/{thinking,working,done}.py` | Hook scripts written/refreshed by `hook_install` on every app start |
| `~/.claude/settings.json` | Hook registration is merged here (idempotent; preserves unrelated entries; BOM-tolerant) |
| `~/.claude/projects/**/<session-id>.jsonl` | Conversation history — source of model/token/speed/permissionMode |

## Tauri / macOS Notes

- `macOSPrivateApi: true` is required in `tauri.conf.json`
- The Rust crate produces both `staticlib` and `cdylib` (required by Tauri)
- `tray-icon` and `image-png` features must be enabled on the `tauri` crate
- The `image` crate (0.25, png feature) is used to decode the sprite sheet at startup via `include_bytes!`
- Tray left-click uses `show_menu_on_left_click(false)` + `on_tray_icon_event` to toggle the popup; right-click shows the system menu (`About Clowder` + `Quit Clowder`)
- Popup and bubble positions are computed from the tray icon's physical rect, centered horizontally below the menu bar
- Tray rect is stored in `TrayRectState = Arc<Mutex<Option<(f64, f64, f64, f64)>>>`, updated on TrayIconEvent::Enter/Move
- `objc` crate (0.2) is used to call `[NSWindow orderFront:]` directly so the bubble appears without activating the app
- Tauri 2 enforces an explicit ACL: `src-tauri/capabilities/default.json` must list `main`, `bubble`, and `about` under `windows` and grant at least `core:default`, `core:event:default`, `core:event:allow-listen`, `core:event:allow-emit`, `core:event:allow-unlisten`. Missing the listen permission causes the bubble's `listen("show-bubble", …)` to silently reject and the bubble window renders empty/transparent. The About dialog additionally needs `core:app:allow-version` (for `getVersion`) and `opener:allow-open-url` scoped to github.com (for the GitHub links).
- `WebviewWindow::outer_position()` can return a stale/wrong value on hidden Tauri 2 windows; the actual NSWindow position (verifiable via `CGWindowListCopyWindowInfo`) reflects what `set_position()` set. Don't rely on `outer_position()` for verification while the window is hidden.
- The macOS native screenshot pipeline used by some MCP screenshot tools filters out borderless transparent overlays even when the owning app is allowlisted; verify bubble visibility via `CGWindowListCopyWindowInfo` rather than screenshots.

## Windows Notes

- **Window anchoring & shadows**: the Windows taskbar typically sits at the bottom of the screen, so popup/bubble windows are anchored *above* the tray icon (`anchor_y` returns `tray_y - win_h - margin`). Side-mounted taskbars fall back to the same "above" math, which keeps the windows on-screen but visually misaligned. Windows DWM also draws a shadow at the full window rect even with `decorations: false`, which appears as an outline around the empty/transparent area outside each card — `setup` calls `set_shadow(false)` per-window on Windows for `main`, `bubble`, and `about` to suppress that.
- **No-activate show**: `show_without_focus()` on Windows uses `ShowWindow(SW_SHOWNOACTIVATE)` followed by `SetWindowPos(HWND_TOPMOST, …, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE)`; macOS uses `[NSWindow setLevel:25]` + `orderFrontRegardless`. Both keep keyboard focus on whatever app the user was already in.
- **Tray icon size**: `TRAY_ICON_SIZE = 72` on Windows vs `64` on macOS — Windows draws tray icons at 16/24/32 logical px and the slightly oversized source survives nearest-neighbor downscaling without muddying the pixel art.
- **CSS branching via `data-platform`**: `main.tsx` reads `navigator.userAgent.includes("Windows")` and sets `<html data-platform="windows|macos">`. The popup and bubble layouts use that attribute to flip vertical anchoring inside the (transparent, fixed-height) Tauri window so cards sit flush with the screen edge nearest the tray.
- **Hook command on Windows**: shebangs are ignored by Windows, so the merged `~/.claude/settings.json` entries wrap the hook path as `python "<path>"`. Forward slashes are used in the path (Windows accepts them on the command line) to keep JSON escaping simple.
- **NotifyIconSettings cleanup**: Windows 11's "Settings → Personalization → Taskbar → Other system tray icons" reads from `HKCU\Control Panel\NotifyIconSettings\<numeric-id>` (one subkey per registered tray icon, identified by `ExecutablePath`). Tauri's NSIS uninstaller does not touch this key, so install → uninstall → reinstall-to-different-path leaves the old subkey behind and the panel shows two Clowder rows. The legacy `IconStreams` / `PastIconsStream` cache wipe under `HKCU\Software\Classes\Local Settings\…\TrayNotify\` does not address this — the modern Win11 location is separate. Cleanup happens in two places: `src-tauri/installer.nsh` (`NSIS_HOOK_POSTUNINSTALL`) deletes any subkey whose `ExecutablePath` ends with `\clowder.exe` during uninstall, and `src-tauri/src/notify_icon_cleanup.rs` runs on every app start and prunes any `clowder.exe` entry whose resolved path no longer exists on disk (covers users who skipped the uninstaller).
- **`ExecutablePath` uses Known-Folder GUID prefixes, not literal paths**: Windows stores `ExecutablePath` as `REG_SZ` but for any executable under a Known Folder it substitutes a `{KF_ID}\<rest>` prefix. Common prefixes: `{6D809377-6AF0-444B-8957-A3773F02200E}` = `FOLDERID_ProgramFilesX64` (`C:\Program Files`), `{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}` = `FOLDERID_ProgramFilesX86`, `{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}` = `FOLDERID_LocalAppData`. Whichever one matches the install location, the row is stored prefixed — never as the literal drive-letter path. This shipped a bug in 0.1.9: `notify_icon_cleanup.rs` did `Path::new(&exe_path).exists()` directly on the raw value, which always returned `false`, so it deleted the live entry on every app start and the icon never persisted into the Settings UI. The runtime cleanup now resolves the KF_ID prefix via `SHGetKnownFolderPath` before the existence check, with a self-protection guard that compares against `std::env::current_exe()` so we never delete our own row regardless. The NSIS hook sidesteps the KF_ID issue by matching any subkey ending with `\clowder.exe` (during uninstall every clowder row should go anyway). Diagnose by inspecting the registry with `Get-ChildItem 'HKCU:\Control Panel\NotifyIconSettings'` and checking `ExecutablePath` values.
