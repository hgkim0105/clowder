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
        ↓
macOS NSStatusItem  — displays animated cat in menu bar

left-click on tray icon
        ↓
webview popup (320×480)  — shows session list via get_sessions IPC command
                           reads stats from JSONL (last 64KB) on each call
```

**Tray icon states:**

| State   | Trigger                              | Duration         |
|---------|--------------------------------------|------------------|
| idle    | all sessions idle                    | persistent       |
| working | any session non-idle and non-done    | while active     |
| done    | all sessions done, none working      | 4 s then → idle  |

**Key design decisions:**
- The webview window is hidden at startup; shown/hidden on tray left-click
- `ActivationPolicy::Accessory` removes Dock icon and app switcher entry
- Window hides automatically via `WindowEvent::Focused(false)` in Rust (more reliable than JS blur)
- Working FPS scales with active session count: 1→12fps, 2→16fps, 3+→20fps
- `make_icon` auto-crops transparent padding then scales to 64×64 for sharp retina rendering
- Stats (model, tokens, speed, permissionMode) are read from JSONL on each `get_sessions` call; not cached to avoid stale data
- `emit_update` events carry no stats (lightweight); popup always calls `get_sessions` on events

**Sprite rows:**

| State   | Row | Frames | FPS          |
|---------|-----|--------|--------------|
| idle    | 0   | 4      | 6            |
| working | 4   | 8      | 12 / 16 / 20 |
| done    | 6   | 4      | 8            |

**Source layout:**
- `src-tauri/src/lib.rs` — tray setup, popup toggle, session/state management, JSONL stats parsing, animation loop
- `src-tauri/src/watcher.rs` — file system watching for session and state directories
- `src/App.tsx` — popup UI: session list, stats badges, context bar, quit button
- `src/hooks/useSessions.ts` — IPC bridge: calls `get_sessions` on load, events, and every 10s
- `src/types.ts` — shared TypeScript types

## Popup Panel

Left-clicking the tray icon opens a 320×480 frosted-glass popup below the menu bar icon. Each session row shows:
- Working directory (truncated to last 2 components if deep)
- State indicator: orange pulse (working) / green dot (done · elapsed) / gray (idle)
- Badges: model name, speed (if non-standard), "Thinking" (if extended thinking), permission mode
- Context bar: token usage vs 200k window, color-coded by fill level

The popup closes when it loses focus. A "Quit Clowder" button sits in the footer.

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
- Popup position is computed from the tray icon's physical rect, centered horizontally below the menu bar
