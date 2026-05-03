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

## 설치 (다운로드)

[**최신 릴리즈에서 다운로드**](https://github.com/hgkim0105/clowder/releases/latest)

1. **`clowder_0.1.0_aarch64.dmg`** 다운로드 → 열어서 Clowder.app을 Applications 폴더로 드래그
2. 터미널에서 훅 등록 (한 줄):
   ```bash
   curl -fsSL https://github.com/hgkim0105/clowder/releases/latest/download/install-hooks.sh | bash
   ```
3. Applications에서 **Clowder** 실행. 메뉴바에 픽셀 고양이가 나타납니다.

> 이미 실행 중인 Claude Code 세션은 훅을 다시 로드하기 위해 재시작해야 합니다.
> `install-hooks.sh`는 idempotent — 다시 실행해도 중복 등록되지 않습니다.
> Apple Silicon (M1/M2/M3/M4) 전용. Intel Mac은 소스에서 빌드하세요.

## 소스에서 빌드

요구사항: macOS · [Rust](https://rustup.rs/) · Node.js 18+ · Python 3

```bash
git clone https://github.com/hgkim0105/clowder.git
cd clowder
npm install
npm run tauri build           # → src-tauri/target/release/bundle/dmg/clowder_*.dmg
./scripts/install-hooks.sh    # 훅 스크립트 + ~/.claude/settings.json 자동 등록
```

## 개발

```bash
npm run tauri dev    # 개발 모드 실행
npm run tauri build  # 프로덕션 빌드
```

## Claude Code 훅 연동

`scripts/install-hooks.sh`가 자동으로 처리하는 작업입니다. 직접 구성하려면:

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
