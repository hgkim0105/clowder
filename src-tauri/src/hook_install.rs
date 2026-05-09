// Idempotently install Claude Code hooks on every app start. Replaces the
// standalone `install-hooks.sh` / `install-hooks.ps1` for users who installed
// via dmg/msi/exe — those bundles can't run post-install scripts uniformly
// (DMG has no install-time hook). Running on every launch self-heals after
// app upgrades and is cheap when nothing has changed.

use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const HOOK_TEMPLATE: &str = r#"#!/usr/bin/env python3
import sys, json, time, pathlib

data = json.load(sys.stdin)
sid = data.get("session_id", "unknown")
tool = data.get("tool_name")
out = {"session_id": sid, "state": "__STATE__", "updated_at": int(time.time() * 1000)}
if tool and "__STATE__" == "working":
    out["tool_name"] = tool
p = pathlib.Path.home() / ".claude" / "clowder" / "state" / f"{sid}.json"
p.parent.mkdir(parents=True, exist_ok=True)
p.write_text(json.dumps(out))
"#;

const HOOK_NAMES: &[(&str, &str)] = &[
    ("thinking", "thinking"),
    ("working", "working"),
    ("done", "done"),
];

const EVENT_MAPPING: &[(&str, &str)] = &[
    ("UserPromptSubmit", "thinking"),
    ("PreToolUse", "working"),
    ("PostToolUse", "thinking"),
    ("Stop", "done"),
];

fn render_hook(state: &str) -> String {
    HOOK_TEMPLATE.replace("__STATE__", state)
}

// macOS uses the raw path and relies on the shebang. Windows ignores shebangs,
// so wrap the path with `python "..."`. Forward slashes either way to keep
// JSON escaping simple — Windows accepts them in command-line paths.
fn hook_command(hook_dir: &Path, name: &str) -> String {
    let path = hook_dir.join(format!("{name}.py"));
    let path_str = path.to_string_lossy().replace('\\', "/");
    if cfg!(target_os = "windows") {
        format!("python \"{path_str}\"")
    } else {
        path_str
    }
}

pub fn ensure_hooks_installed() {
    let Some(home) = dirs::home_dir() else {
        eprintln!("clowder: no home dir; skipping hook install");
        return;
    };
    if let Err(e) = install_in(&home) {
        eprintln!("clowder: hook install failed: {e}");
    }
}

fn install_in(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let hook_dir = home.join(".claude").join("clowder").join("hooks");
    let settings_path = home.join(".claude").join("settings.json");
    fs::create_dir_all(&hook_dir)?;

    for (name, state) in HOOK_NAMES {
        let path = hook_dir.join(format!("{name}.py"));
        let body = render_hook(state);
        let needs_write = fs::read_to_string(&path).map_or(true, |existing| existing != body);
        if needs_write {
            fs::write(&path, &body)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&path, perms)?;
            }
        }
    }

    merge_settings(&settings_path, &hook_dir)?;
    Ok(())
}

