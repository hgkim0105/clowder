use crate::{
    SessionMap, SessionState, StateMap, compute_live_state_ids, emit_update, load_sessions,
    sessions_dir, state_dir,
};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::time::Duration;
use tauri::AppHandle;

pub fn start_watchers(app: AppHandle, sessions: SessionMap, states: StateMap) {
    // Watch sessions directory
    let app1 = app.clone();
    let sm1 = sessions.clone();
    let stm1 = states.clone();
    let sessions_path = sessions_dir();

    // Watch state directory
    let app2 = app.clone();
    let sm2 = sessions.clone();
    let stm2 = states.clone();
    let state_path = state_dir();
    let _ = fs::create_dir_all(&state_path);

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )
        .expect("failed to create sessions watcher");

        watcher
            .watch(&sessions_path, RecursiveMode::NonRecursive)
            .expect("failed to watch sessions dir");

        for event in rx {
            handle_session_event(&event, &sm1, &stm1, &app1);
        }
    });

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(300)),
        )
        .expect("failed to create state watcher");

        watcher
            .watch(&state_path, RecursiveMode::NonRecursive)
            .expect("failed to watch state dir");

        for event in rx {
            handle_state_event(&event, &sm2, &stm2, &app2);
        }
    });
}

fn handle_session_event(event: &Event, sessions: &SessionMap, states: &StateMap, app: &AppHandle) {
    let is_create_modify = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    );
    if !is_create_modify {
        return;
    }

    let fresh = load_sessions();
    let live_ids = compute_live_state_ids(&fresh);
    let mut map = sessions.lock().unwrap();
    *map = fresh;

    // Drop state-map entries whose owning session just disappeared, so the
    // animation loop doesn't keep treating an orphan "working" state as a
    // live worker (and the cat doesn't get stuck animating after a crash).
    let mut states_guard = states.lock().unwrap();
    states_guard.retain(|id, _| live_ids.contains(id));
    emit_update(app, &map, &states_guard);
}

fn handle_state_event(event: &Event, sessions: &SessionMap, states: &StateMap, app: &AppHandle) {
    let is_write = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_)
    );
    if !is_write {
        return;
    }

    for path in &event.paths {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let session_id = stem.to_string();

        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(state) = serde_json::from_str::<SessionState>(&content) else {
            continue;
        };

        let sessions_guard = sessions.lock().unwrap();
        // Ignore state writes for sessions that aren't live. Without this a
        // stale state file from a crashed/rebooted-away Claude Code process
        // could re-enter the map on any spurious touch and revive a ghost.
        let live_ids = compute_live_state_ids(&sessions_guard);
        if !live_ids.contains(&session_id) {
            continue;
        }
        let mut states_guard = states.lock().unwrap();
        states_guard.insert(session_id, state);
        emit_update(app, &sessions_guard, &states_guard);
    }
}
