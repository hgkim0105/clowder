mod watcher;

use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::image::Image as TauriImage;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};
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

const FRAME_SIZE: u32 = 32;
const SPRITE_BYTES: &[u8] = include_bytes!("../../public/Cat Sprite Sheet.png");

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

pub fn emit_update(
    app: &AppHandle,
    sessions: &HashMap<String, SessionInfo>,
    states: &HashMap<String, SessionState>,
) {
    let mut list: Vec<SessionWithState> = sessions
        .values()
        .map(|info| {
            let st = states.get(&info.session_id);
            SessionWithState {
                info: info.clone(),
                state: st.map(|s| s.state.clone()).unwrap_or_else(|| "idle".into()),
                tool_name: st.and_then(|s| s.tool_name.clone()),
            }
        })
        .collect();
    list.sort_by_key(|s| s.info.started_at);
    let _ = app.emit("sessions-update", &list);
}

/// (row, frame_count, fps)
fn anim_config(state: &str, active_count: usize) -> (u32, u32, u64) {
    match state {
        "idle"    => (0, 4, 6),
        "working" => {
            let fps = match active_count {
                0 | 1 => 12,
                2     => 16,
                _     => 20,
            };
            (4, 8, fps)
        }
        _         => (0, 4, 6),
    }
}

fn compute_display_state(list: &[SessionWithState]) -> (&'static str, usize) {
    let active = list.iter().filter(|s| s.state != "idle").count();
    if active > 0 { ("working", active) } else { ("idle", 0) }
}

fn make_icon(sheet: &DynamicImage, row: u32, col: u32) -> TauriImage<'static> {
    use image::imageops::FilterType;
    use image::{GenericImageView, Rgba};

    let sub = sheet.crop_imm(col * FRAME_SIZE, row * FRAME_SIZE, FRAME_SIZE, FRAME_SIZE);
    let rgba = sub.to_rgba8();
    let (w, h) = rgba.dimensions();

    // Find tight bounding box of visible (non-transparent) pixels
    const ALPHA_THRESH: u8 = 10;
    let min_x = (0..w).find(|&x| (0..h).any(|y| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).unwrap_or(0);
    let max_x = (0..w).rev().find(|&x| (0..h).any(|y| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).map(|x| x + 1).unwrap_or(w);
    let min_y = (0..h).find(|&y| (0..w).any(|x| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).unwrap_or(0);
    let max_y = (0..h).rev().find(|&y| (0..w).any(|x| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).map(|y| y + 1).unwrap_or(h);

    let crop_w = max_x.saturating_sub(min_x).max(1);
    let crop_h = max_y.saturating_sub(min_y).max(1);

    // Crop to cat only, then scale to 64x64 for sharp retina rendering
    let cropped = sub.crop_imm(min_x, min_y, crop_w, crop_h);
    let scaled = cropped.resize_exact(64, 64, FilterType::Nearest);
    let out = scaled.to_rgba8();
    let (ow, oh) = out.dimensions();
    TauriImage::new_owned(out.into_raw(), ow, oh)
}

async fn animation_loop(
    tray: tauri::tray::TrayIcon,
    session_map: SessionMap,
    state_map: StateMap,
    sheet: Arc<DynamicImage>,
) {
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let mut display_state = String::from("idle");
    let mut active_count: usize = 0;
    let mut current_frame: u32 = 0;
    let mut last_frame_time = Instant::now();

    eprintln!("[clowder] animation_loop started");

    // Initial render
    let (row, _, _) = anim_config("idle", 0);
    let result = tray.set_icon(Some(make_icon(&sheet, row, 0)));
    eprintln!("[clowder] initial set_icon result: {:?}", result);

    loop {
        tick.tick().await;

        let (new_display, new_active) = {
            let sessions = session_map.lock().unwrap();
            let states = state_map.lock().unwrap();
            let list: Vec<SessionWithState> = sessions
                .values()
                .map(|info| {
                    let st = states.get(&info.session_id);
                    SessionWithState {
                        info: info.clone(),
                        state: st.map(|s| s.state.clone()).unwrap_or_else(|| "idle".into()),
                        tool_name: st.and_then(|s| s.tool_name.clone()),
                    }
                })
                .collect();
            let (s, c) = compute_display_state(&list);
            (s.to_string(), c)
        };

        let (row, frame_count, fps) = anim_config(&new_display, new_active);

        if new_display != display_state {
            display_state = new_display;
            active_count = new_active;
            current_frame = 0;
            last_frame_time = Instant::now();
        } else {
            active_count = new_active;
            let frame_dur = Duration::from_millis(1000 / fps);
            if last_frame_time.elapsed() >= frame_dur {
                current_frame = (current_frame + 1) % frame_count;
                last_frame_time = Instant::now();
            }
        }

        let icon = make_icon(&sheet, row, current_frame);
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_icon_as_template(true);
    }
}

#[tauri::command]
fn get_sessions(
    sessions: tauri::State<SessionMap>,
    states: tauri::State<StateMap>,
) -> Vec<SessionWithState> {
    let sessions = sessions.lock().unwrap();
    let states = states.lock().unwrap();
    let mut list: Vec<SessionWithState> = sessions
        .values()
        .map(|info| {
            let st = states.get(&info.session_id);
            SessionWithState {
                info: info.clone(),
                state: st.map(|s| s.state.clone()).unwrap_or_else(|| "idle".into()),
                tool_name: st.and_then(|s| s.tool_name.clone()),
            }
        })
        .collect();
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

    // Load sprite sheet once at startup
    let sheet = Arc::new(
        image::load_from_memory(SPRITE_BYTES).expect("failed to load sprite sheet"),
    );

    tauri::Builder::default()
        .manage(session_map.clone())
        .manage(state_map.clone())
        .invoke_handler(tauri::generate_handler![get_sessions])
        .setup(move |app| {
            let _ = fs::create_dir_all(state_dir());

            // No Dock icon, no app switcher entry
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Hide the webview window
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
            }

            // Tray menu
            let quit_item = tauri::menu::MenuItem::with_id(
                app,
                "quit",
                "Quit clowder",
                true,
                None::<&str>,
            )?;
            let menu = tauri::menu::Menu::with_items(app, &[&quit_item])?;

            // Initial idle frame
            let (row, _, _) = anim_config("idle", 0);
            let initial_icon = make_icon(&sheet, row, 0);

            let tray = TrayIconBuilder::new()
                .icon(initial_icon)
                .tooltip("clowder")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id().as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // Start file watchers
            let app_handle = app.handle().clone();
            start_watchers(app_handle, session_map.clone(), state_map.clone());

            // Spawn animation loop — tray moved in, kept alive by the task
            let sm = session_map.clone();
            let stm = state_map.clone();
            let sheet_clone = sheet.clone();
            tauri::async_runtime::spawn(async move {
                animation_loop(tray, sm, stm, sheet_clone).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running clowder");
}
