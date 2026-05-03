#!/usr/bin/env bash
# Install Clowder hook scripts and register them in ~/.claude/settings.json.
# Idempotent — safe to re-run.
set -euo pipefail

HOOK_DIR="${HOME}/.claude/clowder/hooks"
STATE_DIR="${HOME}/.claude/clowder/state"
SETTINGS="${HOME}/.claude/settings.json"

mkdir -p "$HOOK_DIR" "$STATE_DIR"

write_hook() {
  local name="$1" state="$2"
  cat > "$HOOK_DIR/$name.py" <<PY
#!/usr/bin/env python3
import sys, json, time, pathlib

data = json.load(sys.stdin)
sid = data.get("session_id", "unknown")
tool = data.get("tool_name")
out = {"session_id": sid, "state": "$state", "updated_at": int(time.time() * 1000)}
if tool and "$state" == "working":
    out["tool_name"] = tool
p = pathlib.Path.home() / ".claude" / "clowder" / "state" / f"{sid}.json"
p.parent.mkdir(parents=True, exist_ok=True)
p.write_text(json.dumps(out))
PY
  chmod +x "$HOOK_DIR/$name.py"
}

write_hook thinking thinking
write_hook working  working
write_hook done     done

# Merge hook entries into ~/.claude/settings.json (creates the file if missing)
python3 - "$SETTINGS" "$HOOK_DIR" <<'PY'
import json, pathlib, sys

settings_path = pathlib.Path(sys.argv[1])
hook_dir = sys.argv[2]
mapping = {
    "UserPromptSubmit": f"{hook_dir}/thinking.py",
    "PreToolUse":       f"{hook_dir}/working.py",
    "PostToolUse":      f"{hook_dir}/thinking.py",
    "Stop":             f"{hook_dir}/done.py",
}

settings = {}
if settings_path.exists():
    try:
        settings = json.loads(settings_path.read_text())
    except json.JSONDecodeError:
        print(f"warn: {settings_path} is not valid JSON; aborting merge", file=sys.stderr)
        sys.exit(1)

hooks = settings.setdefault("hooks", {})
for event, cmd in mapping.items():
    entry = {"type": "command", "command": cmd}
    matchers = hooks.setdefault(event, [])
    # remove any existing clowder entry for this event so we stay idempotent
    for m in matchers:
        m["hooks"] = [h for h in m.get("hooks", []) if "clowder/hooks" not in h.get("command", "")]
    matchers = [m for m in matchers if m.get("hooks")]
    matchers.append({"matcher": "", "hooks": [entry]})
    hooks[event] = matchers

settings_path.parent.mkdir(parents=True, exist_ok=True)
settings_path.write_text(json.dumps(settings, indent=2) + "\n")
print(f"updated {settings_path}")
PY

echo "Clowder hooks installed at $HOOK_DIR"
echo "Restart any running Claude Code sessions to pick up the hooks."
