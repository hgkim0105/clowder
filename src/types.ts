export type CatState = "idle" | "thinking" | "working" | "done" | "scared";

export interface SessionInfo {
  pid: number;
  sessionId: string;
  cwd: string;
  startedAt: number;
  kind: string;
  entrypoint?: string;
}

export interface SessionWithState {
  info: SessionInfo;
  state: CatState;
  toolName?: string;
}

export interface AnimConfig {
  row: number;
  frameCount: number;
  fps: number;
}

// "Cat Sprite Sheet.png": 256x320, 32x32 per frame, 8 cols x 10 rows
export const ANIM_CONFIG: Record<CatState, AnimConfig> = {
  idle:     { row: 0, frameCount: 4, fps: 6  },
  thinking: { row: 1, frameCount: 4, fps: 8  },
  working:  { row: 4, frameCount: 8, fps: 12 },
  scared:   { row: 5, frameCount: 8, fps: 14 },
  done:     { row: 6, frameCount: 4, fps: 5  },
};

export const FRAME_SIZE = 32;
export const DISPLAY_SCALE = 3; // 32 * 3 = 96px
export const SPRITE_SHEET_URL = "/Cat Sprite Sheet.png";
