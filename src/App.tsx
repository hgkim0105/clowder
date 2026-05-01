import { useEffect, useRef, useState } from "react";
import { Cat } from "./components/Cat";
import { useSessions } from "./hooks/useSessions";
import type { CatState } from "./types";
import { SPRITE_SHEET_URL } from "./types";
import "./App.css";

// "done" → idle after 3s, "scared" → idle after 2s
function useDisplayState(rawState: CatState): CatState {
  const [state, setState] = useState<CatState>(rawState);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);

    if (rawState === "done") {
      setState("done");
      timerRef.current = setTimeout(() => setState("idle"), 3000);
    } else if (rawState === "scared") {
      setState("scared");
      timerRef.current = setTimeout(() => setState("idle"), 2000);
    } else {
      setState(rawState);
    }
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [rawState]);

  return state;
}

function CatSession({
  state: rawState,
  label,
  spriteSheet,
}: {
  state: CatState;
  label: string;
  spriteSheet: HTMLImageElement | null;
}) {
  const displayState = useDisplayState(rawState);
  return <Cat state={displayState} spriteSheet={spriteSheet} label={label} />;
}

export default function App() {
  const sessions = useSessions();
  const [spriteSheet, setSpriteSheet] = useState<HTMLImageElement | null>(null);

  useEffect(() => {
    const img = new Image();
    img.src = SPRITE_SHEET_URL;
    img.onload = () => setSpriteSheet(img);
  }, []);

  const shortLabel = (cwd: string) => cwd.split("/").filter(Boolean).pop() ?? cwd;

  return (
    <div className="app">
      {sessions.length === 0 ? (
        <div className="no-sessions">no sessions</div>
      ) : (
        sessions.map((s) => (
          <CatSession
            key={s.info.sessionId}
            state={s.state}
            label={shortLabel(s.info.cwd)}
            spriteSheet={spriteSheet}
          />
        ))
      )}
    </div>
  );
}
