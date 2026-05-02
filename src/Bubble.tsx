import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./Bubble.css";

interface BubbleSess {
  cwd: string;
  model: string | null;
  inputTokens: number;
  outputTokens: number;
}

function formatCwd(cwd: string): string {
  const shortened = cwd.replace(/^\/Users\/[^/]+/, "~");
  const parts = shortened.split("/").filter(Boolean);
  if (parts.length > 3) return "…/" + parts.slice(-2).join("/");
  return shortened;
}

function formatModel(model: string): string {
  const m = model.match(/claude-(sonnet|opus|haiku)-(\d+)-(\d+)/i);
  if (m) return `${m[1][0].toUpperCase() + m[1].slice(1)} ${m[2]}.${m[3]}`;
  return model.replace(/^claude-/, "").replace(/-\d{8}.*$/, "");
}

function formatTokens(n: number): string {
  if (n >= 1000) return `${Math.round(n / 1000)}k`;
  return String(n);
}

export default function Bubble() {
  const [sessions, setSessions] = useState<BubbleSess[]>([]);

  useEffect(() => {
    const unlisten = listen<BubbleSess[]>("show-bubble", (e) => {
      setSessions(e.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    if (sessions.length === 0) return;
    const win = getCurrentWebviewWindow();
    const timer = setTimeout(async () => {
      await win.hide();
      setSessions([]);
    }, 4000);
    return () => clearTimeout(timer);
  }, [sessions]);

  if (sessions.length === 0) return null;

  return (
    <div className="bubble-wrapper">
      <div className="bubble-arrow" />
      <div className="bubble-body">
        {sessions.slice(0, 2).map((s, i) => {
          const totalTok = s.inputTokens + s.outputTokens;
          return (
            <div key={i} className="bubble-row">
              <span className="bubble-check">✓</span>
              <div className="bubble-info">
                <span className="bubble-cwd">{formatCwd(s.cwd)}</span>
                <span className="bubble-meta">
                  {s.model ? formatModel(s.model) : "Claude"}
                  {totalTok > 0 ? ` · ${formatTokens(totalTok)} tok` : ""}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
