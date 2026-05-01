#!/usr/bin/env python3
"""
Install Elthen's Cat Sprite Sheet into clowder.

Usage:
  python3 scripts/install-sprites.py ~/Downloads/Cat\ Sprite\ Sheet.png

The script:
  1. Copies the PNG to public/cat-sprites.png
  2. Inspects dimensions to print the detected row layout
  3. Patches ANIM_CONFIG in src/types.ts
"""

import struct
import sys
import shutil
import zlib
from pathlib import Path

PROJECT = Path(__file__).parent.parent
PUBLIC = PROJECT / "public"
TYPES_TS = PROJECT / "src" / "types.ts"
FRAME_SIZE = 32

# Elthen cat sprite sheet row order (from itch.io description):
# Idle x2, clean x2, movement x2, sleep, paw, jump, scared
# Map our CatState → (row, frameCount, fps)
ROW_MAP = {
    #  state      row  fps  (frame count detected from width)
    "idle":     (0,   8),
    "done":     (2,   8),   # clean
    "working":  (4,  14),   # movement
    "thinking": (7,  10),   # paw
    "scared":   (9,  16),   # scared
}


def png_dimensions(path: Path):
    with open(path, "rb") as f:
        sig = f.read(8)
        if sig != b'\x89PNG\r\n\x1a\n':
            raise ValueError("Not a PNG file")
        # IHDR chunk
        f.read(4)           # chunk length
        chunk_type = f.read(4)
        if chunk_type != b'IHDR':
            raise ValueError("Missing IHDR")
        w = struct.unpack('>I', f.read(4))[0]
        h = struct.unpack('>I', f.read(4))[0]
    return w, h


def patch_types_ts(frame_cols: int):
    text = TYPES_TS.read_text()

    lines = []
    for state, (row, fps) in ROW_MAP.items():
        fc = frame_cols  # assume full row; trim to actual if needed
        lines.append(f'  {state:<9} {{ row: {row}, frameCount: {fc}, fps: {fps:>2}  }},')

    new_block = (
        "export const ANIM_CONFIG: Record<CatState, AnimConfig> = {\n"
        + "\n".join(lines)
        + "\n};"
    )

    import re
    patched = re.sub(
        r'export const ANIM_CONFIG: Record<CatState, AnimConfig> = \{.*?\};',
        new_block,
        text,
        flags=re.DOTALL,
    )

    if patched == text:
        print("  [warn] Could not patch ANIM_CONFIG — update src/types.ts manually")
    else:
        TYPES_TS.write_text(patched)
        print(f"  [ok] Patched ANIM_CONFIG in {TYPES_TS.relative_to(PROJECT)}")


def main():
    if len(sys.argv) < 2:
        # Check if file is already in public/
        candidate = PUBLIC / "Cat Sprite Sheet.png"
        if candidate.exists():
            src = candidate
        else:
            print("Usage: python3 scripts/install-sprites.py <path-to-Cat-Sprite-Sheet.png>")
            print("\nOr place the file in public/ and run without arguments.")
            sys.exit(1)
    else:
        src = Path(sys.argv[1]).expanduser()

    if not src.exists():
        print(f"Error: {src} not found")
        sys.exit(1)

    # Copy to public/
    dest = PUBLIC / "cat-sprites.png"
    shutil.copy2(src, dest)
    print(f"[ok] Copied {src.name} → public/cat-sprites.png")

    # Detect dimensions
    w, h = png_dimensions(dest)
    cols = w // FRAME_SIZE
    rows = h // FRAME_SIZE
    print(f"[ok] Sprite sheet: {w}×{h}px → {cols} cols × {rows} rows (32px frames)")
    print(f"\nDetected layout ({cols} frames per row):")
    row_names = ["Idle 1", "Idle 2", "Clean 1", "Clean 2",
                 "Movement 1", "Movement 2", "Sleep", "Paw", "Jump", "Scared"]
    for i, name in enumerate(row_names[:rows]):
        used = next((f"← {s}" for s, (r, _) in ROW_MAP.items() if r == i), "")
        print(f"  Row {i}: {name} {used}")

    # Patch types.ts
    patch_types_ts(cols)

    print("\n[done] Rebuild with: npx tauri build --debug")


if __name__ == "__main__":
    main()
