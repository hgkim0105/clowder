import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SessionWithState } from "../types";

export function useSessions() {
  const [sessions, setSessions] = useState<SessionWithState[]>([]);

  useEffect(() => {
    // Initial load
    invoke<SessionWithState[]>("get_sessions")
      .then(setSessions)
      .catch(console.error);

    // Live updates via events
    const unlisten = listen<SessionWithState[]>("sessions-update", (event) => {
      setSessions(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return sessions;
}
