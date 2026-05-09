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

#[cfg(target_os = "windows")]
pub fn cleanup_orphan_notify_icons() {
    use std::path::Path;
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

        if Path::new(&exe_path).exists() {
            continue;
        }

        if let Err(e) = parent.delete_subkey_all(&name) {
            eprintln!(
                "[clowder] failed to remove orphan NotifyIconSettings\\{name} ({exe_path}): {e}"
            );
        } else {
            eprintln!("[clowder] removed orphan NotifyIconSettings\\{name} -> {exe_path}");
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn cleanup_orphan_notify_icons() {}
