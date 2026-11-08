#!/usr/bin/env python3
"""Render assets/icon/AppIcon.svg into assets/AppIcon.icns.

Needs: pip install cairosvg pillow
Each size is rendered from the vector source (not downscaled), then gets the
standard macOS icon drop shadow. The .icns is written directly, so this runs
on Linux too; on a Mac `iconutil` is not required.
"""
import io
import struct
import sys
from pathlib import Path

import cairosvg
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SVG = ROOT / "assets/icon/AppIcon.svg"
OUT = ROOT / "assets/AppIcon.icns"

# (icns type, pixel size)
ENTRIES = [
    (b"icp4", 16), (b"ic11", 32),
    (b"icp5", 32), (b"ic12", 64),
    (b"ic07", 128), (b"ic13", 256),
    (b"ic08", 256), (b"ic14", 512),
    (b"ic09", 512), (b"ic10", 1024),
]


def render(px: int) -> Image.Image:
    svg = SVG.read_bytes()
    art = Image.open(io.BytesIO(cairosvg.svg2png(bytestring=svg, output_width=px, output_height=px))).convert("RGBA")
    # macOS template shadow: black 30%, y +10, blur 10 (in 1024 units).
    s = px / 1024
    alpha = art.getchannel("A").point(lambda a: int(a * 0.30))
    shadow = Image.new("RGBA", art.size, (0, 0, 0, 0))
    shadow.putalpha(alpha)
    shadow = shadow.transform(art.size, Image.AFFINE, (1, 0, 0, 0, 1, -10 * s))
    if px >= 64:
        shadow = shadow.filter(ImageFilter.GaussianBlur(10 * s))
    out = Image.new("RGBA", art.size, (0, 0, 0, 0))
    out.alpha_composite(shadow)
    out.alpha_composite(art)
    return out


def png_bytes(img: Image.Image) -> bytes:
    b = io.BytesIO()
    img.save(b, "PNG", optimize=True)
    return b.getvalue()


def main() -> None:
    cache = {px: png_bytes(render(px)) for px in sorted({px for _, px in ENTRIES})}
    body = b"".join(t + struct.pack(">I", len(cache[px]) + 8) + cache[px] for t, px in ENTRIES)
    OUT.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)
    print(f"wrote {OUT.relative_to(ROOT)} ({OUT.stat().st_size // 1024} KB)")
    if len(sys.argv) > 1:  # optional preview: make-icon.py preview.png
        render(1024).save(sys.argv[1])


if __name__ == "__main__":
    main()
