# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Clowder

Clowder is a macOS menu bar app (Tauri + React) that displays an animated pixel-art cat in the system tray. Left-clicking the tray icon opens a popup panel showing all active Claude Code sessions with stats. The cat animates based on session activity.

## Commands

```bash
# Frontend dev server only (Vite, port 1420)
npm run dev

# Run the full Tauri app in dev mode (preferred)
npm run tauri dev

# Build production app
npm run tauri build

# TypeScript check + Vite build (frontend only)
npm run build
```

No test or lint commands are configured.

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
macOS NSStatusItem  — displays animated cat in menu bar

left-click on tray icon
        ↓
webview popup "main" (320×480)  — shows session list via get_sessions IPC command
                                   reads stats from JSONL (last 64KB) on each call

session completes (done transition)
        ↓
webview bubble "bubble" (260×80–116)  — speech bubble below tray icon
                                        auto-hides after 4s, suppressed if popup is open
```

**Tray icon states:**

| State   | Trigger                                                    | Duration         |
|---------|------------------------------------------------------------|------------------|
| idle    | all sessions idle (or only stale done)                     | persistent       |
| working | any session non-idle and non-done                          | while active     |
| done    | any fresh done (within `DONE_FRESHNESS_SECS`), no working  | 4 s then → idle  |

**Key design decisions:**
- Two Tauri webview windows share the same JS bundle; differentiated by `getCurrentWebviewWindow().label` in `main.tsx`
- Both windows are hidden at startup; "main" shown/hidden on tray left-click, "bubble" shown on done transition
- `ActivationPolicy::Accessory` removes Dock icon and app switcher entry
- Windows hide automatically via `WindowEvent::Focused(false)` in Rust (more reliable than JS blur)
- Bubble uses `orderFront:` (via `objc` crate) instead of `show()` to avoid stealing keyboard focus
- Working FPS scales with active session count: 1→12fps, 2→16fps, 3+→20fps
- `make_icon` auto-crops transparent padding then scales to 64×64 for sharp retina rendering
- Stats (model, tokens, speed, permissionMode) are read from JSONL on each `get_sessions` call; not cached to avoid stale data
- `emit_update` events carry no stats (lightweight); popup always calls `get_sessions` on events
- Session ID mismatch handled via `find_active_session_id`: `~/.claude/sessions/<pid>.json` stores initial UUID, but hooks write state with current conversation UUID; the most recently modified JSONL in the project dir resolves the active ID
- `find_recent_state` picks the freshest state between direct UUID lookup and JSONL-based fallback
- **Stale-done decay (`DONE_FRESHNESS_SECS = 60`)**: a session's `done` is meaningful only briefly. After the threshold, `effective_state()` reports it as `idle` for the popup, `trigger_bubble` excludes it, and `animation_loop`'s `raw_display_from()` ignores it for tray icon and bubble triggering. This avoids a stale "green dot" on long-idle Claude processes still sitting at their last Stop hook.
- **Bubble cwd dedupe**: `trigger_bubble` collapses multiple done sessions sharing the same `cwd` to a single row (the most recently updated), so two Claude instances in the same project don't render as duplicate-looking rows.
- **Dynamic context window**: `context_window_for_model()` returns 1 000 000 for Claude 4.x families (`opus-4*`, `sonnet-4*`, `haiku-4*`) and 200 000 otherwise; falls back to 1M if observed input already exceeds the picked value. The context bar in the popup uses this so the percentage stays meaningful past 200k.
- **Tauri 2 capability file (`src-tauri/capabilities/default.json`) is required.** Without `core:event:allow-listen` granted to both `main` and `bubble` windows, frontend `listen()` calls silently reject and the bubble webview never receives the `show-bubble` event — the window appears (transparent + empty) and looks like the feature is broken. Treat this file as load-bearing.

**Sprite rows:**

| State   | Row | Frames | FPS          |
|---------|-----|--------|--------------|
| idle    | 0   | 4      | 6            |
| working | 4   | 8      | 12 / 16 / 20 |
| done    | 6   | 4      | 8            |

**Source layout:**
- `src-tauri/src/lib.rs` — tray setup, popup/bubble toggle, session/state management, JSONL stats parsing, animation loop, `find_active_session_id`, `find_recent_state`, `effective_state`, `context_window_for_model`, `trigger_bubble`
- `src-tauri/src/watcher.rs` — file system watching for session and state directories
- `src-tauri/capabilities/default.json` — Tauri 2 capability granting `core:event:*` and related permissions to `main`/`bubble` (mandatory; see Key design decisions)
- `src/App.tsx` — popup UI: session list, stats badges, context bar, quit button
- `src/Bubble.tsx` — speech bubble UI: completion notification with cwd, model, token count
- `src/Bubble.css` — bubble styles: frosted glass, CSS triangle arrow
- `src/hooks/useSessions.ts` — IPC bridge: calls `get_sessions` on load, events, and every 10s
- `src/main.tsx` — entry point: routes to `<App>` or `<Bubble>` by window label
- `src/types.ts` — shared TypeScript types

## Popup Panel

Left-clicking the tray icon opens a 320×480 frosted-glass popup below the menu bar icon. Each session row shows:
- Working directory (truncated to last 2 components if deep)
- State indicator: orange pulse (working) / green dot (done · elapsed) / gray (idle). A `done` older than `DONE_FRESHNESS_SECS` (60 s) renders as `idle`.
- Badges: model name, speed (if non-standard), "Thinking" (if extended thinking), permission mode
- Context bar: token usage vs the model's context window (1M for Claude 4.x, 200k otherwise), color-coded by fill level

The popup closes when it loses focus. A "Quit Clowder" button sits in the footer.

## Speech Bubble

When a session transitions to done, a 260-wide frosted-glass speech bubble appears below the tray icon for 4 seconds. It shows up to 2 freshly completed sessions (one row per `cwd` — duplicates collapsed to most recent) with a ✓ checkmark, working directory, model name, and total token count. The bubble is suppressed if the popup panel is already open. Height adjusts: 80 px (1 row) or 116 px (2 rows). The bubble window never steals keyboard focus.

Done states older than `DONE_FRESHNESS_SECS` (60 s) are excluded from the bubble payload and from the `working → done` transition detector, so bringing up a stale done state doesn't replay an old notification.

## Runtime File Paths

| Path | Purpose |
|------|---------|
| `~/.claude/sessions/*.json` | Claude Code session metadata (read-only) |
| `~/.claude/clowder/state/*.json` | Per-session cat state written by Claude hooks |
| `~/.claude/projects/**/<session-id>.jsonl` | Conversation history — source of model/token/speed/permissionMode |

## Tauri / macOS Notes

- `macOSPrivateApi: true` is required in `tauri.conf.json`
- The Rust crate produces both `staticlib` and `cdylib` (required by Tauri)
- `tray-icon` and `image-png` features must be enabled on the `tauri` crate
- The `image` crate (0.25, png feature) is used to decode the sprite sheet at startup via `include_bytes!`
- Tray left-click uses `show_menu_on_left_click(false)` + `on_tray_icon_event`; right-click shows the Quit menu
- Popup and bubble positions are computed from the tray icon's physical rect, centered horizontally below the menu bar
- Tray rect is stored in `TrayRectState = Arc<Mutex<Option<(f64, f64, f64, f64)>>>`, updated on TrayIconEvent::Enter/Move
- `objc` crate (0.2) is used to call `[NSWindow orderFront:]` directly so the bubble appears without activating the app
- Tauri 2 enforces an explicit ACL: `src-tauri/capabilities/default.json` must list both `main` and `bubble` under `windows` and grant at least `core:default`, `core:event:default`, `core:event:allow-listen`, `core:event:allow-emit`, `core:event:allow-unlisten`. Missing the listen permission causes the bubble's `listen("show-bubble", …)` to silently reject and the bubble window renders empty/transparent.
- `WebviewWindow::outer_position()` can return a stale/wrong value on hidden Tauri 2 windows; the actual NSWindow position (verifiable via `CGWindowListCopyWindowInfo`) reflects what `set_position()` set. Don't rely on `outer_position()` for verification while the window is hidden.
- The macOS native screenshot pipeline used by some MCP screenshot tools filters out borderless transparent overlays even when the owning app is allowlisted; verify bubble visibility via `CGWindowListCopyWindowInfo` rather than screenshots.

## Windows Notes

- **NotifyIconSettings cleanup**: Windows 11's "Settings → Personalization → Taskbar → Other system tray icons" reads from `HKCU\Control Panel\NotifyIconSettings\<numeric-id>` (one subkey per registered tray icon, identified by `ExecutablePath`). Tauri's NSIS uninstaller does not touch this key, so install → uninstall → reinstall-to-different-path leaves the old subkey behind and the panel shows two Clowder rows. The legacy `IconStreams` / `PastIconsStream` cache wipe under `HKCU\Software\Classes\Local Settings\…\TrayNotify\` does not address this — the modern Win11 location is separate. Cleanup happens in two places: `src-tauri/installer.nsh` (`NSIS_HOOK_POSTUNINSTALL`) deletes any subkey whose `ExecutablePath` ends with `\clowder.exe` during uninstall, and `src-tauri/src/notify_icon_cleanup.rs` runs on every app start and prunes any `clowder.exe` entry whose resolved path no longer exists on disk (covers users who skipped the uninstaller).
- **`ExecutablePath` uses Known-Folder GUID prefixes, not literal paths**: Windows stores `ExecutablePath` as `REG_SZ` but for any executable under a Known Folder it substitutes a `{KF_ID}\<rest>` prefix. Common prefixes: `{6D809377-6AF0-444B-8957-A3773F02200E}` = `FOLDERID_ProgramFilesX64` (`C:\Program Files`), `{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}` = `FOLDERID_ProgramFilesX86`, `{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}` = `FOLDERID_LocalAppData`. Whichever one matches the install location, the row is stored prefixed — never as the literal drive-letter path. This shipped a bug in 0.1.9: `notify_icon_cleanup.rs` did `Path::new(&exe_path).exists()` directly on the raw value, which always returned `false`, so it deleted the live entry on every app start and the icon never persisted into the Settings UI. The runtime cleanup now resolves the KF_ID prefix via `SHGetKnownFolderPath` before the existence check, with a self-protection guard that compares against `std::env::current_exe()` so we never delete our own row regardless. The NSIS hook sidesteps the KF_ID issue by matching any subkey ending with `\clowder.exe` (during uninstall every clowder row should go anyway). Diagnose by inspecting the registry with `Get-ChildItem 'HKCU:\Control Panel\NotifyIconSettings'` and checking `ExecutablePath` values.
