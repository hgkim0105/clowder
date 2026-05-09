mod hook_install;
mod notify_icon_cleanup;
mod watcher;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub context_window: u64,
    pub speed: Option<String>,
    pub permission_mode: Option<String>,
    pub has_thinking: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionWithState {
    pub info: SessionInfo,
    pub state: String,
    pub tool_name: Option<String>,
    pub state_updated_at: Option<u64>,
    pub stats: Option<SessionStats>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BubbleSess {
    pub cwd: String,
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub type SessionMap = Arc<Mutex<HashMap<String, SessionInfo>>>;
pub type StateMap = Arc<Mutex<HashMap<String, SessionState>>>;
// (pos_x, pos_y, size_w, size_h) in physical pixels
pub type TrayRectState = Arc<Mutex<Option<(f64, f64, f64, f64)>>>;

/// Convert a working directory path into the directory name Claude Code uses
/// under ~/.claude/projects. Observed on Windows that `:`, `\`, and `.` all
/// become `-` (e.g. `D:\Projects\.claude\foo` → `D--Projects--claude-foo`);
/// macOS already needed `/` → `-`. Treat all four the same on every OS so the
/// algorithm is platform-agnostic.
fn project_dir_name(cwd: &str) -> String {
    cwd.chars()
        .map(|c| match c {
            ':' | '\\' | '/' | '.' => '-',
            _ => c,
        })
        .collect()
}

/// Find the session UUID currently active for a given working directory.
/// Claude Code stores conversations at ~/.claude/projects/<encoded-cwd>/<session-id>.jsonl.
/// The most recently modified JSONL in that directory is the active conversation.
fn find_active_session_id(cwd: &str) -> Option<String> {
    let proj_dir_name = project_dir_name(cwd);
    let proj_dir = dirs::home_dir()?
        .join(".claude")
        .join("projects")
        .join(&proj_dir_name);
    if !proj_dir.is_dir() {
        return None;
    }
    let mut latest: Option<(std::time::SystemTime, String)> = None;
    for entry in fs::read_dir(&proj_dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str())?.to_string();
        let mtime = entry.metadata().ok()?.modified().ok()?;
        match &latest {
            None => latest = Some((mtime, stem)),
            Some((t, _)) if mtime > *t => latest = Some((mtime, stem)),
            _ => {}
        }
    }
    latest.map(|(_, id)| id)
}

/// Look up the best session state: try direct UUID match first, then fall back to the active
/// conversation for the same project directory. Returns whichever has the more recent timestamp.
fn find_recent_state<'a>(
    info: &SessionInfo,
    states: &'a HashMap<String, SessionState>,
    sessions: &HashMap<String, SessionInfo>,
) -> Option<&'a SessionState> {
    let direct = states.get(&info.session_id);

    // Only use the JSONL fallback when this session is the sole occupant of its CWD.
    // Multiple sessions in the same directory share the same project folder, so
    // find_active_session_id would return the same UUID for all of them and cause
    // one session's state to bleed into the others.
    let sole_in_cwd = sessions.values().filter(|s| s.cwd == info.cwd).count() == 1;
    if !sole_in_cwd {
        return direct;
    }

    let active_id = find_active_session_id(&info.cwd);
    let fallback = active_id.as_deref().and_then(|id| states.get(id));
    match (direct, fallback) {
        (Some(d), Some(f)) => {
            if f.updated_at > d.updated_at { Some(f) } else { Some(d) }
        }
        (Some(d), None) => Some(d),
        (None, Some(f)) => Some(f),
        (None, None) => None,
    }
}

