# Clowder

Claude Code 세션을 모니터링하는 macOS 메뉴바 앱입니다. 픽셀 아트 고양이가 메뉴바 아이콘으로 표시되며 현재 세션 상태를 애니메이션으로 표현합니다.

## 상태

| 상태 | 설명 |
|------|------|
| `idle` | 대기 중 |
| `thinking` | 생각 중 |
| `working` | 도구 실행 중 |
| `done` | 작업 완료 (3초 후 idle로 복귀) |
| `scared` | 오류 발생 (2초 후 idle로 복귀) |

여러 세션이 동시에 활성화된 경우 우선순위가 높은 상태가 표시됩니다: `working > thinking > scared > done > idle`

## 구조

```
~/.claude/sessions/*.json        # Claude Code 세션 메타데이터 (읽기 전용)
~/.claude/clowder/state/*.json   # 세션별 고양이 상태 (훅이 기록)
```

Rust 백엔드가 두 디렉토리를 폴링하고 tokio 애니메이션 루프가 메뉴바 아이콘을 직접 업데이트합니다.

## 개발

```bash
npm run tauri dev    # 개발 모드 실행
npm run tauri build  # 프로덕션 빌드
```

## 요구사항

- macOS
- [Rust](https://rustup.rs/)
- Node.js 18+

## Claude Code 훅 연동

`.claude/settings.json`에 훅을 추가하면 Claude Code 작업 상태가 자동으로 고양이에 반영됩니다.

```json
{
  "hooks": {
    "PreToolUse": [...],
    "PostToolUse": [...],
    "Stop": [...]
  }
}
```

훅은 `~/.claude/clowder/state/<sessionId>.json` 파일에 상태를 기록합니다.
