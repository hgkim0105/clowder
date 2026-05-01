mod watcher;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};
use watcher::start_watchers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    pub kind: String,
    pub entrypoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub state: String,
    pub tool_name: Option<String>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionWithState {
    pub info: SessionInfo,
    pub state: String,
    pub tool_name: Option<String>,
}

pub type SessionMap = Arc<Mutex<HashMap<String, SessionInfo>>>;
pub type StateMap = Arc<Mutex<HashMap<String, SessionState>>>;

// Per-cat width in logical pixels (128px frame * 0.75 scale = 96px + padding + label)
const CAT_WIDTH: f64 = 108.0;
const WIN_HEIGHT: f64 = 116.0;
const MIN_WIDTH: f64 = 120.0;
// Gap above macOS dock (logical px)
const DOCK_MARGIN: f64 = 80.0;

pub fn sessions_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".claude")
        .join("sessions")
}

pub fn state_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".claude")
        .join("clowder")
        .join("state")
}

pub fn load_sessions() -> HashMap<String, SessionInfo> {
    let mut map = HashMap::new();
    let dir = sessions_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(info) = serde_json::from_str::<SessionInfo>(&content) else {
            continue;
        };
        map.insert(info.session_id.clone(), info);
    }
    map
}

fn load_state(session_id: &str) -> Option<SessionState> {
    let path = state_dir().join(format!("{}.json", session_id));
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn emit_update(app: &AppHandle, sessions: &HashMap<String, SessionInfo>, states: &HashMap<String, SessionState>) {
    let mut list: Vec<SessionWithState> = sessions.values().map(|info| {
        let st = states.get(&info.session_id);
        SessionWithState {
            info: info.clone(),
            state: st.map(|s| s.state.clone()).unwrap_or_else(|| "idle".into()),
            tool_name: st.and_then(|s| s.tool_name.clone()),
        }
    }).collect();
    list.sort_by_key(|s| s.info.started_at);

    // Resize window to fit session count
    resize_window(app, list.len());

    let _ = app.emit("sessions-update", &list);
}

fn resize_window(app: &AppHandle, count: usize) {
    let Some(win) = app.get_webview_window("main") else { return };

    let width = (CAT_WIDTH * count.max(1) as f64).max(MIN_WIDTH);

    // Get primary monitor size to position at bottom-left above dock
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let scale = monitor.scale_factor();
        let mw = monitor.size().width as f64 / scale;
        let mh = monitor.size().height as f64 / scale;

        let x = (mw - width) / 2.0; // centered horizontally
        let y = mh - WIN_HEIGHT - DOCK_MARGIN;

        let _ = win.set_position(LogicalPosition::new(x, y));
    }

    let _ = win.set_size(LogicalSize::new(width, WIN_HEIGHT));
}

#[tauri::command]
fn get_sessions(
    sessions: tauri::State<SessionMap>,
    states: tauri::State<StateMap>,
) -> Vec<SessionWithState> {
    let sessions = sessions.lock().unwrap();
    let states = states.lock().unwrap();
    let mut list: Vec<SessionWithState> = sessions.values().map(|info| {
        let st = states.get(&info.session_id);
        SessionWithState {
            info: info.clone(),
            state: st.map(|s| s.state.clone()).unwrap_or_else(|| "idle".into()),
            tool_name: st.and_then(|s| s.tool_name.clone()),
        }
    }).collect();
    list.sort_by_key(|s| s.info.started_at);
    list
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let session_map: SessionMap = Arc::new(Mutex::new(load_sessions()));
    let state_map: StateMap = Arc::new(Mutex::new(HashMap::new()));

    // Pre-load existing state files
    {
        let sessions = session_map.lock().unwrap();
        let mut states = state_map.lock().unwrap();
        for session_id in sessions.keys() {
            if let Some(st) = load_state(session_id) {
                states.insert(session_id.clone(), st);
            }
        }
    }

    tauri::Builder::default()
        .manage(session_map.clone())
        .manage(state_map.clone())
        .invoke_handler(tauri::generate_handler![get_sessions])
        .setup(move |app| {
            let _ = fs::create_dir_all(state_dir());

            // Initial window positioning
            let count = session_map.lock().unwrap().len();
            let app_handle = app.handle().clone();
            resize_window(&app_handle, count);

            let sm = session_map.clone();
            let stm = state_map.clone();
            start_watchers(app_handle, sm, stm);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running clowder");
}
