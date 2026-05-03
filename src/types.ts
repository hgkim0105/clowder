export interface SessionInfo {
  pid: number;
  sessionId: string;
  cwd: string;
  startedAt: number;
  kind: string;
  entrypoint?: string;
}

export interface SessionStats {
  model: string | null;
  inputTokens: number;
  outputTokens: number;
  contextWindow: number;
  speed: string | null;
  permissionMode: string | null;
  hasThinking: boolean;
}

export interface SessionWithState {
  info: SessionInfo;
  state: string;
  toolName?: string;
  stateUpdatedAt?: number | null;
  stats?: SessionStats | null;
}
