# Clowder

Claude Code 세션을 모니터링하는 macOS 데스크탑 오버레이 앱입니다. 활성 세션마다 픽셀 아트 고양이 한 마리가 나타나 현재 작업 상태를 애니메이션으로 표현합니다.

![preview](public/FREE_Cat%202D%20Pixel%20Art/preview.gif)

## 상태

| 상태 | 설명 |
|------|------|
| `idle` | 대기 중 |
| `thinking` | 생각 중 |
| `working` | 도구 실행 중 |
| `done` | 작업 완료 (3초 후 idle로 복귀) |
| `scared` | 오류 발생 (2초 후 idle로 복귀) |

## 구조

```
~/.claude/sessions/*.json        # Claude Code 세션 메타데이터 (읽기 전용)
~/.claude/clowder/state/*.json   # 세션별 고양이 상태 (훅이 기록)
```

Rust 백엔드가 두 디렉토리를 폴링하고, Tauri IPC를 통해 React 프론트엔드에 실시간으로 상태를 전달합니다.

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

## 크레딧

픽셀 아트: [FREE Cat 2D Pixel Art](public/FREE_Cat%202D%20Pixel%20Art/License.txt)
