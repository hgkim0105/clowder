# Install Clowder hook scripts and register them in ~/.claude/settings.json.
# Idempotent — safe to re-run. Windows equivalent of install-hooks.sh.
# Requires Python 3 on PATH (used by the hook bodies and the JSON merge).

$ErrorActionPreference = 'Stop'

$HookDir  = Join-Path $env:USERPROFILE '.claude\clowder\hooks'
$StateDir = Join-Path $env:USERPROFILE '.claude\clowder\state'
$Settings = Join-Path $env:USERPROFILE '.claude\settings.json'

New-Item -ItemType Directory -Force -Path $HookDir, $StateDir | Out-Null

function Write-Hook($name, $state) {
    # Double-quoted here-string lets PowerShell substitute $state at install
    # time (matches the bash version's behavior). Python's f-string braces
    # `{sid}` aren't PowerShell-special so they pass through verbatim.
    $py = @"
#!/usr/bin/env python
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
"@
    Set-Content -Path (Join-Path $HookDir "$name.py") -Value $py -Encoding utf8
}

Write-Hook 'thinking' 'thinking'
Write-Hook 'working'  'working'
Write-Hook 'done'     'done'

# Merge hook entries into ~/.claude/settings.json. Use forward slashes in the
# command path so JSON escaping stays simple. Wrap the whole command in
# `python "..."` because Windows doesn't honor Python shebangs and we want
# Claude Code to invoke the interpreter directly.
$hookDirFwd = $HookDir -replace '\\', '/'

$pyMerge = @'
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
    entry = {"type": "command", "command": f'python "{cmd}"'}
    matchers = hooks.setdefault(event, [])
    for m in matchers:
        m["hooks"] = [h for h in m.get("hooks", []) if "clowder/hooks" not in h.get("command", "")]
    matchers = [m for m in matchers if m.get("hooks")]
    matchers.append({"matcher": "", "hooks": [entry]})
    hooks[event] = matchers

settings_path.parent.mkdir(parents=True, exist_ok=True)
settings_path.write_text(json.dumps(settings, indent=2) + "\n")
print(f"updated {settings_path}")
'@

$pyMerge | python - $Settings $hookDirFwd

Write-Output "Clowder hooks installed at $HookDir"
Write-Output "Restart any running Claude Code sessions to pick up the hooks."
