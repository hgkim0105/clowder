# Clowder

Claude Code 세션을 모니터링하는 트레이 앱 (macOS 메뉴바 / Windows 작업표시줄). 픽셀 아트 고양이가 트레이 아이콘으로 표시되며, 세션 활동 상태에 따라 애니메이션이 바뀝니다.

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

## 설치 (다운로드)

[**최신 릴리즈에서 다운로드**](https://github.com/hgkim0105/clowder/releases/latest)

### macOS (Apple Silicon)

1. **`clowder_x.y.z_aarch64.dmg`** 다운로드 → 열어서 Clowder.app을 Applications 폴더로 드래그
2. Applications에서 **Clowder** 실행. 메뉴바에 픽셀 고양이가 나타납니다.

> ⚠️ **첫 실행 시 Gatekeeper 경고가 뜹니다.** 현재 빌드는 Apple Developer 인증서로 서명/notarize 되지 않아, 더블클릭하면 *"확인되지 않은 개발자라 열 수 없습니다"* 경고가 나옵니다. 두 가지 우회 방법:
>
> - **Finder에서**: Applications에서 Clowder를 **우클릭 → 열기 → 열기**. 한 번만 승인하면 이후엔 더블클릭으로 실행됩니다.
> - **터미널에서**: `xattr -dr com.apple.quarantine /Applications/clowder.app` 실행 후 `open /Applications/clowder.app`. quarantine 속성을 제거해 Gatekeeper 검사를 건너뜁니다.
>
> 정식 코드 서명/notarize 도입 진행 중 — `docs/auto-update-and-signing.md` 참조.

### Windows (x64)

1. **`clowder_x.y.z_x64-setup.exe`** (NSIS) 다운로드 → 실행해서 설치
2. 시작 메뉴 또는 설치된 위치에서 **Clowder** 실행. 작업표시줄 트레이에 픽셀 고양이가 나타납니다.

> ⚠️ **SmartScreen 경고가 뜹니다.** 현재 빌드는 Authenticode 인증서로 서명되지 않아, 인스톨러 실행 시 *"Windows에서 PC를 보호했습니다"* 파란 화면이 나옵니다. **추가 정보 → 실행** 버튼을 눌러 진행하세요. (정식 코드 서명 도입 진행 중 — `docs/auto-update-and-signing.md` 참조)

> 첫 실행 시 Clowder가 자동으로 `~/.claude/clowder/hooks/`에 훅 스크립트를 배치하고 `~/.claude/settings.json`에 등록합니다 (idempotent — 매 실행마다 자가 복구).
> 이미 실행 중인 Claude Code 세션은 훅을 다시 로드하기 위해 재시작해야 합니다.
> macOS는 Apple Silicon (M1/M2/M3/M4) 전용. Intel Mac/x86 Windows ARM은 소스에서 빌드하세요.
> 자동시작이 기본 활성화됩니다 (macOS LaunchAgent / Windows 레지스트리 Run 키). 시스템 설정에서 끌 수 있습니다.
> 수동으로 훅만 다시 등록하고 싶으면 `scripts/install-hooks.{sh,ps1}`을 직접 실행할 수 있습니다.

### Windows 재설치 시 트레이 중복 방지 (수동 검증)

Win11 "설정 → 개인 설정 → 작업 표시줄 → 기타 시스템 트레이 아이콘" 목록에 Clowder 항목이 두 개 뜨는 일이 없도록, 인스톨러는 uninstall 시 `HKCU\Control Panel\NotifyIconSettings`의 자기 entry를 정리하고, 앱은 첫 실행 시 존재하지 않는 경로를 가리키는 `clowder.exe` orphan 키를 자동 제거합니다.

검증 절차:

1. 깨끗한 상태에서 시작 — `reg query "HKCU\Control Panel\NotifyIconSettings" /s /f clowder.exe` 결과에 clowder 항목이 없는지 확인
2. v0.1.8+ 인스톨러로 설치 → 한 번 실행 → 트레이 설정 목록에 Clowder 1개 표시
3. 제거 (uninstaller)
4. 다른 경로로 재설치 (예: per-user `%LOCALAPPDATA%` ↔ per-machine `Program Files`)
5. 트레이 설정 목록에 **Clowder 항목이 1개만** 보여야 함 (구버전 동작에서는 2개)

## 소스에서 빌드

요구사항: [Rust](https://rustup.rs/) · Node.js 18+ · Python 3 · 플랫폼별 네이티브 툴체인 (macOS: Xcode CLT / Windows: Visual Studio C++ Build Tools)

```bash
git clone https://github.com/hgkim0105/clowder.git
cd clowder
npm install
npm run tauri build           # macOS → bundle/dmg/clowder_*.dmg, Windows → bundle/nsis/clowder_*-setup.exe
```

설치 후 처음 실행하면 훅이 자동으로 등록됩니다. 수동으로 다시 등록하고 싶을 때만 `./scripts/install-hooks.sh` (또는 `scripts\install-hooks.ps1`) 사용.

## 개발

```bash
npm run tauri dev    # 개발 모드 실행
npm run tauri build  # 프로덕션 빌드
```

## Claude Code 훅 연동

앱 첫 실행 시 자동으로 처리되는 작업입니다 (`src-tauri/src/hook_install.rs` 참조). 직접 구성하려면:

1. `~/.claude/clowder/hooks/{thinking,working,done}.py` 세 파일을 생성하고 `chmod +x`. 각 스크립트는 `stdin`으로 받은 `session_id`/`tool_name`을 `~/.claude/clowder/state/<sid>.json`에 `{"state": ..., "updated_at": ...}` 형태로 기록합니다 (실제 구현은 `scripts/install-hooks.sh` 참조).
2. `~/.claude/settings.json`에 다음 매핑 추가:

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
