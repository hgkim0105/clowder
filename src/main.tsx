import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import App from "./App";
import Bubble from "./Bubble";

const label = getCurrentWebviewWindow().label;

// Tag the platform so CSS can adjust popup anchoring. On Windows the tray sits
// at the bottom of the screen so the popup card needs to stick to the bottom
// of its (transparent, fixed-height) window to land just above the taskbar.
const isWindows = navigator.userAgent.includes("Windows");
document.documentElement.setAttribute(
  "data-platform",
  isWindows ? "windows" : "macos",
);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {label === "bubble" ? <Bubble /> : <App />}
  </React.StrictMode>
);
