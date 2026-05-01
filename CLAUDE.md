# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Clowder

Clowder is a macOS menu bar app (Tauri + React) that displays an animated pixel-art cat in the system tray. The cat reflects the dominant state across all active Claude Code sessions: idle, thinking, working, done, or scared.

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
~/.claude/sessions/*.json          (Claude session metadata)
~/.claude/clowder/state/*.json     (per-session state: idle/thinking/working/done/scared)
        ↓
watcher.rs  — polls both dirs every 300–500ms via notify crate
        ↓
lib.rs      — updates SessionMap + StateMap (Arc<Mutex>), emits "sessions-update" Tauri event
        ↓
animation_loop (tokio task)  — reads maps every 50ms, computes dominant state,
                               advances sprite frame, calls tray.set_icon()
        ↓
macOS NSStatusItem  — displays animated cat in menu bar
```

**Key design decisions:**
- Display is entirely via `TrayIconBuilder` (NSStatusItem) — the webview window is hidden
- `ActivationPolicy::Accessory` removes the Dock icon and app switcher entry
- Dominant state priority: `working > thinking > scared > done > idle`
- `done→idle` (3s) and `scared→idle` (2s) auto-revert are handled in the Rust animation loop, not the frontend
- `make_icon` auto-crops transparent padding from each sprite frame then scales to 64×64 for sharp retina rendering
- Sprite sheet: `public/Cat Sprite Sheet.png`, 256×320px, 32×32px per frame, 8 cols × 10 rows

**Sprite rows:**

| State    | Row | Frames | FPS |
|----------|-----|--------|-----|
| idle     | 0   | 4      | 6   |
| thinking | 1   | 4      | 8   |
| working  | 4   | 8      | 12  |
| scared   | 5   | 8      | 14  |
| done     | 6   | 4      | 5   |

**Source layout:**
- `src-tauri/src/lib.rs` — tray setup, session/state management, animation loop
- `src-tauri/src/watcher.rs` — file system polling for session and state directories
- `src/hooks/useSessions.ts` — IPC bridge (unused in tray mode, kept for future use)
- `src/types.ts` — shared TypeScript types and animation config

## Runtime File Paths

| Path | Purpose |
|------|---------|
| `~/.claude/sessions/*.json` | Claude Code session metadata (read-only) |
| `~/.claude/clowder/state/*.json` | Per-session cat state written by Claude hooks |

## Tauri / macOS Notes

- `macOSPrivateApi: true` is required in `tauri.conf.json`
- The Rust crate produces both `staticlib` and `cdylib` (required by Tauri)
- The webview window is set to `visible: false`; all display is via the tray icon
- `tray-icon` and `image-png` features must be enabled on the `tauri` crate
- The `image` crate (0.25, png feature) is used to decode the sprite sheet at startup via `include_bytes!`
