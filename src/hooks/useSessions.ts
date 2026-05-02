import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SessionWithState } from "../types";

export function useSessions() {
  const [sessions, setSessions] = useState<SessionWithState[]>([]);

  useEffect(() => {
    const refresh = () =>
      invoke<SessionWithState[]>("get_sessions")
        .then(setSessions)
        .catch(console.error);

    refresh();

    // Re-fetch (with stats) on any state change
    const unlisten = listen("sessions-update", refresh);

    // Periodic refresh every 10s for token updates
    const timer = setInterval(refresh, 10_000);

    return () => {
      unlisten.then((fn) => fn());
      clearInterval(timer);
    };
  }, []);

  return sessions;
}
