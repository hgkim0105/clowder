import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import App from "./App";
import Bubble from "./Bubble";

const label = getCurrentWebviewWindow().label;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {label === "bubble" ? <Bubble /> : <App />}
  </React.StrictMode>
);
