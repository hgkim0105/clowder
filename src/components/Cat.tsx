import { useEffect, useRef } from "react";
import type { CatState } from "../types";
import { ANIM_CONFIG, FRAME_SIZE, DISPLAY_SCALE } from "../types";

interface CatProps {
  state: CatState;
  spriteSheet: HTMLImageElement | null;
  label?: string;
}

const SPRITE_SIZE = FRAME_SIZE * DISPLAY_SCALE;

export function Cat({ state, spriteSheet, label }: CatProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frameRef = useRef(0);
  const animRef = useRef<number>(0);
  const lastTimeRef = useRef(0);

  useEffect(() => {
    frameRef.current = 0;
  }, [state]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const config = ANIM_CONFIG[state];
    const interval = 1000 / config.fps;

    const render = (timestamp: number) => {
      if (timestamp - lastTimeRef.current >= interval) {
        lastTimeRef.current = timestamp;
        frameRef.current = (frameRef.current + 1) % config.frameCount;

        ctx.clearRect(0, 0, canvas.width, canvas.height);

        if (spriteSheet) {
          ctx.imageSmoothingEnabled = false;
          ctx.drawImage(
            spriteSheet,
            frameRef.current * FRAME_SIZE,
            config.row * FRAME_SIZE,
            FRAME_SIZE,
            FRAME_SIZE,
            0,
            0,
            SPRITE_SIZE,
            SPRITE_SIZE
          );
        }
      }
      animRef.current = requestAnimationFrame(render);
    };

    animRef.current = requestAnimationFrame(render);
    return () => cancelAnimationFrame(animRef.current);
  }, [state, spriteSheet]);

  return (
    <div className="cat-wrapper">
      <canvas
        ref={canvasRef}
        width={SPRITE_SIZE}
        height={SPRITE_SIZE}
        className="cat-canvas"
      />
      {label && <div className="cat-label">{label}</div>}
    </div>
  );
}
