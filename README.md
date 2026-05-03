# Clowder

Claude Code 세션을 모니터링하는 macOS 메뉴바 앱. 픽셀 아트 고양이가 메뉴바 아이콘으로 표시되며, 세션 활동 상태에 따라 애니메이션이 바뀝니다.

## 기능

### 메뉴바 고양이 아이콘

픽셀 아트 고양이가 메뉴바에 상주하며, Claude Code 세션 상태에 따라 애니메이션이 바뀝니다.

| 상태 | 트리거 | 애니메이션 |
|------|--------|-----------|
| `idle` | 모든 세션 대기 중 (또는 done이 60초 이상 경과) | 느긋하게 앉아 있음 |
| `working` | 도구 실행 중인 세션 있음 | 활발하게 움직임 |
| `done` | 최근 60초 이내 완료된 세션 있음 (working 없음) | 4초간 완료 포즈 후 idle 복귀 |

동시에 작업 중인 세션이 많을수록 애니메이션이 빨라집니다: 1개→12fps, 2개→16fps, 3개+→20fps

> Claude가 응답을 마치고 사용자 입력을 기다리는 동안 `done` 상태가 유지되지만, 60초가 지나면 표시상으로는 idle로 간주됩니다 (`DONE_FRESHNESS_SECS`).

### 세션 팝업

트레이 아이콘을 **좌클릭**하면 현재 활성 세션 목록이 담긴 패널이 열립니다.

각 세션 행에서 확인할 수 있는 정보:

- **작업 디렉토리** — 깊은 경로는 마지막 2단계로 축약
- **상태 표시** — 주황 펄스 (working) / 초록 점 + 경과 시간 (done, 60초 내) / 회색 (idle)
- **모델 뱃지** — 사용 중인 Claude 모델명
- **속도 뱃지** — 비표준 속도일 때 표시
- **Thinking 뱃지** — 확장 사고(extended thinking) 활성 시 표시
- **권한 모드 뱃지** — 현재 permission mode
- **컨텍스트 바** — 모델별 컨텍스트 윈도우 대비 토큰 사용량을 색상으로 표시 (Claude 4.x 계열은 1M, 그 외는 200k)

패널은 포커스를 잃으면 자동으로 닫힙니다. 하단의 "Quit Clowder" 버튼으로 앱을 종료할 수 있습니다.

### 완료 알림 (말풍선)

세션이 `done` 상태로 전환되면 메뉴바 아이콘 아래에 말풍선이 4초간 표시됩니다.

- 완료된 세션의 작업 디렉토리, 모델명, 총 토큰 수 표시
- 같은 작업 디렉토리의 세션은 가장 최근 1개로 합쳐서 표시 (cwd 단위 dedupe)
- 최대 2개 행까지 한 번에 표시
- 팝업 패널이 열려 있으면 억제됨
- 키보드 포커스를 빼앗지 않음
- 60초 이상 지난 done은 버블에서 제외됨

## 구조

```
~/.claude/sessions/*.json                        # Claude Code 세션 메타데이터 (읽기 전용)
~/.claude/clowder/state/*.json                   # 세션별 상태 (훅이 기록)
~/.claude/projects/<cwd-/-as-dash>/<id>.jsonl    # 대화 이력 (모델/토큰/속도 소스)
```

Rust 백엔드가 두 디렉토리를 감시(notify 크레이트, 300–500ms 폴링)하고, 50ms 틱 애니메이션 루프가 메뉴바 아이콘을 직접 업데이트합니다.

## 설치

### 요구사항

- macOS
- [Rust](https://rustup.rs/)
- Node.js 18+

### 앱 설치

```bash
# 저장소 클론
git clone https://github.com/hgkim0105/clowder.git
cd clowder

# 의존성 설치
npm install

# 프로덕션 빌드
npm run tauri build
```

빌드가 완료되면 `src-tauri/target/release/bundle/dmg/` 아래에 `.dmg` 파일이 생성됩니다. DMG를 열어 Clowder.app을 Applications 폴더로 드래그하세요.

### 훅 스크립트 배포

앱을 실행하기 전에 Claude Code 훅 스크립트를 설치해야 합니다.

```bash
mkdir -p ~/.claude/clowder/hooks ~/.claude/clowder/state
```

아래 내용으로 각 파일을 생성하세요.

**`~/.claude/clowder/hooks/thinking.py`**
```python
#!/usr/bin/env python3
import sys, json, os, pathlib

data = json.load(sys.stdin)
session_id = data.get("session_id", "unknown")
state_dir = pathlib.Path.home() / ".claude" / "clowder" / "state"
state_dir.mkdir(parents=True, exist_ok=True)
(state_dir / f"{session_id}.json").write_text(json.dumps({"state": "thinking"}))
```

**`~/.claude/clowder/hooks/working.py`**
```python
#!/usr/bin/env python3
import sys, json, pathlib

data = json.load(sys.stdin)
session_id = data.get("session_id", "unknown")
state_dir = pathlib.Path.home() / ".claude" / "clowder" / "state"
state_dir.mkdir(parents=True, exist_ok=True)
(state_dir / f"{session_id}.json").write_text(json.dumps({"state": "working"}))
```

**`~/.claude/clowder/hooks/done.py`**
```python
#!/usr/bin/env python3
import sys, json, pathlib

data = json.load(sys.stdin)
session_id = data.get("session_id", "unknown")
state_dir = pathlib.Path.home() / ".claude" / "clowder" / "state"
state_dir.mkdir(parents=True, exist_ok=True)
(state_dir / f"{session_id}.json").write_text(json.dumps({"state": "done"}))
```

파일 생성 후 실행 권한을 부여하세요.

```bash
chmod +x ~/.claude/clowder/hooks/*.py
```

## 개발

```bash
npm run tauri dev    # 개발 모드 실행
npm run tauri build  # 프로덕션 빌드
```

## Claude Code 훅 연동

`~/.claude/settings.json`에 훅을 등록하면 Claude Code 작업 상태가 자동으로 고양이에 반영됩니다.

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "matcher": "", "hooks": [{ "type": "command", "command": "~/.claude/clowder/hooks/thinking.py" }] }],
    "PreToolUse":       [{ "matcher": "", "hooks": [{ "type": "command", "command": "~/.claude/clowder/hooks/working.py" }] }],
    "PostToolUse":      [{ "matcher": "", "hooks": [{ "type": "command", "command": "~/.claude/clowder/hooks/thinking.py" }] }],
    "Stop":             [{ "matcher": "", "hooks": [{ "type": "command", "command": "~/.claude/clowder/hooks/done.py" }] }]
  }
}
```

훅 스크립트는 `~/.claude/clowder/hooks/`에 위치하며, `~/.claude/clowder/state/<sessionId>.json`에 상태를 기록합니다.