fn find_session_jsonl(session_id: &str) -> Option<PathBuf> {
    let projects_dir = dirs::home_dir()?.join(".claude").join("projects");
    let filename = format!("{}.jsonl", session_id);
    for entry in fs::read_dir(&projects_dir).ok()?.flatten() {
        let candidate = entry.path().join(&filename);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn load_session_stats(session_id: &str) -> Option<SessionStats> {
    let path = find_session_jsonl(session_id)?;
    let mut file = fs::File::open(&path).ok()?;
    let size = file.seek(SeekFrom::End(0)).ok()?;
    file.seek(SeekFrom::Start(size.saturating_sub(65536))).ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;

    let mut model = None;
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut speed = None;
    let mut permission_mode = None;
    let mut has_thinking = false;
    let mut found_model = false;
    let mut found_perm = false;

    for line in buf.lines().rev() {
        if found_model && found_perm {
            break;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if !found_perm {
            if let Some(pm) = v["permissionMode"].as_str() {
                permission_mode = Some(pm.to_string());
                found_perm = true;
            }
        }

        if !found_model && v["type"].as_str() == Some("assistant") {
            if let Some(usage) = v["message"]["usage"].as_object() {
                model = v["message"]["model"].as_str().map(|s| s.to_string());
                let inp = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let cr = usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let cc = usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                input_tokens = inp + cr + cc;
                output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                speed = usage.get("speed").and_then(|v| v.as_str()).map(|s| s.to_string());
                if let Some(content) = v["message"]["content"].as_array() {
                    has_thinking = content.iter().any(|c| c["type"].as_str() == Some("thinking"));
                }
                found_model = true;
            }
        }
    }

    let context_window = context_window_for_model(model.as_deref(), input_tokens);

    Some(SessionStats {
        model,
        input_tokens,
        output_tokens,
        context_window,
        speed,
        permission_mode,
        has_thinking,
    })
}

/// Pick the context window based on model id. Newer Claude 4.x models support 1M
/// context; everything else defaults to 200k. If the observed input already exceeds
/// the picked value, fall back to 1M so the bar stays meaningful.
fn context_window_for_model(model: Option<&str>, observed: u64) -> u64 {
    let base = match model {
        Some(m) => {
            let m = m.to_ascii_lowercase();
            // Claude 4.x families that support 1M context
            if m.contains("opus-4") || m.contains("sonnet-4") || m.contains("haiku-4") {
                1_000_000
            } else {
                200_000
            }
        }
        None => 200_000,
    };
    if observed > base { 1_000_000 } else { base }
}

const FRAME_SIZE: u32 = 32;
const SPRITE_BYTES: &[u8] = include_bytes!("../../public/Cat Sprite Sheet.png");

/// Output icon size for the system tray. macOS menu bar handles 64×64 well
/// (DWM scales for retina). Windows tray draws at 16/24/32 logical px; we
/// feed it a slightly oversized 40 so high-DPI taskbars get a crisper cat
/// without the source being so big that nearest-neighbor downscaling muddies
/// the pixel art.
#[cfg(target_os = "windows")]
const TRAY_ICON_SIZE: u32 = 72;
#[cfg(not(target_os = "windows"))]
const TRAY_ICON_SIZE: u32 = 64;

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

/// "done" only stays fresh briefly. After this many seconds since the last state
/// update, treat done as idle for display purposes — the user has long since
/// moved on and persistent green dots are noise.
const DONE_FRESHNESS_SECS: u64 = 60;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Display-time view of state: stale "done" decays to "idle".
fn effective_state(st: Option<&SessionState>) -> (String, Option<String>, Option<u64>) {
    let Some(s) = st else {
        return ("idle".into(), None, None);
    };
    if s.state == "done" && now_ms().saturating_sub(s.updated_at) > DONE_FRESHNESS_SECS * 1000 {
        ("idle".into(), None, Some(s.updated_at))
    } else {
        (s.state.clone(), s.tool_name.clone(), Some(s.updated_at))
    }
}

pub fn emit_update(
    app: &AppHandle,
    sessions: &HashMap<String, SessionInfo>,
    states: &HashMap<String, SessionState>,
) {
    let mut list: Vec<SessionWithState> = sessions
        .values()
        .map(|info| {
            let st = find_recent_state(info, states, sessions);
            let (state, tool_name, state_updated_at) = effective_state(st);
            SessionWithState {
                info: info.clone(),
                state,
                tool_name,
                state_updated_at,
                stats: None,
            }
        })
        .collect();
    list.sort_by_key(|s| s.info.started_at);
    let _ = app.emit("sessions-update", &list);
}

/// Y position for a window anchored to the tray icon, in physical pixels.
/// macOS menu bar sits at the top of the screen, so windows hang below the
/// tray rect. Windows taskbar typically sits at the bottom, so windows must
/// rise above the tray rect to stay on-screen. (Side-mounted Windows taskbars
/// are not yet handled — they'll fall back to "above" which is wrong but
/// keeps the window on-screen.)
fn anchor_y(tray_y: f64, tray_h: f64, win_h_phys: f64, margin_phys: f64) -> f64 {
    if cfg!(target_os = "windows") {
        (tray_y - win_h_phys - margin_phys).max(0.0)
    } else {
        tray_y + tray_h + margin_phys
    }
}

/// Convert tray event rect position/size to physical pixel coordinates.
fn phys_coords(
    pos: tauri::Position,
    sz: tauri::Size,
    scale: f64,
) -> (f64, f64, f64, f64) {
    let (px, py) = match pos {
        tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
        tauri::Position::Logical(p) => (p.x * scale, p.y * scale),
    };
    let (sw, sh) = match sz {
        tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
        tauri::Size::Logical(s) => (s.width * scale, s.height * scale),
    };
    (px, py, sw, sh)
}

/// Show window without stealing keyboard focus.
/// orderFrontRegardless works even when the app is in Accessory (background) mode.
/// NSStatusBarWindowLevel (25) places the bubble above the menu bar, ensuring visibility.
#[cfg(target_os = "macos")]
fn show_without_focus(win: &tauri::WebviewWindow) {
    if let Ok(ptr) = win.ns_window() {
        let ns_window = ptr as *mut objc::runtime::Object;
        unsafe {
            let _: () = msg_send![ns_window, setLevel: 25i64];
            let _: () = msg_send![ns_window, orderFrontRegardless];
        }
    } else {
        let _ = win.show();
    }
}

/// Windows: show the window without activating it. ShowWindow(SW_SHOWNOACTIVATE)
/// keeps keyboard focus on the previously active app, and SetWindowPos with
/// SWP_NOACTIVATE pins it topmost without stealing focus.
#[cfg(target_os = "windows")]
fn show_without_focus(win: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SW_SHOWNOACTIVATE,
    };
    if let Ok(hwnd) = win.hwnd() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    } else {
        let _ = win.show();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn show_without_focus(win: &tauri::WebviewWindow) {
    let _ = win.show();
}

/// Show the speech bubble when sessions transition to done.
fn trigger_bubble(
    app: &AppHandle,
    tray: &tauri::tray::TrayIcon,
    tray_rect: &TrayRectState,
    state_map: &StateMap,
    session_map: &SessionMap,
) {
    // Don't interrupt if the popup is already open
    if app
        .get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false)
    {
        return;
    }

    let done_sessions: Vec<BubbleSess> = {
        let sessions = session_map.lock().unwrap();
        let states = state_map.lock().unwrap();
        let sole_in_cwd_for = |cwd: &str| sessions.values().filter(|s| s.cwd == cwd).count() == 1;
        // Pick the most recently updated done session per cwd, fresh ones only
        let now = now_ms();
        let mut by_cwd: HashMap<String, (&SessionInfo, u64)> = HashMap::new();
        for info in sessions.values() {
            let Some(st) = find_recent_state(info, &*states, &*sessions) else { continue };
            if st.state != "done" { continue; }
            if now.saturating_sub(st.updated_at) > DONE_FRESHNESS_SECS * 1000 { continue; }
            by_cwd
                .entry(info.cwd.clone())
                .and_modify(|(cur, ts)| {
                    if st.updated_at > *ts {
                        *cur = info;
                        *ts = st.updated_at;
                    }
                })
                .or_insert((info, st.updated_at));
        }
        let mut list: Vec<BubbleSess> = by_cwd
            .into_values()
            .map(|(info, _)| {
                let stats = if sole_in_cwd_for(&info.cwd) {
                    find_active_session_id(&info.cwd)
                        .as_deref()
                        .and_then(load_session_stats)
                        .or_else(|| load_session_stats(&info.session_id))
                } else {
                    load_session_stats(&info.session_id)
                };
                BubbleSess {
                    cwd: info.cwd.clone(),
                    model: stats.as_ref().and_then(|s| s.model.clone()),
                    input_tokens: stats.as_ref().map(|s| s.input_tokens).unwrap_or(0),
                    output_tokens: stats.as_ref().map(|s| s.output_tokens).unwrap_or(0),
                }
            })
            .collect();
        list.sort_by(|a, b| a.cwd.cmp(&b.cwd));
        list
    };

    if done_sessions.is_empty() {
        return;
    }

    // Prefer cached rect; fall back to live tray.rect() query
    let stored_rect: Option<(f64, f64, f64, f64)> = tray_rect.lock().unwrap().or_else(|| {
        let r = tray.rect().ok()??;
        let scale = app
            .get_webview_window("main")
            .and_then(|w| w.scale_factor().ok())
            .unwrap_or(2.0);
        Some(phys_coords(r.position, r.size, scale))
    });
    let app_clone = app.clone();

    let _ = app.run_on_main_thread(move || {
        let Some(bubble) = app_clone.get_webview_window("bubble") else {
            return;
        };

        let height = if done_sessions.len() > 1 { 116.0_f64 } else { 80.0_f64 };
        let _ = bubble.set_size(tauri::LogicalSize::new(260.0_f64, height));

        if let Some((px, py, sw, sh)) = stored_rect {
            let scale = bubble.scale_factor().unwrap_or(2.0);
            let bubble_w_phys = 260.0 * scale;
            let bubble_h_phys = height * scale;
            let x = (px + sw / 2.0 - bubble_w_phys / 2.0).max(0.0);
            let y = anchor_y(py, sh, bubble_h_phys, 8.0 * scale);
            let _ = bubble.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
        }

        show_without_focus(&bubble);
        let _ = app_clone.emit("show-bubble", &done_sessions);
    });
}

/// (row, frame_count, fps)
fn anim_config(state: &str, active_count: usize) -> (u32, u32, u64) {
    match state {
        "idle"    => (0, 4, 6),
        "done"    => (6, 4, 8),
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

fn make_icon(sheet: &DynamicImage, row: u32, col: u32) -> TauriImage<'static> {
    use image::imageops::FilterType;

    let sub = sheet.crop_imm(col * FRAME_SIZE, row * FRAME_SIZE, FRAME_SIZE, FRAME_SIZE);
    let rgba = sub.to_rgba8();
    let (w, h) = rgba.dimensions();

    const ALPHA_THRESH: u8 = 10;
    let min_x = (0..w).find(|&x| (0..h).any(|y| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).unwrap_or(0);
    let max_x = (0..w).rev().find(|&x| (0..h).any(|y| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).map(|x| x + 1).unwrap_or(w);
    let min_y = (0..h).find(|&y| (0..w).any(|x| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).unwrap_or(0);
    let max_y = (0..h).rev().find(|&y| (0..w).any(|x| rgba.get_pixel(x, y)[3] > ALPHA_THRESH)).map(|y| y + 1).unwrap_or(h);

    let crop_w = max_x.saturating_sub(min_x).max(1);
    let crop_h = max_y.saturating_sub(min_y).max(1);

    let cropped = sub.crop_imm(min_x, min_y, crop_w, crop_h);
    let scaled = cropped.resize_exact(TRAY_ICON_SIZE, TRAY_ICON_SIZE, FilterType::Nearest);
    let out = scaled.to_rgba8();
    let (ow, oh) = out.dimensions();
    TauriImage::new_owned(out.into_raw(), ow, oh)
}

async fn animation_loop(
    app: AppHandle,
    tray: tauri::tray::TrayIcon,
    state_map: StateMap,
    session_map: SessionMap,
    tray_rect: TrayRectState,
    sheet: Arc<DynamicImage>,
) {
    const DONE_DISPLAY_SECS: u64 = 4;

    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let mut display_state = String::from("idle");
    let mut current_frame: u32 = 0;
    let mut last_frame_time = Instant::now();
    let mut done_since: Option<Instant> = None;

    // Stale "done" states (from sessions that finished long ago and are now sitting
    // idle waiting for user input) shouldn't keep the cat in done forever, nor cause
    // false bubble triggers. Match the popup's freshness window.
    let raw_display_from = |states: &HashMap<String, SessionState>| -> (String, usize) {
        let now = now_ms();
        let working = states.values().filter(|s| s.state != "idle" && s.state != "done").count();
        if working > 0 {
            ("working".to_string(), working)
        } else if states
            .values()
            .any(|s| s.state == "done" && now.saturating_sub(s.updated_at) <= DONE_FRESHNESS_SECS * 1000)
        {
            ("done".to_string(), 0)
        } else {
            ("idle".to_string(), 0)
        }
    };

    // Initialise prev_raw_display to current state so startup doesn't fire bubble
    let mut prev_raw_display = {
        let states = state_map.lock().unwrap();
        raw_display_from(&*states).0
    };

    eprintln!("[clowder] animation_loop started");

    let (row, _, _) = anim_config("idle", 0);
    let result = tray.set_icon(Some(make_icon(&sheet, row, 0)));
    eprintln!("[clowder] initial set_icon result: {:?}", result);

    loop {
        tick.tick().await;

        let (raw_display, new_active) = {
            let states = state_map.lock().unwrap();
            raw_display_from(&*states)
        };

        // Detect idle/working → done transition and show bubble
        if raw_display == "done" && prev_raw_display != "done" {
            trigger_bubble(&app, &tray, &tray_rect, &state_map, &session_map);
        }
        prev_raw_display = raw_display.clone();

        // "done" shows for DONE_DISPLAY_SECS then reverts to idle
        let effective_display: &str = if raw_display == "done" {
            if done_since.is_none() {
                done_since = Some(Instant::now());
            }
            if done_since.unwrap().elapsed().as_secs() < DONE_DISPLAY_SECS {
                "done"
            } else {
                "idle"
            }
        } else {
            done_since = None;
            &raw_display
        };

        let (row, frame_count, fps) = anim_config(effective_display, new_active);

        if effective_display != display_state {
            display_state = effective_display.to_string();
            current_frame = 0;
            last_frame_time = Instant::now();
        } else {
            let frame_dur = Duration::from_millis(1000 / fps);
            if last_frame_time.elapsed() >= frame_dur {
                current_frame = (current_frame + 1) % frame_count;
                last_frame_time = Instant::now();
            }
        }

        let icon = make_icon(&sheet, row, current_frame);
        let _ = tray.set_icon(Some(icon));
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(true);
    }
}

// The About dialog lives in its own webview window (label "about") so the
// content is plain HTML — gives us a real clickable link to the author and
// repo without dropping into Win32 TaskDialog or NSAttributedString.
fn show_about_dialog(app: &AppHandle) {
    let Some(win) = app.get_webview_window("about") else {
        eprintln!("clowder: about window not found");
        return;
    };
    let _ = win.show();
    let _ = win.set_focus();
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
            let st = find_recent_state(info, &*states, &*sessions);
            let sole_in_cwd = sessions.values().filter(|s| s.cwd == info.cwd).count() == 1;
            let stats = if sole_in_cwd {
                // Single session in this CWD: prefer the most recently active JSONL
                find_active_session_id(&info.cwd)
                    .as_deref()
                    .and_then(load_session_stats)
                    .or_else(|| load_session_stats(&info.session_id))
            } else {
                // Multiple sessions in same CWD: only use the session's own UUID
                load_session_stats(&info.session_id)
            };
            let (state, tool_name, state_updated_at) = effective_state(st);
            SessionWithState {
                info: info.clone(),
                state,
                tool_name,
                state_updated_at,
                stats,
            }
        })
        .collect();
    list.sort_by_key(|s| s.info.started_at);
    list
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    hook_install::ensure_hooks_installed();
    notify_icon_cleanup::cleanup_orphan_notify_icons();

    let session_map: SessionMap = Arc::new(Mutex::new(load_sessions()));
    let state_map: StateMap = Arc::new(Mutex::new(HashMap::new()));
    let tray_rect_state: TrayRectState = Arc::new(Mutex::new(None));

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

    let sheet = Arc::new(
        image::load_from_memory(SPRITE_BYTES).expect("failed to load sprite sheet"),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .manage(session_map.clone())
        .manage(state_map.clone())
        .manage(tray_rect_state.clone())
        .invoke_handler(tauri::generate_handler![get_sessions])
        .setup(move |app| {
            let _ = fs::create_dir_all(state_dir());

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Enable launch-on-login on first run. Idempotent: if already enabled
            // (or the user explicitly disabled it via system settings), this is a
            // no-op. macOS writes a LaunchAgent plist; Windows writes the Run
            // registry key — both via tauri-plugin-autostart.
            {
                use tauri_plugin_autostart::ManagerExt;
                let autostart = app.autolaunch();
                if !autostart.is_enabled().unwrap_or(false) {
                    let _ = autostart.enable();
                }
            }

            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
                // Windows DWM draws a shadow at the full window rect even with
                // decorations off, which appears as an outline around the empty
                // area outside the popup card. Disable per-window on Windows.
                #[cfg(target_os = "windows")]
                let _ = win.set_shadow(false);
            }
            if let Some(win) = app.get_webview_window("bubble") {
                let _ = win.hide();
                #[cfg(target_os = "windows")]
                let _ = win.set_shadow(false);
            }
            if let Some(win) = app.get_webview_window("about") {
                let _ = win.hide();
                #[cfg(target_os = "windows")]
                let _ = win.set_shadow(false);
            }

            let about_item = tauri::menu::MenuItem::with_id(
                app,
                "about",
                "About Clowder",
                true,
                None::<&str>,
            )?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let quit_item = tauri::menu::MenuItem::with_id(
                app,
                "quit",
                "Quit Clowder",
                true,
                None::<&str>,
            )?;
            let menu = tauri::menu::Menu::with_items(app, &[&about_item, &separator, &quit_item])?;

            let (row, _, _) = anim_config("idle", 0);
            let initial_icon = make_icon(&sheet, row, 0);

            let tray_rect_for_handler = tray_rect_state.clone();
            let tray = TrayIconBuilder::new()
                .icon(initial_icon)
                .tooltip("clowder")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "quit" => app.exit(0),
                    "about" => show_about_dialog(app),
                    _ => {}
                })
                .on_tray_icon_event({
                    let tray_rect_clone = tray_rect_for_handler;
                    move |tray, event| {
                        use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

                        let app = tray.app_handle();
                        let scale = app
                            .get_webview_window("main")
                            .and_then(|w| w.scale_factor().ok())
                            .unwrap_or(2.0);

                        // Keep the stored tray rect up-to-date from any event
                        {
                            let (px, py, sw, sh) = match &event {
                                TrayIconEvent::Click { rect, .. }
                                | TrayIconEvent::Move { rect, .. }
                                | TrayIconEvent::Enter { rect, .. } => {
                                    phys_coords(rect.position, rect.size, scale)
                                }
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };
                            if sw > 0.0 {
                                *tray_rect_clone.lock().unwrap() = Some((px, py, sw, sh));
                            }
                        }

                        // Toggle popup on left click
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            rect,
                            ..
                        } = event
                        {
                            if let Some(win) = app.get_webview_window("main") {
                                if win.is_visible().unwrap_or(false) {
                                    let _ = win.hide();
                                } else {
                                    // Hide bubble if it's showing
                                    if let Some(b) = app.get_webview_window("bubble") {
                                        let _ = b.hide();
                                    }
                                    let (pos_x, pos_y, sz_w, sz_h) =
                                        phys_coords(rect.position, rect.size, scale);
                                    let win_width_phys = 320.0 * scale;
                                    let win_height_phys = 480.0 * scale;
                                    let x = (pos_x + sz_w / 2.0 - win_width_phys / 2.0).max(0.0);
                                    let y = anchor_y(pos_y, sz_h, win_height_phys, 0.0);
                                    let _ = win.set_position(tauri::PhysicalPosition::new(
                                        x as i32, y as i32,
                                    ));
                                    let _ = win.show();
                                    let _ = win.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(app)?;

            // Hide popup on focus loss
            if let Some(win) = app.get_webview_window("main") {
                let win_clone = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = win_clone.hide();
                    }
                });
            }

            let app_handle = app.handle().clone();
            start_watchers(app_handle, session_map.clone(), state_map.clone());

            let app_for_anim = app.handle().clone();
            let stm = state_map.clone();
            let sess = session_map.clone();
            let tr = tray_rect_state.clone();
            let sheet_clone = sheet.clone();
            tauri::async_runtime::spawn(async move {
                animation_loop(app_for_anim, tray, stm, sess, tr, sheet_clone).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running clowder");
}
