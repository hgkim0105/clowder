# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Clowder

Clowder is a macOS desktop overlay app (Tauri + React) that displays animated cat companions — one per active Claude Code session. Cats animate based on session state: idle, thinking, working, done, or scared.

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
lib.rs      — updates SessionMap + StateMap (Arc<Mutex>), resizes window, emits "sessions-update" Tauri event
        ↓
useSessions.ts  — calls get_sessions Tauri command on mount, subscribes to "sessions-update" events
        ↓
App.tsx     — renders one <Cat> per session, manages auto-revert transitions (done→idle 3s, scared→idle 2s)
        ↓
Cat.tsx     — canvas-based sprite animation, 8 frames per row, FPS varies by state (5–14)
```

**Key design decisions:**
- Session ID is hashed to deterministically assign a sprite (same session always gets the same cat color)
- Window width is dynamically resized by Rust: `max(120, sessions * 108)` px; height is fixed at 120px
- Window is transparent, always-on-top, skip-taskbar; positioned above the macOS dock with 80px margin
- Sprite sheets have rows per state; frame offsets are stored in `sprite-offsets.json`
- State transitions `done` and `scared` auto-revert to `idle` in the frontend (App.tsx), not in Rust

**Source layout:**
- `src-tauri/src/lib.rs` — Tauri commands, session/state management, window resizing
- `src-tauri/src/watcher.rs` — file system polling for session and state directories
- `src/hooks/useSessions.ts` — IPC bridge between Tauri backend and React
- `src/components/Cat.tsx` — canvas animation renderer
- `src/App.tsx` — session list, sprite loading, state transition timers
- `src/types.ts` — shared TypeScript types (`CatState`, `SessionInfo`, animation config)
- `src/sprite-offsets.json` — per-frame pixel offsets for each sprite sheet

## Runtime File Paths

| Path | Purpose |
|------|---------|
| `~/.claude/sessions/*.json` | Claude Code session metadata (read-only) |
| `~/.claude/clowder/state/*.json` | Per-session cat state written by Claude hooks |

## Tauri / macOS Notes

- `macOSPrivateApi: true` is required in `tauri.conf.json` for the transparent always-on-top overlay to work
- The Rust crate produces both `staticlib` and `cdylib` (required by Tauri)
- Before `tauri dev` runs, it starts the Vite dev server automatically (configured in `tauri.conf.json`)
