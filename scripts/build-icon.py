#!/usr/bin/env python3
"""
Build the Clowder app icon set from the cat sprite sheet.

Picks the idle-row frame 0 (sitting cat), trims transparent padding, scales
up with nearest-neighbor to keep crisp pixels, then writes:
  - src-tauri/icons/{32x32, 128x128, 128x128@2x}.png
  - src-tauri/icons/icon.icns (full macOS iconset)
  - src-tauri/icons/icon.ico
"""
import shutil, subprocess, tempfile
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
# The colored sprites in public/sprites/ are 64-px-tall horizontal strips.
# The idle strip's first frame is a sitting cat — perfect mascot pose.
SOURCE_STRIP = ROOT / "public" / "sprites" / "idle.png"
FRAME_SIZE = 64
FRAME_INDEX = 0
ICON_DIR = ROOT / "src-tauri" / "icons"

def trim_alpha(img: Image.Image, threshold: int = 10) -> Image.Image:
    """Crop transparent borders so the cat fills the frame."""
    a = img.split()[-1]
    bbox = a.point(lambda p: 255 if p > threshold else 0).getbbox()
    return img.crop(bbox) if bbox else img

def square_pad(img: Image.Image, scale: float = 0.78) -> Image.Image:
    """Place the cat centered in a transparent square, leaving some margin."""
    w, h = img.size
    side = max(w, h)
    canvas_side = round(side / scale)
    canvas = Image.new("RGBA", (canvas_side, canvas_side), (0, 0, 0, 0))
    x = (canvas_side - w) // 2
    y = (canvas_side - h) // 2
    canvas.paste(img, (x, y), img)
    return canvas

def main() -> None:
    strip = Image.open(SOURCE_STRIP).convert("RGBA")
    x0 = FRAME_INDEX * FRAME_SIZE
    frame = strip.crop((x0, 0, x0 + FRAME_SIZE, FRAME_SIZE))
    cat = trim_alpha(frame)
    padded = square_pad(cat, scale=0.78)

    # Master size for all downstream resampling
    master = padded.resize((1024, 1024), Image.NEAREST)

    ICON_DIR.mkdir(parents=True, exist_ok=True)

    # Tauri-required PNGs
    master.resize((32, 32), Image.NEAREST).save(ICON_DIR / "32x32.png")
    master.resize((128, 128), Image.NEAREST).save(ICON_DIR / "128x128.png")
    master.resize((256, 256), Image.NEAREST).save(ICON_DIR / "128x128@2x.png")

    # macOS .icns via iconutil
    sizes = [
        ("16x16",       16),  ("16x16@2x",     32),
        ("32x32",       32),  ("32x32@2x",     64),
        ("128x128",    128),  ("128x128@2x",  256),
        ("256x256",    256),  ("256x256@2x",  512),
        ("512x512",    512),  ("512x512@2x", 1024),
    ]
    with tempfile.TemporaryDirectory() as td:
        iconset = Path(td) / "clowder.iconset"
        iconset.mkdir()
        for name, size in sizes:
            master.resize((size, size), Image.NEAREST).save(iconset / f"icon_{name}.png")
        subprocess.run(
            ["iconutil", "-c", "icns", str(iconset), "-o", str(ICON_DIR / "icon.icns")],
            check=True,
        )

    # Windows .ico (multi-resolution)
    ico_sizes = [(16,16), (32,32), (48,48), (64,64), (128,128), (256,256)]
    master.save(ICON_DIR / "icon.ico", format="ICO", sizes=ico_sizes)

    print("wrote:")
    for p in sorted(ICON_DIR.iterdir()):
        print(f"  {p.name:<20} {p.stat().st_size:>8} B")

if __name__ == "__main__":
    main()
