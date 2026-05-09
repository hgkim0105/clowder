import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./About.css";

const REPO_URL = "https://github.com/hgkim0105/clowder";
const AUTHOR_URL = "https://github.com/hgkim0105";

// Centralizes link handling: prevent the webview from navigating itself, then
// hand the URL to the OS via the opener plugin so it lands in the user's
// default browser.
function handleExternal(e: React.MouseEvent, url: string) {
  e.preventDefault();
  openUrl(url);
}

export default function About() {
  const [version, setVersion] = useState("");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(""));
  }, []);

  const close = () => getCurrentWebviewWindow().hide();

  // Press Esc to dismiss — matches what users expect from a small dialog.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="about-wrapper">
      <div className="about-card">
        <div className="about-drag" data-tauri-drag-region />
        <button className="about-close" onClick={close} aria-label="Close">×</button>
        <img src="/clowder-icon.png" className="about-icon" alt="Clowder" />
        <h1 className="about-title">Clowder</h1>
        {version && <div className="about-version">v{version}</div>}
        <p className="about-tagline">Claude Code cat companion</p>
        <p className="about-desc">
          Watches your Claude Code sessions
          <br />
          and brings them to life in the menu bar.
        </p>
        <div className="about-credit">
          Built with Tauri + Rust + React
          <br />
          by{" "}
          <a
            href={AUTHOR_URL}
            onClick={(e) => handleExternal(e, AUTHOR_URL)}
          >
            @hgkim0105
          </a>{" "}
          + Claude 🐱
        </div>
        <div className="about-links">
          <a
            href={REPO_URL}
            onClick={(e) => handleExternal(e, REPO_URL)}
          >
            View on GitHub →
          </a>
        </div>
      </div>
    </div>
  );
}
