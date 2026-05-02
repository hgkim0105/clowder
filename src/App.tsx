import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessions } from "./hooks/useSessions";
import type { SessionStats, SessionWithState } from "./types";
import "./App.css";

function formatCwd(cwd: string): string {
  const shortened = cwd.replace(/^\/Users\/[^/]+/, "~");
  const parts = shortened.split("/").filter(Boolean);
  if (parts.length > 3) return "…/" + parts.slice(-2).join("/");
  return shortened;
}

function formatDuration(startedAt: number): string {
  const elapsed = Date.now() - startedAt;
  const minutes = Math.floor(elapsed / 60000);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m`;
  return "now";
}

function formatModel(model: string): string {
  const m = model.match(/claude-(sonnet|opus|haiku)-(\d+)-(\d+)/i);
  if (m) return `${m[1].charAt(0).toUpperCase() + m[1].slice(1)} ${m[2]}.${m[3]}`;
  return model.replace(/^claude-/, "").replace(/-\d{8}.*$/, "");
}

function formatTokens(n: number): string {
  if (n >= 1000) return `${Math.round(n / 1000)}k`;
  return String(n);
}

function formatPermMode(mode: string): string {
  if (mode === "acceptEdits") return "Auto-edit";
  if (mode === "bypassPermissions") return "Bypass";
  return mode;
}

function ContextBar({ stats }: { stats: SessionStats }) {
  const pct = Math.min(stats.inputTokens / stats.contextWindow, 1);
  const used = formatTokens(stats.inputTokens);
  const total = formatTokens(stats.contextWindow);
  const color =
    pct > 0.9 ? "#ff453a" : pct > 0.75 ? "#ff9f0a" : pct > 0.5 ? "#ffd60a" : "#30d158";

  return (
    <div className="ctx-row">
      <div className="ctx-bar-bg">
        <div className="ctx-bar-fill" style={{ width: `${pct * 100}%`, background: color }} />
      </div>
      <span className="ctx-text">
        {used} / {total}
      </span>
    </div>
  );
}

function StatsBadges({ stats }: { stats: SessionStats }) {
  const badges: string[] = [];
  if (stats.model) badges.push(formatModel(stats.model));
  if (stats.speed && stats.speed !== "standard") badges.push(stats.speed.charAt(0).toUpperCase() + stats.speed.slice(1));
  if (stats.hasThinking) badges.push("Thinking");
  if (stats.permissionMode) badges.push(formatPermMode(stats.permissionMode));

  return (
    <div className="badges-row">
      {badges.map((b, i) => (
        <span key={i} className="badge">{b}</span>
      ))}
    </div>
  );
}

function SessionRow({ session }: { session: SessionWithState }) {
  const isDone = session.state === "done";
  const isActive = session.state !== "idle" && !isDone;
  const { stats, stateUpdatedAt } = session;

  let dotClass = "dot-idle";
  let stateClass = "state-idle";
  let stateLabel = "idle";

  if (isDone) {
    dotClass = "dot-done";
    stateClass = "state-done";
    stateLabel = stateUpdatedAt
      ? `done · ${formatDuration(stateUpdatedAt)} ago`
      : "done";
  } else if (isActive) {
    dotClass = "dot-active";
    stateClass = "state-active";
    stateLabel = session.toolName ?? session.state;
  }

  return (
    <div className={`session-row ${isActive ? "session-active" : ""} ${isDone ? "session-done" : ""}`}>
      <div className="session-top">
        <div className="session-cwd">{formatCwd(session.info.cwd)}</div>
        <span className="session-duration">{formatDuration(session.info.startedAt)}</span>
      </div>
      <div className="state-row">
        <span className={`dot ${dotClass}`} />
        <span className={`state-text ${stateClass}`}>{stateLabel}</span>
      </div>
      {stats && <StatsBadges stats={stats} />}
      {stats && stats.inputTokens > 0 && <ContextBar stats={stats} />}
    </div>
  );
}

export default function App() {
  const sessions = useSessions();
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    timerRef.current = setInterval(() => {}, 30000);
    return () => { if (timerRef.current) clearInterval(timerRef.current); };
  }, []);

  const activeCount = sessions.filter((s) => s.state !== "idle").length;

  return (
    <div className="popup">
      <div className="header">
        <span className="header-title">Clowder</span>
        <span className="header-count">
          {sessions.length === 0
            ? "no sessions"
            : `${sessions.length} session${sessions.length > 1 ? "s" : ""}${activeCount > 0 ? ` · ${activeCount} active` : ""}`}
        </span>
      </div>
      <div className="session-list">
        {sessions.length === 0 ? (
          <div className="empty">No Claude Code sessions running</div>
        ) : (
          sessions.map((s) => <SessionRow key={s.info.sessionId} session={s} />)
        )}
      </div>
      <div className="footer">
        <button className="quit-btn" onClick={() => invoke("quit_app")}>Quit Clowder</button>
      </div>
    </div>
  );
}
