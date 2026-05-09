//! Windows-only: prune dead `HKCU\Control Panel\NotifyIconSettings\<id>` entries
//! that point at clowder.exe paths which no longer exist.
//!
//! Background: Windows 11's "Other system tray icons" panel reads from
//! `HKCU\Control Panel\NotifyIconSettings\<numeric-id>`. Each subkey carries an
//! `ExecutablePath` and a few flags. Tauri's NSIS uninstaller template doesn't
//! touch this location, so installing → uninstalling → reinstalling to a
//! different path leaves the old subkey behind and the settings panel shows two
//! Clowder rows, the dead one's toggle no-ops.
//!
//! The traditional `IconStreams` / `PastIconsStream` cache wipe under
//! `HKCU\Software\Classes\Local Settings\…\TrayNotify\` does NOT clean this —
//! Win11's modern NotifyIconSettings is a separate registry surface.
//!
//! This runs on every app start (cheap — at most a few subkeys to enum) and is
//! idempotent: if no orphans exist, it's a no-op. The NSIS uninstaller hook
//! handles the standard uninstall flow; this catches the case where a user just
//! deletes the install folder and reinstalls elsewhere.
//!
//! ## Path format gotcha (the reason for the resolve step below)
//!
//! Windows stores `ExecutablePath` as `REG_SZ`, but for executables under any
//! Known Folder it uses a `{KF_ID}\<rest>` prefix instead of the literal
//! drive-letter path. For example clowder.exe at `%LOCALAPPDATA%\clowder\` is
//! stored as `{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}\clowder\clowder.exe`,
//! never as `C:\Users\…\AppData\Local\clowder\clowder.exe`.
//!
//! Pre-fix, this code did `Path::new(&exe_path).exists()` directly on the raw
//! registry value, which always returned `false` for KF_ID-prefixed paths and
//! deleted the *live* entry on every app start — preventing it from ever
//! being promoted into the Win11 Settings UI.

#[cfg(target_os = "windows")]
pub fn cleanup_orphan_notify_icons() {
    use std::path::{Path, PathBuf};
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let parent = match hkcu.open_subkey_with_flags(
        r"Control Panel\NotifyIconSettings",
        KEY_READ | KEY_WRITE,
    ) {
        Ok(k) => k,
        Err(_) => return, // key may not exist on older Windows builds
    };

    // Resolve the running executable once so we can self-protect: even if some
    // future change to the heuristic gets aggressive, we must never delete the
    // entry that points at *us*.
    let my_exe = std::env::current_exe().ok();

    // Collect names first; deleting while iterating would invalidate the enumerator.
    let names: Vec<String> = parent.enum_keys().filter_map(Result::ok).collect();

    for name in names {
        let Ok(sub) = parent.open_subkey(&name) else { continue };
        let Ok(exe_path) = sub.get_value::<String, _>("ExecutablePath") else { continue };

        // Scope the cleanup to clowder.exe entries only — touching other apps'
        // dead registrations would be surprising even if technically helpful.
        let lower = exe_path.to_ascii_lowercase();
        let is_clowder = lower.ends_with("\\clowder.exe") || lower.ends_with("/clowder.exe");
        if !is_clowder {
            continue;
        }

        // Resolve `{KF_ID}\<rest>` → real drive-letter path. Without this,
        // `Path::exists()` would return false for every Win11 entry and we'd
        // nuke our own live registration.
        let resolved = resolve_known_folder_prefix(&exe_path);
        let resolved_path = PathBuf::from(&resolved);

        // Self-protection: never delete an entry that resolves to our own
        // running executable, regardless of whether `.exists()` agrees.
        if let Some(my) = &my_exe {
            if paths_equal_loose(&resolved_path, my) {
                continue;
            }
        }

        if Path::new(&resolved).exists() {
            continue;
        }

        if let Err(e) = parent.delete_subkey_all(&name) {
            eprintln!(
                "[clowder] failed to remove orphan NotifyIconSettings\\{name} ({exe_path} -> {resolved}): {e}"
            );
        } else {
            eprintln!(
                "[clowder] removed orphan NotifyIconSettings\\{name} -> {exe_path} (resolved: {resolved})"
            );
        }
    }
}

/// Expand a leading `{KF_ID}` Known-Folder GUID prefix into its real drive-letter
/// path. Returns the input unchanged if there is no prefix or resolution fails.
#[cfg(target_os = "windows")]
fn resolve_known_folder_prefix(raw: &str) -> String {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    use windows::core::GUID;

    if !raw.starts_with('{') {
        return raw.to_string();
    }
    let Some(end) = raw.find('}') else {
        return raw.to_string();
    };
    // GUID::try_from expects 36-char form without braces.
    let guid_str = &raw[1..end];
    let Ok(guid) = GUID::try_from(guid_str) else {
        return raw.to_string();
    };
    let rest = &raw[end + 1..]; // typically begins with '\\'

    unsafe {
        match SHGetKnownFolderPath(&guid, KF_FLAG_DEFAULT, None) {
            Ok(pwstr) if !pwstr.is_null() => {
                let folder = pwstr.to_string().unwrap_or_default();
                CoTaskMemFree(Some(pwstr.as_ptr() as _));
                if folder.is_empty() {
                    raw.to_string()
                } else {
                    format!("{folder}{rest}")
                }
            }
            _ => raw.to_string(),
        }
    }
}

/// Loose path equality: case-insensitive string compare. Used for the
/// self-protection guard; we don't canonicalize because resolved KF_ID paths
/// already give us drive-letter form, which matches `current_exe()`.
#[cfg(target_os = "windows")]
fn paths_equal_loose(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(not(target_os = "windows"))]
pub fn cleanup_orphan_notify_icons() {}