fn merge_settings(settings_path: &Path, hook_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let original_text = fs::read_to_string(settings_path).unwrap_or_default();
    // Strip a UTF-8 BOM if present. PowerShell 5.1's `Set-Content -Encoding utf8`
    // and various Windows editors prepend `EF BB BF`, which serde_json refuses
    // as a JSON prefix — without this the merge silently no-ops on those files.
    let parse_text = original_text.strip_prefix('\u{FEFF}').unwrap_or(&original_text);
    let mut settings: Value = if parse_text.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(parse_text) {
            Ok(v) => v,
            Err(_) => {
                eprintln!("clowder: ~/.claude/settings.json is not valid JSON; skipping hook merge");
                return Ok(());
            }
        }
    };

    let root = settings.as_object_mut().ok_or("settings.json root not an object")?;
    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("`hooks` is not an object")?;

    for (event, hook_name) in EVENT_MAPPING {
        let cmd = hook_command(hook_dir, hook_name);
        let new_entry = json!({"type": "command", "command": cmd});

        let matchers = hooks
            .entry(event.to_string())
            .or_insert_with(|| json!([]));
        let arr = matchers
            .as_array_mut()
            .ok_or("event matchers value is not an array")?;

        // Remove our own prior entries so the merge stays idempotent across
        // upgrades and across path-format changes (e.g. moving from raw path
        // to `python "..."` on Windows).
        for m in arr.iter_mut() {
            if let Some(inner) = m.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                inner.retain(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .map_or(true, |s| !s.contains("clowder/hooks") && !s.contains("clowder\\hooks"))
                });
            }
        }
        arr.retain(|m| {
            m.get("hooks")
                .and_then(|h| h.as_array())
                .is_some_and(|a| !a.is_empty())
        });
        arr.push(json!({"matcher": "", "hooks": [new_entry]}));
    }

    // Pretty-print with 2-space indent + trailing newline to match what the
    // standalone install-hooks scripts produce.
    let new_text = serde_json::to_string_pretty(&settings)? + "\n";
    if new_text != original_text {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(settings_path, new_text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    fn unique_tmp(label: &str) -> PathBuf {
        let mut p = env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("clowder-test-{label}-{nanos}-{}", std::process::id()));
        p
    }

    #[test]
    fn fresh_install_writes_three_hook_files_and_settings() {
        let home = unique_tmp("fresh");
        fs::create_dir_all(&home).unwrap();
        install_in(&home).unwrap();

        for name in ["thinking", "working", "done"] {
            let path = home.join(".claude/clowder/hooks").join(format!("{name}.py"));
            let body = fs::read_to_string(&path).unwrap();
            assert!(body.contains(&format!("\"state\": \"{name}\"")), "{name}.py missing state");
        }

        let settings_text = fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        let settings: Value = serde_json::from_str(&settings_text).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        for event in ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"] {
            let arr = hooks[event].as_array().unwrap();
            assert_eq!(arr.len(), 1, "expected one matcher for {event}");
            let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
            assert!(cmd.contains("clowder/hooks"), "{event} cmd = {cmd}");
        }

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn idempotent_second_run_does_not_change_file_or_duplicate_entries() {
        let home = unique_tmp("idem");
        fs::create_dir_all(&home).unwrap();
        install_in(&home).unwrap();
        let first = fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        install_in(&home).unwrap();
        let second = fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        assert_eq!(first, second, "second run produced different output");

        let settings: Value = serde_json::from_str(&second).unwrap();
        for event in ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"] {
            assert_eq!(settings["hooks"][event].as_array().unwrap().len(), 1);
        }

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn preserves_unrelated_hook_entries_and_root_keys() {
        let home = unique_tmp("preserve");
        fs::create_dir_all(home.join(".claude")).unwrap();
        let initial = serde_json::json!({
            "model": "claude-opus-4-7",
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "/some/other/tool.sh"}]
                }],
                "Stop": [{
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "/usr/bin/notify-send done"}]
                }]
            }
        });
        fs::write(
            home.join(".claude/settings.json"),
            serde_json::to_string_pretty(&initial).unwrap() + "\n",
        )
        .unwrap();

        install_in(&home).unwrap();

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap()).unwrap();
        assert_eq!(settings["model"], "claude-opus-4-7", "root key dropped");

        // Pre-existing PreToolUse Bash matcher must survive.
        let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
        let bash_kept = pre.iter().any(|m| {
            m["hooks"][0]["command"]
                .as_str()
                .map_or(false, |c| c.contains("other/tool.sh"))
        });
        assert!(bash_kept, "non-clowder Bash matcher was dropped");

        // Pre-existing Stop notify-send must survive alongside ours.
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        let notify_kept = stop.iter().any(|m| {
            m["hooks"][0]["command"]
                .as_str()
                .map_or(false, |c| c.contains("notify-send"))
        });
        assert!(notify_kept, "non-clowder notify matcher was dropped");

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn reinstall_replaces_old_clowder_path_without_duplication() {
        let home = unique_tmp("replace");
        fs::create_dir_all(home.join(".claude")).unwrap();
        // Simulate a settings.json with a stale clowder path (e.g. from a
        // different user dir or an older install location).
        let stale = serde_json::json!({
            "hooks": {
                "Stop": [{
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "/old/path/to/.claude/clowder/hooks/done.py"}]
                }]
            }
        });
        fs::write(
            home.join(".claude/settings.json"),
            serde_json::to_string_pretty(&stale).unwrap() + "\n",
        )
        .unwrap();

        install_in(&home).unwrap();

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap()).unwrap();
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1, "stale + new produced {} entries", stop.len());
        let cmd = stop[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(!cmd.contains("/old/path/"), "stale entry not removed: {cmd}");

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn handles_utf8_bom_prefixed_settings_json() {
        // Regression: PowerShell 5.1's `Set-Content -Encoding utf8` writes a
        // UTF-8 BOM, which serde_json rejects as a prefix. Without BOM
        // stripping the merge silently no-ops and hook entries are never
        // written even though the hook .py files do get created.
        let home = unique_tmp("bom");
        fs::create_dir_all(home.join(".claude")).unwrap();
        let body = "{\n    \"theme\": \"dark\",\n    \"hooks\": {}\n}\n";
        let with_bom = format!("\u{FEFF}{body}");
        fs::write(home.join(".claude/settings.json"), with_bom).unwrap();

        install_in(&home).unwrap();

        let after =
            fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        assert!(
            !after.starts_with('\u{FEFF}'),
            "rewrite should drop the BOM"
        );
        let settings: Value = serde_json::from_str(&after).unwrap();
        assert_eq!(settings["theme"], "dark", "unrelated key dropped");
        for event in ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"] {
            assert_eq!(
                settings["hooks"][event].as_array().unwrap().len(),
                1,
                "{event} entry missing — BOM still blocking merge"
            );
        }

        let _ = fs::remove_dir_all(&home);
    }
}
