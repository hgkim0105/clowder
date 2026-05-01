import { useEffect, useRef, useState } from "react";
import { Cat } from "./components/Cat";
import { useSessions } from "./hooks/useSessions";
import type { CatState } from "./types";
import { SPRITE_SHEET_URL } from "./types";
import "./App.css";

const STATE_PRIORITY: CatState[] = ["working", "thinking", "scared", "done", "idle"];

function dominantState(states: CatState[]): CatState {
  for (const s of STATE_PRIORITY) {
    if (states.includes(s)) return s;
  }
  return "idle";
}

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
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [rawState]);

  return state;
}

export default function App() {
  const sessions = useSessions();
  const [spriteSheet, setSpriteSheet] = useState<HTMLImageElement | null>(null);

  useEffect(() => {
    const img = new Image();
    img.src = SPRITE_SHEET_URL;
    img.onload = () => setSpriteSheet(img);
  }, []);

  const rawState = dominantState(sessions.map((s) => s.state as CatState));
  const displayState = useDisplayState(rawState);

  return (
    <div className="app">
      <Cat state={displayState} spriteSheet={spriteSheet} />
    </div>
  );
}
